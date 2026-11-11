//! Glow: a soft bloom of light around the bright areas, like a diffusion
//! filter or a dreamy Orton print. The picture's highlights are picked
//! out, blurred widely, and laid back over the picture with a screen
//! blend, so bright areas spill light into their surroundings while the
//! rest stays put and nothing is ever pushed past white.

use image::{imageops, RgbImage};
use rustyedit_core::{DevelopSettings, GlowStyle};

/// The blur behind the glow at Size 50: wide, in proportion to the photo,
/// so a preview looks like the full-size export.
const BASE_SIGMA: f32 = 22.0 / 2048.0;

/// Whether the glow does anything for these settings.
pub(crate) fn is_active(s: &DevelopSettings) -> bool {
    s.glow >= 0.5
}

/// The blur's width, with the Size slider (0..=100; 50 is the default).
pub(crate) fn glow_sigma(width: u32, height: u32, size: f32) -> f32 {
    let factor = 0.35 + size.clamp(0.0, 100.0) / 100.0 * 1.3;
    (width.max(height) as f32 * BASE_SIGMA * factor).max(4.0)
}

/// The slider as a strength, 0..=1.
pub(crate) fn glow_amount(glow: f32) -> f32 {
    glow.clamp(0.0, 100.0) / 100.0
}

