//! The non-destructive edit pipeline: turns a photo ([`DevelopSource`])
//! plus a `DevelopSettings` into a rendered image, in these stages:
//!
//! 0. for RAW files, demosaicing and development (white balance, exposure,
//!    highlight recovery, the camera's colour); `retouch`: spot removal
//!    (heal / clone) on the original photo; then `lens`: distortion,
//!    chromatic aberration and vignetting corrections;
//! 1. `geometry`: flips and rotation, then `denoise`: noise reduction;
//! 2. the base stage (`tone`, `color`, `detail`); sharpening with a custom
//!    radius or edge masking runs after it, in `sharpen`;
//! 3. `finishing` (before the crop): tone curves, Color Mixer, Color
//!    Grading, Dehaze and gradient masks;
//! 4. `crop`: straighten and crop;
//! 5. `finishing` (after the crop): Vignette and Grain, framed by the crop;
//! 6. conversion from the working colour space (`cms`) into the output's.
//!
//! Each stage is skipped while its settings are neutral.
//!
//! Every stage runs on the GPU when one is available (`gpu`), in 16-bit
//! floats from start to finish. The CPU modules are the reference and the
//! fallback; they work in 8 bits between stages.

mod auto;
mod cms;
mod color;
mod crop;
mod denoise;
mod detail;
mod develop_source;
mod finishing;
mod frame;
mod geometry;
mod gpu;
mod lens;
mod look;
mod maskview;
mod parallel;
mod proof;
mod retouch;
mod sharpen;
mod tone;
mod upright;
mod watermark;

pub use auto::{auto_tone, neutral_white_balance, sample_colour, BalanceError, NeutralBalance};
pub use cms::{is_proofing_profile, set_display_profile, OutputSpace};
pub use proof::{apply_proof, ProofIntent, ProofSettings};
pub use maskview::overlay_mask;
pub use look::{match_look, preset_from_look, LookStats};
pub use frame::{frame_bgra, frame_rgb16, frame_rgb8, framed_size};
pub use watermark::apply_watermark;
pub use develop_source::DevelopSource;
pub use retouch::find_spot_source;
pub use upright::{detect as detect_upright, Upright, UprightMode};

use image::{imageops::FilterType, DynamicImage, RgbImage, RgbaImage};
use rustyedit_core::{DevelopSettings, Histogram};

/// Applies the full develop pipeline to `source`, returning a new sRGB
/// image. `source` is never mutated, so callers can keep it around and
/// re-render cheaply whenever a slider changes.
pub fn apply(source: &DevelopSource, settings: &DevelopSettings) -> RgbImage {
    render_export(source, settings, None, None, &OutputSpace::Srgb)
}

/// [`apply`] into `space`, then fitted within `long_edge` (Lanczos3) and
/// sharpened for output (`image::imageops::unsharpen` sigma and
/// threshold), all in one GPU pass when possible. Embed `space.icc()` in
/// the file written.
pub fn render_export(
    source: &DevelopSource,
    settings: &DevelopSettings,
    long_edge: Option<u32>,
    sharpen: Option<(f32, i32)>,
    space: &OutputSpace,
) -> RgbImage {
    // Sections switched off in the panel render as if untouched.
    let settings = &settings.effective();
    let output = gpu::Output::Rgb {
        fit: long_edge,
        sharpen,
        space: space.clone(),
        deep: false,
    };
    if let Some(gpu::Rendered::Rgb(image)) = gpu::render(source, settings, output) {
        return image;
    }
    let mut rgb = apply_cpu(source, settings);
    if let Some(edge) = long_edge {
        if rgb.width().max(rgb.height()) > edge {
            let (w, h) = fit_within(rgb.width(), rgb.height(), edge);
            rgb = image::imageops::resize(&rgb, w, h, FilterType::Lanczos3);
        }
    }
    if let Some((sigma, threshold)) = sharpen {
        rgb = image::imageops::unsharpen(&rgb, sigma, threshold);
    }
    if let Some(lut) = cms::lut(space) {
        lut.apply_rgb8(&mut rgb);
    }
    rgb
}

/// [`render_export`] at 16 bits per channel, for PNG and TIFF files. The
/// whole pipeline runs in floating point and the conversion into `space`
/// happens just before the 16-bit quantisation, so wide spaces like
/// ProPhoto don't band. Without a GPU the 8-bit render is widened instead
/// (still correctly tagged, just not smoother).
pub fn render_export16(
    source: &DevelopSource,
    settings: &DevelopSettings,
    long_edge: Option<u32>,
    sharpen: Option<(f32, i32)>,
    space: &OutputSpace,
) -> image::ImageBuffer<image::Rgb<u16>, Vec<u16>> {
    let effective = settings.effective();
    let output = gpu::Output::Rgb {
        fit: long_edge,
        sharpen,
        space: space.clone(),
        deep: true,
    };
    if let Some(gpu::Rendered::Rgb16(image)) = gpu::render(source, &effective, output) {
        return image;
    }
    let rgb = render_export(source, settings, long_edge, sharpen, space);
    let (w, h) = rgb.dimensions();
    let samples: Vec<u16> = rgb.as_raw().iter().map(|&v| v as u16 * 257).collect();
    image::ImageBuffer::from_raw(w, h, samples).expect("same number of samples")
}

