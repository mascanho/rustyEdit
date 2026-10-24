//! Camera matching for RAW photos: starting a RAW photo from how the
//! camera itself showed it.
//!
//! Every RAW file carries the camera's own JPEG preview, made with the
//! maker's colour look and, on most modern cameras, its lens corrections.
//! That preview is the reference:
//!
//! - Look: the photo's tone and colour are fitted to the preview with the
//!   Match look engine. It's the same scene, so colours are matched fully,
//!   not just leaned towards.
//! - Lens: the Distortion slider is fitted by lining the photo's edges up
//!   with the preview's (allowing for the slight zoom cameras add after
//!   correcting), and Vignetting by matching how bright the corners are
//!   against the centre. A camera that didn't correct the lens leaves both
//!   at zero.
//!
//! This works for every maker and lens the camera knows, with no lens
//! database, but only as well as the camera's own corrections.

use image::{imageops::FilterType, DynamicImage, RgbImage};
use rustyedit_core::DevelopSettings;

use crate::{
    auto::solve_increasing,
    look::{match_look_with, LookStats, MatchOptions},
    prepare_preview, render_export, DevelopSource, OutputSpace,
};

/// Long edge the photo and preview are compared at.
const FIT_EDGE: u32 = 512;
/// Previews smaller than this (old cameras' thumbnails) aren't trusted.
const MIN_PREVIEW_EDGE: u32 = 640;
/// Previews whose shape differs more than this from the photo's are
/// cropped differently, so their geometry isn't compared.
const MAX_ASPECT_DIFFERENCE: f32 = 0.03;
/// Zoom the camera may have added after correcting distortion.
const SCALES: [f32; 7] = [0.97, 0.98, 0.99, 1.0, 1.01, 1.02, 1.03];

/// What to take from the camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraMatch {
    /// Tone and colour.
    pub look: bool,
    /// Distortion and vignetting corrections.
    pub lens: bool,
}

/// `settings` with the camera's look and/or lens corrections taken from
/// `preview` (the camera's JPEG, upright). Only the matched parts change:
/// tone and colour for the look, Distortion and Vignetting for the lens.
pub fn match_camera(
    source: &DevelopSource,
    settings: &DevelopSettings,
    preview: &DynamicImage,
    what: CameraMatch,
) -> DevelopSettings {
    if preview.width().max(preview.height()) < MIN_PREVIEW_EDGE {
        return *settings;
    }
    let small = prepare_preview(source, FIT_EDGE);
    let render = |s: &DevelopSettings| render_export(&small, s, None, None, &OutputSpace::Srgb);
    let mut s = *settings;

    if what.lens {
        if let Some(k) = fit_distortion(&render, preview) {
            s.lens_distortion = k;
        }
    }
    if what.look {
        let matched = match_look_with(
            source,
            &s,
            &LookStats::of_image(preview),
            MatchOptions {
                // Corner darkening is the lens's business.
                vignette: false,
                // Same scene: copy the colours fully.
                mixer_strength: 1.0,
            },
        );
        s.exposure = matched.exposure;
        s.contrast = matched.contrast;
        s.highlights = matched.highlights;
        s.shadows = matched.shadows;
        s.whites = matched.whites;
        s.blacks = matched.blacks;
        s.temperature = matched.temperature;
        s.tint = matched.tint;
        s.vibrance = matched.vibrance;
        s.saturation = matched.saturation;
        s.curves = matched.curves;
        s.parametric = matched.parametric;
        s.mixer = matched.mixer;
        s.grading = matched.grading;
    }
    if what.lens {
        if let Some(v) = fit_vignetting(&render, &s, preview) {
            s.lens_vignetting = v;
        }
    }
    s
}

/// Edge strength (Sobel on a lightly blurred greyscale), one value per pixel.
fn edges(img: &RgbImage) -> (Vec<f32>, usize, usize) {
    let grey = image::imageops::blur(&DynamicImage::ImageRgb8(img.clone()).to_luma8(), 0.8);
    let (w, h) = (grey.width() as usize, grey.height() as usize);
    let at = |x: usize, y: usize| grey.as_raw()[y * w + x] as f32;
    let mut out = vec![0.0; w * h];
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let gx = at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1)
                - at(x - 1, y - 1)
                - 2.0 * at(x - 1, y)
                - at(x - 1, y + 1);
            let gy = at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1)
                - at(x - 1, y - 1)
                - 2.0 * at(x, y - 1)
                - at(x + 1, y - 1);
            out[y * w + x] = gx.hypot(gy);
        }
    }
    (out, w, h)
}

