//! The non-destructive edit pipeline: turns a decoded source image plus a
//! `DevelopSettings` into a rendered preview, in three stages:
//!
//! 0. `retouch`: spot removal (heal / clone) on the original photo, then
//!    `lens`: distortion, chromatic aberration and vignetting corrections;
//! 1. `geometry`: flips and rotation, then `denoise`: noise reduction;
//! 2. the base stage (`tone`, `color`, `detail`), on the GPU (`gpu`) when
//!    one is available and the CPU otherwise (sharpening with a custom
//!    radius or edge masking runs after it, in `sharpen`);
//! 3. `finishing` (before the crop): tone curves, Color Mixer, Color
//!    Grading, Dehaze and gradient masks, on the CPU across threads;
//! 4. `crop`: straighten and crop;
//! 5. `finishing` (after the crop): Vignette and Grain, framed by the crop.
//!
//! Each stage is skipped while its settings are neutral.

mod color;
mod crop;
mod denoise;
mod detail;
mod finishing;
mod geometry;
mod gpu;
mod lens;
mod parallel;
mod retouch;
mod sharpen;
mod tone;

pub use retouch::find_spot_source;

use image::{DynamicImage, RgbImage};
use rustyedit_core::DevelopSettings;

/// Applies the full develop pipeline to `source`, returning a new image.
/// `source` is never mutated, so callers can keep a cached decoded copy
/// around and re-render cheaply whenever a slider changes.
pub fn apply(source: &DynamicImage, settings: &DevelopSettings) -> RgbImage {
    apply_with(source, settings, true)
}

fn apply_with(source: &DynamicImage, settings: &DevelopSettings, allow_gpu: bool) -> RgbImage {
    // Sections switched off in the panel render as if untouched.
    let settings = &settings.effective();
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

    // Sharpening with a custom radius or an edge mask isn't in the base
    // stage; it runs right after it.
    let custom_sharpen = settings.sharpening > 0.5
        && (settings.sharpen_masking > 0.5 || (settings.sharpen_radius - 50.0).abs() > 0.5);
    let base = DevelopSettings {
        sharpening: if custom_sharpen {
            0.0
        } else {
            settings.sharpening
        },
        ..*settings
    };
    if has_base_edits(&base) {
        buffer = base_stage(buffer, &base, allow_gpu);
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

/// Tone, colour and detail: on the GPU when possible, else the CPU.
fn base_stage(mut buffer: RgbImage, settings: &DevelopSettings, allow_gpu: bool) -> RgbImage {
    if allow_gpu {
        if let Some(rendered) = gpu::apply(&buffer, settings) {
            return rendered;
        }
    }
    for pixel in buffer.pixels_mut() {
        let with_tone = tone::apply_tone_rgb8(*pixel, settings);
        *pixel = color::apply_color_rgb8(with_tone, settings);
    }
    detail::apply_detail(&mut buffer, settings);
    buffer
}

/// Whether any of the base stage's sliders is off neutral.
fn has_base_edits(s: &DevelopSettings) -> bool {
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

/// Downscales `source` so its longest edge is at most `max_edge`, using a
/// fast filter. Intended to keep interactive Develop-panel previews quick
/// even on very large source files; full quality is only needed at export.
pub fn downscale_for_preview(source: &DynamicImage, max_edge: u32) -> DynamicImage {
    if source.width() <= max_edge && source.height() <= max_edge {
        return source.clone();
    }
    source.resize(max_edge, max_edge, image::imageops::FilterType::Triangle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_matches_cpu() {
        let Some(name) = gpu_adapter_name() else {
            eprintln!("no GPU available; skipping");
            return;
        };
        eprintln!("GPU: {name}");
        let img = RgbImage::from_fn(2560, 1706, |x, y| {
            image::Rgb([
                (x * 255 / 2560) as u8,
                (y * 255 / 1706) as u8,
                ((x * 7 + y * 13) % 256) as u8,
            ])
        });
        let source = DynamicImage::ImageRgb8(img);
        let settings = DevelopSettings {
            exposure: 20.0,
            contrast: 15.0,
            highlights: -30.0,
            shadows: 25.0,
            whites: 10.0,
            blacks: -10.0,
            temperature: 20.0,
            tint: -10.0,
            vibrance: 30.0,
            saturation: 15.0,
            clarity: 40.0,
            sharpening: 50.0,
            ..Default::default()
        };
        let t = std::time::Instant::now();
        let gpu = apply_with(&source, &settings, true);
        let gpu_time = t.elapsed();
        let t = std::time::Instant::now();
        let cpu = apply_with(&source, &settings, false);
        eprintln!("gpu {gpu_time:?} vs cpu {:?}", t.elapsed());
        let total: u64 = gpu
            .as_raw()
            .iter()
            .zip(cpu.as_raw())
            .map(|(a, b)| a.abs_diff(*b) as u64)
            .sum();
        let max = gpu
            .as_raw()
            .iter()
            .zip(cpu.as_raw())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        let mean = total as f64 / gpu.as_raw().len() as f64;
        eprintln!("mean abs diff {mean:.4}, max {max}");
        assert!(mean < 1.0, "mean diff {mean}");
    }
}