/// The brightness a highlight must pass to start glowing (the Threshold
/// slider, 0..=100): a low threshold lets mid-tones glow too. Halation
/// keeps to the brightest edges.
pub(crate) fn glow_threshold(threshold: f32, style: GlowStyle) -> f32 {
    let start = 0.30 + threshold.clamp(0.0, 100.0) / 100.0 * 0.5;
    if style == GlowStyle::Halation {
        (start + 0.1).min(0.95)
    } else {
        start
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// What is blurred to make the glow: the highlights, weighted by how
/// bright the pixel is, or for the softening styles the whole picture.
pub(crate) fn source(v: [f32; 3], style: GlowStyle, threshold: f32) -> [f32; 3] {
    if !style.from_highlights() {
        return v;
    }
    let luma = 0.299 * v[0] + 0.587 * v[1] + 0.114 * v[2];
    let weight = smoothstep(threshold, 1.0, luma);
    v.map(|c| c * weight)
}

fn screen(v: f32, b: f32) -> f32 {
    v + b.clamp(0.0, 1.0) * (1.0 - v)
}

/// One pixel: `v` is the picture and `b` the blur of [`source`].
pub(crate) fn glow_pixel(v: [f32; 3], b: [f32; 3], style: GlowStyle, amount: f32) -> [f32; 3] {
    let a = amount;
    [0, 1, 2].map(|c| {
        let (v, b) = (v[c], b[c]);
        let out = match style {
            GlowStyle::Bloom => screen(v, b * a * 2.0),
            GlowStyle::Halation => screen(v, b * a * 2.4 * [1.0, 0.38, 0.14][c]),
            GlowStyle::Cinema => screen(v, b * a * 2.2 * [1.0, 0.82, 0.62][c]),
            GlowStyle::Diffusion => v + (b - v) * 0.55 * a + (screen(v, b) - v) * 0.25 * a,
            GlowStyle::Dreamy => {
                let overlay = 0.6 * screen(v, b) + 0.4 * v * b;
                v + (overlay - v) * 0.85 * a
            }
            GlowStyle::Mist => {
                let softened = v + (b - v) * 0.5 * a;
                softened + (1.0 - softened) * 0.12 * a
            }
        };
        out.clamp(0.0, 1.0)
    })
}

/// Lays the glow on `image`. A no-op at zero.
pub fn apply_glow(image: &mut RgbImage, settings: &DevelopSettings) {
    if !is_active(settings) || image.width() < 3 || image.height() < 3 {
        return;
    }
    let style = settings.glow_style;
    let amount = glow_amount(settings.glow);
    let threshold = glow_threshold(settings.glow_threshold, style);
    let mut base = image.clone();
    for px in base.pixels_mut() {
        let s = source(px.0.map(|c| c as f32 / 255.0), style, threshold);
        px.0 = s.map(|c| (c * 255.0).round() as u8);
    }
    let sigma = glow_sigma(image.width(), image.height(), settings.glow_size);
    let blurred = imageops::blur(&base, sigma);
    let blurred = blurred.as_raw();
    let raw: &mut [u8] = &mut *image;
    crate::parallel::for_row_bands(raw, 3, |first, band| {
        for (i, px) in band.chunks_exact_mut(3).enumerate() {
            let at = (first + i) * 3;
            let v = [0, 1, 2].map(|c| px[c] as f32 / 255.0);
            let b = [0, 1, 2].map(|c| blurred[at + c] as f32 / 255.0);
            let out = glow_pixel(v, b, style, amount);
            for c in 0..3 {
                px[c] = (out[c] * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// A bright half beside a dark half.
    fn scene() -> RgbImage {
        RgbImage::from_fn(160, 60, |x, _| {
            let v = if x < 80 { 245 } else { 40 };
            Rgb([v, v, v])
        })
    }

    fn with(style: GlowStyle, glow: f32) -> DevelopSettings {
        DevelopSettings { glow, glow_style: style, ..Default::default() }
    }

    fn glowed(style: GlowStyle, glow: f32) -> RgbImage {
        let mut img = scene();
        apply_glow(&mut img, &with(style, glow));
        img
    }

    #[test]
    fn light_spills_from_the_bright_side_into_the_dark_one() {
        let img = glowed(GlowStyle::Bloom, 100.0);
        assert!(img.get_pixel(84, 30)[0] > 80, "{}", img.get_pixel(84, 30)[0]);
        assert!(img.get_pixel(155, 30)[0] <= 41);
    }

    #[test]
    fn more_glow_is_always_brighter_and_never_clips_past_white() {
        for style in GlowStyle::ALL {
            let at = |g: f32| glowed(style, g).get_pixel(86, 30)[0];
            assert!(at(20.0) <= at(60.0) && at(60.0) <= at(100.0), "{style:?}");
            assert!(glowed(style, 100.0).get_pixel(10, 30)[0] >= 150, "{style:?}");
        }
    }

    #[test]
    fn zero_glow_leaves_the_picture_alone() {
        for style in GlowStyle::ALL {
            assert_eq!(glowed(style, 0.0), scene(), "{style:?}");
        }
    }

    #[test]
    fn a_dark_picture_stays_dark_with_the_highlight_styles() {
        for style in [GlowStyle::Bloom, GlowStyle::Halation, GlowStyle::Cinema] {
            let mut img = RgbImage::from_pixel(64, 64, Rgb([20, 20, 20]));
            apply_glow(&mut img, &with(style, 100.0));
            assert_eq!(img.get_pixel(32, 32)[0], 20, "{style:?}");
        }
    }

    #[test]
    fn halation_is_warm_and_diffusion_softens_the_edge() {
        let halation = glowed(GlowStyle::Halation, 100.0);
        let p = halation.get_pixel(84, 30);
        assert!(p[0] > p[1] && p[1] > p[2], "{p:?}");
        // Diffusion pulls the bright side of the edge down and the dark up.
        let diffused = glowed(GlowStyle::Diffusion, 100.0);
        assert!(diffused.get_pixel(78, 30)[0] < 245);
        assert!(diffused.get_pixel(82, 30)[0] > 40);
    }

    #[test]
    fn mist_lifts_the_blacks() {
        let img = glowed(GlowStyle::Mist, 100.0);
        assert!(img.get_pixel(155, 30)[0] > 40 + 10, "{}", img.get_pixel(155, 30)[0]);
    }

    #[test]
    fn a_wider_size_spreads_the_light_further() {
        // A big picture, so the blur isn't held at its minimum width.
        let at = |size: f32| {
            let mut img = RgbImage::from_fn(1200, 40, |x, _| {
                let v = if x < 600 { 245 } else { 40 };
                Rgb([v, v, v])
            });
            let s = DevelopSettings { glow: 100.0, glow_size: size, ..Default::default() };
            apply_glow(&mut img, &s);
            img.get_pixel(640, 20)[0]
        };
        assert!(at(100.0) > at(10.0), "{} {}", at(100.0), at(10.0));
    }
}