/// The size `width` x `height` scales to so its longest edge is `edge`,
/// keeping the aspect ratio as `image`'s `DynamicImage::resize` does.
pub(crate) fn fit_within(width: u32, height: u32, edge: u32) -> (u32, u32) {
    let ratio = f64::min(edge as f64 / width as f64, edge as f64 / height as f64);
    (
        ((width as f64 * ratio).round() as u32).max(1),
        ((height as f64 * ratio).round() as u32).max(1),
    )
}

/// Which clipped pixels the live preview paints over (toggled from the
/// histogram, or with J).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClipWarnings {
    /// Pixels at (near) pure black in every channel, painted blue.
    pub shadows: bool,
    /// Pixels at (near) pure white in any channel, painted red.
    pub highlights: bool,
}

impl ClipWarnings {
    /// The warning colour for an RGB pixel, if it's clipped. A little
    /// tolerance, since JPEG sources rarely hit 0 or 255 exactly.
    fn colour(&self, [r, g, b]: [u8; 3]) -> Option<[u8; 3]> {
        let max = r.max(g).max(b);
        if self.highlights && max >= 254 {
            Some([255, 40, 40])
        } else if self.shadows && max <= 2 {
            Some([40, 110, 255])
        } else {
            None
        }
    }
}

/// A live-preview frame: what the viewer shows plus the histogram and
/// scopes of the render (taken before the clipping warnings are painted).
pub struct PreviewFrame {
    /// BGRA pixels (in an `RgbaImage` container) in the display's colour
    /// space, ready for the UI.
    pub bgra: RgbaImage,
    pub histogram: Histogram,
}

/// Renders a live-preview frame. On the GPU this also builds the display
/// frame and the histogram there, leaving no per-pixel work for the CPU.
pub fn render_preview(
    source: &DevelopSource,
    settings: &DevelopSettings,
    clip: ClipWarnings,
) -> PreviewFrame {
    let settings = &settings.effective();
    let output = gpu::Output::Preview {
        clip_shadows: clip.shadows,
        clip_highlights: clip.highlights,
    };
    if let Some(gpu::Rendered::Preview { bgra, histogram }) = gpu::render(source, settings, output)
    {
        return PreviewFrame { bgra, histogram };
    }
    let rendered = apply_cpu(source, settings);
    let histogram = Histogram::from_rgb8_sized(rendered.as_raw(), rendered.width() as usize);
    PreviewFrame {
        bgra: to_display_bgra(&rendered, clip),
        histogram,
    }
}

/// Renders part of the photo at full resolution, for the zoomed-in view:
/// `region` is left, top, width and height as fractions of the cropped
/// photo, `size` the pixel size to deliver it at (at most the region's
/// own). BGRA in the display's colour space.
pub fn render_detail(
    source: &DevelopSource,
    settings: &DevelopSettings,
    region: (f32, f32, f32, f32),
    size: (u32, u32),
) -> RgbaImage {
    let settings = &settings.effective();
    let output = gpu::Output::Detail { region, size };
    if let Some(gpu::Rendered::Detail(bgra)) = gpu::render(source, settings, output) {
        return bgra;
    }
    // The slow way: render everything, then cut the region out.
    let full = apply_cpu(source, settings);
    let (w, h) = (full.width() as f32, full.height() as f32);
    let x0 = ((region.0 * w).floor() as u32).min(full.width() - 1);
    let y0 = ((region.1 * h).floor() as u32).min(full.height() - 1);
    let rw = (((region.0 + region.2) * w).ceil() as u32).clamp(x0 + 1, full.width()) - x0;
    let rh = (((region.1 + region.3) * h).ceil() as u32).clamp(y0 + 1, full.height()) - y0;
    let mut part = image::imageops::crop_imm(&full, x0, y0, rw, rh).to_image();
    let (tw, th) = (size.0.clamp(1, rw), size.1.clamp(1, rh));
    if (tw, th) != (rw, rh) {
        part = image::imageops::resize(&part, tw, th, FilterType::Lanczos3);
    }
    to_display_bgra(&part, ClipWarnings::default())
}

/// Converts a working-space render into BGRA for the display, painting
/// clipped pixels (judged on the working-space values) when asked.
fn to_display_bgra(rgb: &RgbImage, clip: ClipWarnings) -> RgbaImage {
    let (width, height) = rgb.dimensions();
    let mut display = rgb.clone();
    if let Some(lut) = cms::lut(&cms::display_space()) {
        lut.apply_rgb8(&mut display);
    }
    let mut bgra = Vec::with_capacity(width as usize * height as usize * 4);
    let overlay = clip.shadows || clip.highlights;
    for (working, shown) in rgb.as_raw().chunks_exact(3).zip(display.as_raw().chunks_exact(3)) {
        let shown = [shown[0], shown[1], shown[2]];
        let [r, g, b] = if overlay {
            clip.colour([working[0], working[1], working[2]])
                .unwrap_or(shown)
        } else {
            shown
        };
        bgra.extend_from_slice(&[b, g, r, 255]);
    }
    RgbaImage::from_raw(width, height, bgra).expect("buffer is exactly width * height * 4 bytes")
}