/// How well `photo`'s edges, zoomed by `scale` about the centre, line up
/// with `reference`'s in the outer part of the frame (where distortion
/// shows): normalised cross-correlation, 1 = perfect.
fn alignment(photo: &[f32], reference: &[f32], w: usize, h: usize, scale: f32) -> f32 {
    let (cx, cy) = ((w as f32 - 1.0) / 2.0, (h as f32 - 1.0) / 2.0);
    let norm = (cx * cx + cy * cy).sqrt().max(1.0);
    let sample = |fx: f32, fy: f32| -> Option<f32> {
        if fx < 0.0 || fy < 0.0 || fx > (w - 1) as f32 || fy > (h - 1) as f32 {
            return None;
        }
        let (x0, y0) = (fx as usize, fy as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let p = |x: usize, y: usize| photo[y * w + x];
        let top = p(x0, y0) * (1.0 - tx) + p(x1, y0) * tx;
        let bottom = p(x0, y1) * (1.0 - tx) + p(x1, y1) * tx;
        Some(top * (1.0 - ty) + bottom * ty)
    };
    let (mut sa, mut sb, mut saa, mut sbb, mut sab, mut n) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for y in (2..h - 2).step_by(2) {
        for x in (2..w - 2).step_by(2) {
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            let r = (dx * dx + dy * dy).sqrt() / norm;
            if !(0.4..=0.95).contains(&r) {
                continue;
            }
            let Some(a) = sample(cx + dx / scale, cy + dy / scale) else {
                continue;
            };
            let b = reference[y * w + x];
            let (a, b) = (a as f64, b as f64);
            sa += a;
            sb += b;
            saa += a * a;
            sbb += b * b;
            sab += a * b;
            n += 1.0;
        }
    }
    if n < 100.0 {
        return 0.0;
    }
    let cov = sab - sa * sb / n;
    let va = saa - sa * sa / n;
    let vb = sbb - sb * sb / n;
    if va <= 0.0 || vb <= 0.0 {
        return 0.0;
    }
    (cov / (va * vb).sqrt()) as f32
}

/// The Distortion value that lines the photo up with the camera's
/// (corrected) preview, or `None` when the preview can't be compared or no
/// value beats leaving it alone.
fn fit_distortion(
    render: &impl Fn(&DevelopSettings) -> RgbImage,
    preview: &DynamicImage,
) -> Option<f32> {
    let with = |k: f32| {
        let mut s = DevelopSettings::default();
        s.lens_distortion = k;
        render(&s)
    };
    let first = with(0.0);
    let (w, h) = first.dimensions();
    let photo_aspect = w as f32 / h.max(1) as f32;
    let preview_aspect = preview.width() as f32 / preview.height().max(1) as f32;
    if (photo_aspect / preview_aspect - 1.0).abs() > MAX_ASPECT_DIFFERENCE {
        return None;
    }
    let (reference, rw, rh) = edges(&preview.resize_exact(w, h, FilterType::Triangle).to_rgb8());
    if (rw, rh) != (w as usize, h as usize) {
        return None;
    }
    let score = |img: &RgbImage| {
        let (photo, w, h) = edges(img);
        SCALES
            .iter()
            .map(|&scale| alignment(&photo, &reference, w, h, scale))
            .fold(f32::MIN, f32::max)
    };

    let none = score(&first);
    let mut best = (0.0f32, none);
    for k in (-60..=60).step_by(10).filter(|&k| k != 0) {
        let s = score(&with(k as f32));
        if s > best.1 {
            best = (k as f32, s);
        }
    }
    let centre = best.0;
    for k in (-8..=8).step_by(2).filter(|&d| d != 0).map(|d| centre + d as f32) {
        if !(-100.0..=100.0).contains(&k) {
            continue;
        }
        let s = score(&with(k));
        if s > best.1 {
            best = (k, s);
        }
    }
    // Only a clear improvement counts; otherwise the camera didn't correct.
    (best.0 != 0.0 && best.1 > none + 0.01).then_some(best.0)
}

/// Brightness of the corners against the centre (display levels, 0-1).
fn corner_ratio(img: &RgbImage) -> f32 {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let (mut centre, mut nc, mut corners, mut nk) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for (x, y, p) in img.enumerate_pixels() {
        let dx = (x as f32 + 0.5) / w * 2.0 - 1.0;
        let dy = (y as f32 + 0.5) / h * 2.0 - 1.0;
        let r = (dx * dx + dy * dy).sqrt() / std::f32::consts::SQRT_2;
        let l = (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0;
        if r < 0.35 {
            centre += l;
            nc += 1.0;
        } else if r > 0.85 {
            corners += l;
            nk += 1.0;
        }
    }
    if nc == 0.0 || nk == 0.0 || centre / nc < 0.02 {
        return 1.0;
    }
    (corners / nk) / (centre / nc)
}

/// The Vignetting value that brightens the corners as much as the camera
/// did, or `None` if the camera didn't brighten them beyond the photo.
fn fit_vignetting(
    render: &impl Fn(&DevelopSettings) -> RgbImage,
    settings: &DevelopSettings,
    preview: &DynamicImage,
) -> Option<f32> {
    let with = |v: f32| {
        let mut s = *settings;
        s.lens_vignetting = v;
        corner_ratio(&render(&s))
    };
    let target = corner_ratio(&preview.to_rgb8());
    let now = with(0.0);
    if target <= now + 0.02 {
        return None;
    }
    let v = solve_increasing(|v| with(v) - target, (0.0, 100.0), (0.0, now - target), 30.0, 0.01, 5);
    (v >= 1.0).then(|| v.round())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// A detailed test scene: a grid of lines and dots.
    fn scene(w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| {
            let line = x % 40 < 3 || y % 40 < 3;
            let dot = ((x / 20) + (y / 20)) % 7 == 0;
            let v = if line { 30 } else if dot { 200 } else { 140 };
            Rgb([v, (v as f32 * 0.9) as u8, (v as f32 * 0.8) as u8])
        })
    }

    #[test]
    fn a_matching_preview_needs_no_lens_correction() {
        let img = scene(960, 640);
        let source = DevelopSource::from_image(&DynamicImage::ImageRgb8(img.clone()), None);
        let small = prepare_preview(&source, FIT_EDGE);
        let render = |s: &DevelopSettings| render_export(&small, s, None, None, &OutputSpace::Srgb);
        // The preview is the photo itself: nothing to correct.
        let preview = DynamicImage::ImageRgb8(render(&DevelopSettings::default()));
        assert_eq!(fit_distortion(&render, &preview), None);
        assert_eq!(fit_vignetting(&render, &DevelopSettings::default(), &preview), None);
    }

    #[test]
    fn finds_the_distortion_the_camera_corrected() {
        let img = scene(960, 640);
        let source = DevelopSource::from_image(&DynamicImage::ImageRgb8(img), None);
        let small = prepare_preview(&source, FIT_EDGE);
        let render = |s: &DevelopSettings| render_export(&small, s, None, None, &OutputSpace::Srgb);
        // A "camera" preview with 30 units of distortion correction.
        let corrected = DevelopSettings {
            lens_distortion: 30.0,
            ..Default::default()
        };
        let preview = DynamicImage::ImageRgb8(render(&corrected));
        let k = fit_distortion(&render, &preview).expect("should find a correction");
        assert!((k - 30.0).abs() <= 4.0, "found {k}");
    }

    #[test]
    fn finds_the_vignetting_the_camera_corrected() {
        let img = RgbImage::from_pixel(960, 640, Rgb([110, 110, 110]));
        let source = DevelopSource::from_image(&DynamicImage::ImageRgb8(img), None);
        let small = prepare_preview(&source, FIT_EDGE);
        let render = |s: &DevelopSettings| render_export(&small, s, None, None, &OutputSpace::Srgb);
        let corrected = DevelopSettings {
            lens_vignetting: 40.0,
            ..Default::default()
        };
        let preview = DynamicImage::ImageRgb8(render(&corrected));
        let v = fit_vignetting(&render, &DevelopSettings::default(), &preview).expect("should fit");
        assert!((v - 40.0).abs() <= 5.0, "found {v}");
    }

    #[test]
    fn tiny_previews_are_ignored() {
        let source = DevelopSource::from_image(&DynamicImage::ImageRgb8(scene(960, 640)), None);
        let thumb = DynamicImage::ImageRgb8(scene(160, 106));
        let settings = DevelopSettings::default();
        let both = CameraMatch {
            look: true,
            lens: true,
        };
        assert_eq!(match_camera(&source, &settings, &thumb, both), settings);
    }
}