/// The CPU pipeline, in the working space. `settings` must already be
/// `effective()`.
fn apply_cpu(source: &DevelopSource, settings: &DevelopSettings) -> RgbImage {
    let image = source.cpu_image();
    let source: &DynamicImage = &image;
    let retouched;
    let source = if settings.spots.is_empty() {
        source
    } else {
        let mut rgb = source.to_rgb8();
        retouch::apply_spots(&mut rgb, &settings.spots);
        retouched = DynamicImage::ImageRgb8(rgb);
        &retouched
    };
    let corrected;
    let source = if lens::is_active(settings) {
        corrected = DynamicImage::ImageRgb8(lens::correct(&source.to_rgb8(), settings));
        &corrected
    } else {
        source
    };
    let rotated = geometry::apply_geometry(source, settings);
    let mut buffer = rotated.to_rgb8();

    // Noise comes out first, before tone changes can amplify it.
    denoise::reduce_noise(&mut buffer, settings.noise_luma, settings.noise_color);

    let (base, custom_sharpen) = split_sharpening(settings);
    if has_base_edits(&base) {
        for pixel in buffer.pixels_mut() {
            let with_tone = tone::apply_tone_rgb8(*pixel, &base);
            *pixel = color::apply_color_rgb8(with_tone, &base);
        }
        detail::apply_detail(&mut buffer, &base);
    }
    if custom_sharpen {
        sharpen::sharpen(
            &mut buffer,
            settings.sharpening,
            settings.sharpen_radius,
            settings.sharpen_masking,
        );
    }
    if let Some(finishing) = finishing::Finishing::before_crop(settings, &buffer) {
        finishing.apply(&mut buffer);
    }
    let mut buffer = crop::straighten_and_crop(buffer, &settings.crop);
    let (width, height) = buffer.dimensions();
    if let Some(finishing) = finishing::Finishing::after_crop(settings, width, height) {
        finishing.apply(&mut buffer);
    }
    buffer
}

/// The settings for the base stage, and whether sharpening instead runs
/// after it (`sharpen`): sharpening with a custom radius or an edge mask
/// isn't part of the base stage.
pub(crate) fn split_sharpening(settings: &DevelopSettings) -> (DevelopSettings, bool) {
    let custom = settings.sharpening > 0.5
        && (settings.sharpen_masking > 0.5 || (settings.sharpen_radius - 50.0).abs() > 0.5);
    let base = DevelopSettings {
        sharpening: if custom { 0.0 } else { settings.sharpening },
        ..*settings
    };
    (base, custom)
}

/// Whether any of the base stage's sliders is off neutral.
pub(crate) fn has_base_edits(s: &DevelopSettings) -> bool {
    [
        s.exposure,
        s.contrast,
        s.highlights,
        s.shadows,
        s.whites,
        s.blacks,
        s.temperature,
        s.tint,
        s.vibrance,
        s.saturation,
        s.clarity,
        s.sharpening,
    ]
    .iter()
    .any(|v| *v != 0.0)
}

/// Name of the GPU the develop pipeline runs on, or `None` when it falls
/// back to the CPU. Initialises the device on first call.
pub use gpu::adapter_name as gpu_adapter_name;

/// The "smart preview" the live preview renders from: `source` with its
/// long edge at most `max_edge`. For RAW files it's demosaiced on the GPU
/// and kept as linear camera data, so white balance and exposure still
/// work like the camera's.
pub fn prepare_preview(source: &DevelopSource, max_edge: u32) -> DevelopSource {
    if let Some(preview) = gpu::preview_source(source, max_edge) {
        return preview;
    }
    let (w, h) = (source.width(), source.height());
    if w <= max_edge && h <= max_edge && !source.is_raw() {
        return source.clone();
    }
    let image = source.cpu_image();
    let (nw, nh) = fit_within(w, h, max_edge.min(w.max(h)));
    DevelopSource::from_working(image.resize_exact(nw, nh, FilterType::Triangle))
}

/// Downscales `image` so its longest edge is at most `max_edge`, using a
/// fast filter (on the GPU when possible). For thumbnails; no colour
/// conversion.
pub fn downscale_for_preview(source: &DynamicImage, max_edge: u32) -> DynamicImage {
    let (width, height) = (source.width(), source.height());
    if width <= max_edge && height <= max_edge {
        return source.clone();
    }
    let (new_width, new_height) = fit_within(width, height, max_edge);
    if let Some(resized) = gpu::resize(source, new_width, new_height) {
        return DynamicImage::ImageRgb8(resized);
    }
    source.resize(max_edge, max_edge, FilterType::Triangle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyedit_core::{Crop, Gradient, GradientKind, LocalAdjust, Spot, SpotKind};

    /// A photo-like test card: smooth gradients, saturated patches, hard
    /// edges and noise, at an odd size so edges of the GPU grid get hit.
    fn card(width: u32, height: u32) -> DevelopSource {
        DevelopSource::from_working(card_image(width, height))
    }

    fn card_image(width: u32, height: u32) -> DynamicImage {
        let img = RgbImage::from_fn(width, height, |x, y| {
            let noise = ((x.wrapping_mul(7919) ^ y.wrapping_mul(104_729)) % 23) as i32 - 11;
            let base = match (x * 4 / width, y * 3 / height) {
                (0, _) => [x * 255 / width, y * 255 / height, 128],
                (1, 0) => [220, 40, 30],
                (1, 1) => [30, 200, 60],
                (1, _) => [40, 60, 230],
                (2, _) if (x / 9 + y / 9) % 2 == 0 => [245, 245, 240],
                (2, _) => [12, 10, 14],
                _ => [200, 150 + (y * 60 / height), 90],
            };
            image::Rgb(base.map(|c| (c as i32 + noise).clamp(0, 255) as u8))
        });
        DynamicImage::ImageRgb8(img)
    }

    fn gpu_available() -> bool {
        match gpu_adapter_name() {
            Some(name) => {
                eprintln!("GPU: {name}");
                true
            }
            None => {
                eprintln!("no GPU available; skipping");
                false
            }
        }
    }

    /// Renders on both paths and checks they agree: same size, tiny mean
    /// difference, and almost no pixel off by more than a couple of levels.
    fn working_rgb(source: &DevelopSource, settings: &DevelopSettings) -> Option<RgbImage> {
        let output = gpu::Output::Rgb {
            fit: None,
            sharpen: None,
            space: OutputSpace::DisplayP3,
            deep: false,
        };
        match gpu::render(source, &settings.effective(), output)? {
            gpu::Rendered::Rgb(image) => Some(image),
            _ => None,
        }
    }

    /// Renders on both paths and checks they agree. The GPU keeps 16-bit
    /// floats between stages where the CPU rounds to 8 bits, so they can
    /// differ by a level or two, and a little more where a threshold
    /// (unsharp masking) or a hue boundary tips one way or the other.
    fn assert_parity(name: &str, source: &DevelopSource, settings: &DevelopSettings) {
        let effective = settings.effective();
        let gpu = match working_rgb(source, settings) {
            Some(image) => image,
            None => panic!("{name}: GPU render failed"),
        };
        let cpu = apply_cpu(source, &effective);
        assert_eq!(gpu.dimensions(), cpu.dimensions(), "{name}: size");
        let diffs: Vec<u8> = gpu
            .as_raw()
            .iter()
            .zip(cpu.as_raw())
            .map(|(a, b)| a.abs_diff(*b))
            .collect();
        let mean = diffs.iter().map(|d| *d as f64).sum::<f64>() / diffs.len() as f64;
        let max = *diffs.iter().max().unwrap();
        let off = diffs.iter().filter(|d| **d > 3).count() as f64 / diffs.len() as f64;
        eprintln!("{name}: mean {mean:.4}, max {max}, >3 off {:.4}%", off * 100.0);
        assert!(mean < 2.0, "{name}: mean diff {mean}");
        assert!(off < 0.2, "{name}: {:.3}% of values off by > 3", off * 100.0);
    }

    fn with(f: impl Fn(&mut DevelopSettings)) -> DevelopSettings {
        let mut s = DevelopSettings::default();
        f(&mut s);
        s
    }

    #[test]
    fn gpu_matches_cpu_stage_by_stage() {
        if !gpu_available() {
            return;
        }
        let source = card(517, 389);
        let cases: Vec<(&str, DevelopSettings)> = vec![
            ("neutral", DevelopSettings::default()),
            (
                "basic",
                with(|s| {
                    s.exposure = 20.0;
                    s.contrast = 15.0;
                    s.highlights = -30.0;
                    s.shadows = 25.0;
                    s.whites = 10.0;
                    s.blacks = -10.0;
                    s.temperature = 20.0;
                    s.tint = -10.0;
                    s.vibrance = 30.0;
                    s.saturation = 15.0;
                    s.clarity = 40.0;
                    s.sharpening = 50.0;
                }),
            ),
            ("negative clarity", with(|s| s.clarity = -60.0)),
            (
                "custom sharpening",
                with(|s| {
                    s.sharpening = 80.0;
                    s.sharpen_radius = 70.0;
                    s.sharpen_masking = 40.0;
                }),
            ),
            (
                "unmasked sharpening",
                with(|s| {
                    s.sharpening = 60.0;
                    s.sharpen_radius = 20.0;
                }),
            ),
            (
                "noise reduction",
                with(|s| {
                    s.noise_luma = 70.0;
                    s.noise_color = 50.0;
                }),
            ),
            ("colour noise only", with(|s| s.noise_color = 90.0)),
            (
                "lens",
                with(|s| {
                    s.lens_distortion = 60.0;
                    s.lens_ca = -80.0;
                    s.lens_vignetting = 50.0;
                }),
            ),
            (
                "rotate 90 + flip",
                with(|s| {
                    s.rotation_degrees = 90.0;
                    s.flip_horizontal = true;
                }),
            ),
            (
                "rotate 270 + flip v",
                with(|s| {
                    s.rotation_degrees = -90.0;
                    s.flip_vertical = true;
                }),
            ),
            ("rotate 180", with(|s| s.rotation_degrees = 180.0)),
            (
                "curves",
                with(|s| {
                    s.curves.master.move_point(1, 1.0, 0.8);
                    s.curves.red.move_point(0, 0.0, 0.1);
                    s.parametric.shadows = 40.0;
                    s.parametric.highlights = -30.0;
                }),
            ),
            (
                "mixer",
                with(|s| {
                    for (i, band) in s.mixer.iter_mut().enumerate() {
                        band.hue = (i as f32 * 25.0) - 90.0;
                        band.saturation = 60.0 - i as f32 * 15.0;
                        band.luminance = -40.0 + i as f32 * 10.0;
                    }
                }),
            ),
            (
                "grading",
                with(|s| {
                    s.grading.shadows.hue = 220.0;
                    s.grading.shadows.saturation = 60.0;
                    s.grading.midtones.hue = 40.0;
                    s.grading.midtones.saturation = 30.0;
                    s.grading.highlights.hue = 50.0;
                    s.grading.highlights.saturation = 80.0;
                    s.grading.balance = 20.0;
                }),
            ),
            (
                "brush and range masks",
                with(|s| {
                    let mut brush = Gradient {
                        kind: GradientKind::Brush,
                        feather: 0.6,
                        adjust: LocalAdjust {
                            exposure: 50.0,
                            saturation: 40.0,
                            ..Default::default()
                        },
                        ..Default::default()
                    };
                    for i in 0..12 {
                        brush.add_dab(rustyedit_core::Dab {
                            x: 0.2 + i as f32 * 0.05,
                            y: 0.4 + (i as f32 * 0.7).sin() * 0.1,
                            radius: 0.08,
                            erase: i == 7,
                        });
                    }
                    s.gradients.push(brush);
                    s.gradients.push(Gradient {
                        kind: GradientKind::Range,
                        luma_range: (0.0, 0.5),
                        luma_feather: 0.2,
                        hue_on: true,
                        hue: 30.0,
                        hue_width: 60.0,
                        adjust: LocalAdjust {
                            exposure: -40.0,
                            contrast: 25.0,
                            ..Default::default()
                        },
                        ..Default::default()
                    });
                }),
            ),
            ("dehaze", with(|s| s.dehaze = 70.0)),
            ("haze", with(|s| s.dehaze = -50.0)),
            (
                "gradients",
                with(|s| {
                    s.gradients.push(Gradient {
                        kind: GradientKind::Linear,
                        start: (0.5, 0.0),
                        end: (0.5, 0.6),
                        adjust: LocalAdjust {
                            exposure: -60.0,
                            contrast: 30.0,
                            temperature: -40.0,
                            dehaze: 50.0,
                            sharpening: 70.0,
                            ..Default::default()
                        },
                        opacity: 100.0,
                        ..Default::default()
                    });
                    s.gradients.push(Gradient {
                        kind: GradientKind::Radial,
                        start: (0.4, 0.6),
                        radius: (0.3, 0.2),
                        feather: 0.5,
                        invert: true,
                        adjust: LocalAdjust {
                            shadows: 50.0,
                            highlights: -40.0,
                            saturation: -50.0,
                            tint: 30.0,
                            clarity: 80.0,
                            grading: {
                                let mut g = rustyedit_core::ColorGrading::default();
                                g.midtones.hue = 200.0;
                                g.midtones.saturation = 70.0;
                                g
                            },
                            ..Default::default()
                        },
                        opacity: 80.0,
                        ..Default::default()
                    });
                }),
            ),
            (
                "crop",
                with(|s| {
                    s.crop = Crop {
                        rect: (0.1, 0.15, 0.7, 0.6),
                        ..Default::default()
                    }
                }),
            ),
            (
                "perspective crop",
                with(|s| {
                    s.crop = Crop {
                        rect: (0.1, 0.1, 0.8, 0.8),
                        angle: -2.0,
                        vertical: 45.0,
                        horizontal: -30.0,
                        ..Default::default()
                    }
                }),
            ),
            (
                "straightened crop",
                with(|s| {
                    s.crop = Crop {
                        rect: (0.05, 0.1, 0.8, 0.75),
                        angle: 7.5,
                        ..Default::default()
                    }
                }),
            ),
            (
                "vignette and grain",
                with(|s| {
                    s.vignette = -60.0;
                    s.vignette_midpoint = 30.0;
                    s.grain = 50.0;
                    s.grain_size = 60.0;
                }),
            ),
            ("light vignette", with(|s| s.vignette = 70.0)),
            (
                "spots",
                with(|s| {
                    s.spots.push(Spot {
                        kind: SpotKind::Heal,
                        target: (0.3, 0.4),
                        source: (0.6, 0.5),
                        radius: 0.05,
                        feather: 0.5,
                        opacity: 1.0,
                    });
                }),
            ),
        ];
        let mut everything = DevelopSettings::default();
        for (_, case) in &cases {
            everything = merge(everything, case);
        }
        for (name, settings) in &cases {
            assert_parity(name, &source, settings);
        }
        assert_parity("everything", &source, &everything);
    }

    /// `a` with every non-default field of `b` copied over.
    fn merge(mut a: DevelopSettings, b: &DevelopSettings) -> DevelopSettings {
        let d = DevelopSettings::default();
        macro_rules! take {
            ($($f:ident),*) => { $( if b.$f != d.$f { a.$f = b.$f; } )* };
        }
        take!(
            exposure, contrast, highlights, shadows, whites, blacks, temperature, tint,
            vibrance, saturation, clarity, sharpening, sharpen_radius, sharpen_masking,
            noise_luma, noise_color, lens_distortion, lens_vignetting, lens_ca, spots,
            gradients, curves, parametric, mixer, grading, dehaze, vignette,
            vignette_midpoint, vignette_feather, grain, grain_size, crop,
            rotation_degrees, flip_horizontal, flip_vertical
        );
        a
    }

    #[test]
    fn preview_frame_and_histogram_match_cpu() {
        if !gpu_available() {
            return;
        }
        let source = card(400, 300);
        let settings = with(|s| {
            s.exposure = 40.0;
            s.blacks = -60.0;
            s.contrast = 30.0;
        });
        let clip = ClipWarnings {
            shadows: true,
            highlights: true,
        };
        let frame = render_preview(&source, &settings, clip);
        let rendered = apply_cpu(&source, &settings.effective());
        let expected = to_display_bgra(&rendered, clip);
        assert_eq!(frame.bgra.dimensions(), expected.dimensions());
        let differing = frame
            .bgra
            .as_raw()
            .iter()
            .zip(expected.as_raw())
            .filter(|(a, b)| a.abs_diff(**b) > 2)
            .count();
        assert!(differing < 600, "{differing} BGRA values differ");
        // Some pixels clip, so the warnings were painted.
        assert!(frame
            .bgra
            .pixels()
            .any(|p| p.0 == [40, 40, 255, 255] || p.0 == [255, 110, 40, 255]));

        let cpu = Histogram::from_rgb8_sized(rendered.as_raw(), rendered.width() as usize);
        let gpu = &frame.histogram;
        assert_eq!(gpu.total(), cpu.total());
        let bins_off: u32 = (0..256)
            .map(|i| gpu.red[i].abs_diff(cpu.red[i]) + gpu.luma[i].abs_diff(cpu.luma[i]))
            .sum();
        assert!(bins_off < 4000, "histograms differ by {bins_off}");
        let sum = |v: &[u32]| v.iter().map(|n| *n as u64).sum::<u64>();
        assert_eq!(sum(&gpu.scopes.vector), sum(&cpu.scopes.vector));
        assert_eq!(sum(&gpu.scopes.wave_luma), sum(&cpu.scopes.wave_luma));
    }

    #[test]
    fn cached_renders_follow_setting_and_photo_changes() {
        if !gpu_available() {
            return;
        }
        let a = card(300, 200);
        let b = DevelopSource::from_working(DynamicImage::ImageRgb8(RgbImage::from_pixel(
            300,
            200,
            image::Rgb([90, 90, 90]),
        )));
        let noisy = with(|s| s.noise_luma = 80.0);
        let none = ClipWarnings::default();
        // Same photo, early stage changes, then the cached early stage is
        // reused with later edits; then a different photo of the same size.
        for (source, settings) in [
            (&a, noisy),
            (&a, DevelopSettings::default()),
            (&a, with(|s| s.exposure = 30.0)),
            (&a, DevelopSettings { exposure: 30.0, ..noisy }),
            (&b, DevelopSettings { exposure: 30.0, ..noisy }),
        ] {
            let frame = render_preview(source, &settings, none);
            let expected = to_display_bgra(&apply_cpu(source, &settings), none);
            let mean = frame
                .bgra
                .as_raw()
                .iter()
                .zip(expected.as_raw())
                .map(|(x, y)| x.abs_diff(*y) as f64)
                .sum::<f64>()
                / expected.as_raw().len() as f64;
            assert!(mean < 0.75, "mean diff {mean}");
        }
    }

    #[test]
    fn sixteen_bit_export_matches_eight_bit_and_has_finer_steps() {
        if !gpu_available() {
            return;
        }
        let source = card(517, 389);
        let settings = with(|s| {
            s.exposure = 12.0;
            s.contrast = 10.0;
        });
        for space in [OutputSpace::Srgb, OutputSpace::AdobeRgb, OutputSpace::ProPhoto] {
            let eight = render_export(&source, &settings, None, None, &space);
            let deep = render_export16(&source, &settings, None, None, &space);
            assert_eq!(eight.dimensions(), deep.dimensions(), "{space:?}");
            // Same picture: the 16-bit values, scaled down, agree with the
            // 8-bit ones to within rounding.
            let worst = eight
                .as_raw()
                .iter()
                .zip(deep.as_raw())
                .map(|(a, b)| (*a as i32 - (*b as i32 + 128) / 257).abs())
                .max()
                .unwrap();
            assert!(worst <= 2, "{space:?}: worst difference {worst}");
            // And it really has more than 256 levels per channel.
            let mut distinct: Vec<u16> = deep.as_raw().iter().copied().collect();
            distinct.sort_unstable();
            distinct.dedup();
            assert!(distinct.len() > 400, "{space:?}: {} distinct levels", distinct.len());
        }
    }

    #[test]
    fn export_resize_and_sharpening_match_cpu() {
        if !gpu_available() {
            return;
        }
        let source = card(1203, 801);
        let settings = with(|s| s.exposure = 15.0);
        let off_by_more_than = |sharpen: Option<(f32, i32)>, limit: u8| {
            let gpu = render_export(&source, &settings, Some(500), sharpen, &OutputSpace::DisplayP3);
            let mut cpu = apply_cpu(&source, &settings.effective());
            cpu = image::imageops::resize(&cpu, 500, 333, image::imageops::FilterType::Lanczos3);
            if let Some((sigma, threshold)) = sharpen {
                cpu = image::imageops::unsharpen(&cpu, sigma, threshold);
            }
            assert_eq!(gpu.dimensions(), cpu.dimensions());
            let off = gpu
                .as_raw()
                .iter()
                .zip(cpu.as_raw())
                .filter(|(a, b)| a.abs_diff(**b) > limit)
                .count();
            off as f64 / cpu.as_raw().len() as f64
        };
        // The resize itself agrees closely.
        let resized = off_by_more_than(None, 2);
        assert!(resized < 0.001, "{:.3}% off after resizing", resized * 100.0);
        // A 2-level sharpening threshold flips wherever the 8-bit CPU
        // values and the float GPU ones fall either side of it.
        let sharpened = off_by_more_than(Some((1.0, 2)), 3);
        assert!(sharpened < 0.1, "{:.3}% off after sharpening", sharpened * 100.0);
    }

    #[test]
    fn gpu_downscale_matches_cpu() {
        if !gpu_available() {
            return;
        }
        let source = card_image(1203, 801);
        let gpu = downscale_for_preview(&source, 512).to_rgb8();
        let cpu = source
            .resize(512, 512, image::imageops::FilterType::Triangle)
            .to_rgb8();
        assert_eq!(gpu.dimensions(), cpu.dimensions());
        let max = gpu
            .as_raw()
            .iter()
            .zip(cpu.as_raw())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(max <= 2, "max diff {max}");
    }

    #[test]
    #[ignore = "benchmark: cargo test --release -p rustyedit-processing -- --ignored --nocapture"]
    fn benchmark_gpu_vs_cpu() {
        if !gpu_available() {
            return;
        }
        let source = card(2560, 1706);
        let settings = with(|s| {
            s.exposure = 20.0;
            s.clarity = 40.0;
            s.noise_luma = 40.0;
            s.noise_color = 40.0;
            s.dehaze = 20.0;
            s.vignette = -30.0;
            s.grading.shadows.saturation = 30.0;
            s.curves.master.move_point(1, 1.0, 0.9);
            s.crop.angle = 3.0;
        });
        let effective = settings.effective();
        let time = |label: &str, f: &dyn Fn()| {
            f();
            let t = std::time::Instant::now();
            for _ in 0..5 {
                f();
            }
            eprintln!("{label}: {:?}", t.elapsed() / 5);
        };
        time("cpu full render", &|| {
            apply_cpu(&source, &effective);
        });
        time("gpu full render (rgb)", &|| {
            working_rgb(&source, &effective);
        });
        time("gpu preview, cached early stage", &|| {
            render_preview(&source, &settings, ClipWarnings::default());
        });
        for (label, only) in [
            ("neutral", DevelopSettings::default()),
            ("exposure", with(|s| s.exposure = 10.0)),
            ("clarity", with(|s| s.clarity = 40.0)),
            ("sharpening (5 taps)", with(|s| s.sharpening = 40.0)),
            ("custom sharpening", with(|s| { s.sharpening = 40.0; s.sharpen_radius = 80.0; })),
            ("noise", with(|s| s.noise_luma = 40.0)),
            ("finishing", with(|s| { s.dehaze = 20.0; s.grading.shadows.saturation = 30.0; })),
            ("crop", with(|s| s.crop.angle = 3.0)),
        ] {
            time(&format!("gpu preview: {label}"), &|| {
                render_preview(&source, &only, ClipWarnings::default());
            });
        }
    }

    /// A busy edit: everything that reaches across pixels, plus a
    /// straightened crop, so the detail view's margins are exercised.
    fn busy() -> DevelopSettings {
        with(|s| {
            s.exposure = 10.0;
            s.clarity = 50.0;
            s.sharpening = 40.0;
            s.noise_luma = 40.0;
            s.noise_color = 40.0;
            s.vignette = -40.0;
            s.grain = 30.0;
            s.crop = Crop {
                rect: (0.05, 0.08, 0.85, 0.8),
                angle: 4.0,
                ..Default::default()
            };
            s.gradients.push(Gradient {
                kind: GradientKind::Radial,
                start: (0.5, 0.5),
                radius: (0.3, 0.25),
                feather: 0.6,
                adjust: LocalAdjust {
                    clarity: 60.0,
                    sharpening: 40.0,
                    exposure: 20.0,
                    ..Default::default()
                },
                opacity: 100.0,
                ..Default::default()
            });
        })
    }

    #[test]
    fn detail_region_matches_the_full_render() {
        if !gpu_available() {
            return;
        }
        let source = card(900, 640);
        let settings = busy();
        // The whole photo as a "region" is an ordinary full render.
        let full_size = {
            let rgb = working_rgb(&source, &settings).expect("full render");
            (rgb.width(), rgb.height())
        };
        let detail = |region, size| match gpu::render(
            &source,
            &settings,
            gpu::Output::Detail { region, size },
        ) {
            Some(gpu::Rendered::Detail(bgra)) => bgra,
            _ => panic!("detail render failed"),
        };
        let full = detail((0.0, 0.0, 1.0, 1.0), full_size);
        let (fw, fh) = (full.width() as f32, full.height() as f32);
        // A region in the middle and one touching the corner: each must
        // match the same pixels of the full render.
        for region in [(0.3, 0.35, 0.25, 0.2), (0.0, 0.0, 0.3, 0.3)] {
            let x0 = (region.0 * fw).floor() as u32;
            let y0 = (region.1 * fh).floor() as u32;
            let x1 = ((region.0 + region.2) * fw).ceil() as u32;
            let y1 = ((region.1 + region.3) * fh).ceil() as u32;
            let size = (x1 - x0, y1 - y0);
            let part = detail(region, size);
            assert_eq!(part.dimensions(), size);
            let expected = image::imageops::crop_imm(&full, x0, y0, size.0, size.1).to_image();
            let max = part
                .as_raw()
                .iter()
                .zip(expected.as_raw())
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(max <= 1, "region {region:?}: max diff {max}");
        }
    }

    #[test]
    fn gpu_keeps_gradients_smooth() {
        if !gpu_available() {
            return;
        }
        // A dark, gentle gradient pushed hard: 8-bit stages leave gaps
        // between the levels it can reach; floats fill them.
        let image = DynamicImage::ImageRgb16(image::ImageBuffer::from_fn(1024, 4, |x, _| {
            let v = (x as f32 / 1023.0 * 0.08 * 65535.0) as u16;
            image::Rgb([v, v, v])
        }));
        let source = DevelopSource::from_working(image);
        let settings = with(|s| {
            s.exposure = 100.0;
            s.shadows = 60.0;
            s.contrast = 40.0;
        });
        let levels = |img: &RgbImage| {
            let mut seen: Vec<u8> = img.pixels().map(|p| p[1]).collect();
            seen.sort_unstable();
            seen.dedup();
            seen.len()
        };
        let gpu = working_rgb(&source, &settings).unwrap();
        let cpu = apply_cpu(&source, &settings.effective());
        eprintln!("distinct levels: gpu {}, cpu {}", levels(&gpu), levels(&cpu));
        assert!(levels(&gpu) as f32 > levels(&cpu) as f32 * 1.5);
    }

    #[test]
    fn raw_development_matches_the_cpu() {
        if !gpu_available() {
            return;
        }
        // Linear camera RGB, with a blown highlight in one corner.
        let color = develop_source::RawColor {
            matrix: [1.6, -0.4, -0.2, -0.2, 1.4, -0.2, 0.0, -0.5, 1.5],
            clip: [2.0, 1.0, 1.5],
        };
        let mut data = Vec::new();
        for y in 0..64u32 {
            for x in 0..64u32 {
                let c = if x > 48 && y > 48 {
                    [2.0, 1.0, 1.5]
                } else {
                    [x as f32 / 80.0, y as f32 / 70.0, 0.3]
                };
                let h = |v: f32| half::f16::from_f32(v).to_bits() as u32;
                data.push(h(c[0]) | (h(c[1]) << 16));
                data.push(h(c[2]));
            }
        }
        let source = DevelopSource::from_half(64, 64, data, Some(color));
        let gpu = working_rgb(&source, &DevelopSettings::default()).unwrap();
        let cpu = apply_cpu(&source, &DevelopSettings::default());
        let max = gpu
            .as_raw()
            .iter()
            .zip(cpu.as_raw())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(max <= 2, "max diff {max}");
        // The blown corner comes out neutral white, not magenta.
        let corner = gpu.get_pixel(60, 60);
        assert!(corner[0] > 250 && corner[1] > 250 && corner[2] > 250, "{corner:?}");
        // Pulling exposure down recovers it as a neutral grey.
        let darker = working_rgb(&source, &with(|s| s.exposure = -60.0)).unwrap();
        let corner = darker.get_pixel(60, 60);
        assert!(corner[0] < 250, "{corner:?}");
        assert!((corner[0] as i32 - corner[1] as i32).abs() < 12, "{corner:?}");
    }
}
