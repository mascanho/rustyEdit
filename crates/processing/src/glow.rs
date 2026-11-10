//! Glow: a soft bloom of light around the bright areas, like a diffusion
//! filter or a dreamy Orton print. The picture's highlights are picked
//! out, blurred widely, and laid back over the picture with a screen
//! blend, so bright areas spill light into their surroundings while the
//! rest stays put and nothing is ever pushed past white.

use image::{imageops, RgbImage};

/// How much brighter the (dim, blurred) highlights are laid on at the
/// slider's far end.
const GAIN: f32 = 2.0;
/// Brightness where a pixel starts to count as a highlight, and where it
/// counts fully.
pub(crate) const HIGHLIGHTS: (f32, f32) = (0.55, 1.0);

/// The blur behind the glow: wide, in proportion to the photo, so a
/// preview looks like the full-size export.
pub(crate) fn glow_sigma(width: u32, height: u32) -> f32 {
    (width.max(height) as f32 / 2048.0 * 22.0).max(4.0)
}

/// How much of the bloom is laid on at slider value `glow` (0..=100).
pub(crate) fn glow_amount(glow: f32) -> f32 {
    glow.clamp(0.0, 100.0) / 100.0 * GAIN
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The part of a pixel (0..=1 per channel) that blooms: the pixel, weighted
/// by how much of a highlight it is.
pub(crate) fn highlight(v: [f32; 3]) -> [f32; 3] {
    let luma = 0.299 * v[0] + 0.587 * v[1] + 0.114 * v[2];
    let weight = smoothstep(HIGHLIGHTS.0, HIGHLIGHTS.1, luma);
    v.map(|c| c * weight)
}

/// One pixel: `v` is the picture and `b` the blur of its highlights.
pub(crate) fn glow_pixel(v: [f32; 3], b: [f32; 3], amount: f32) -> [f32; 3] {
    [0, 1, 2].map(|c| {
        let bloom = (b[c] * amount).clamp(0.0, 1.0);
        v[c] + bloom * (1.0 - v[c])
    })
}

/// Lays the glow on `image`. A no-op at zero.
pub fn apply_glow(image: &mut RgbImage, glow: f32) {
    if glow < 0.5 || image.width() < 3 || image.height() < 3 {
        return;
    }
    let amount = glow_amount(glow);
    let mut lights = image.clone();
    for px in lights.pixels_mut() {
        let h = highlight(px.0.map(|c| c as f32 / 255.0));
        px.0 = h.map(|c| (c * 255.0).round() as u8);
    }
    let blurred = imageops::blur(&lights, glow_sigma(image.width(), image.height()));
    let blurred = blurred.as_raw();
    let raw: &mut [u8] = &mut *image;
    crate::parallel::for_row_bands(raw, 3, |first, band| {
        for (i, px) in band.chunks_exact_mut(3).enumerate() {
            let at = (first + i) * 3;
            let v = [0, 1, 2].map(|c| px[c] as f32 / 255.0);
            let b = [0, 1, 2].map(|c| blurred[at + c] as f32 / 255.0);
            let out = glow_pixel(v, b, amount);
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

    #[test]
    fn light_spills_from_the_bright_side_into_the_dark_one() {
        let mut img = scene();
        apply_glow(&mut img, 100.0);
        // Just past the edge, the dark side is lifted; far away it isn't.
        assert!(img.get_pixel(84, 30)[0] > 80, "{}", img.get_pixel(84, 30)[0]);
        assert!(img.get_pixel(155, 30)[0] <= 41);
    }

    #[test]
    fn more_glow_is_always_brighter_and_never_clips_past_white() {
        let at = |g: f32| {
            let mut img = scene();
            apply_glow(&mut img, g);
            img.get_pixel(86, 30)[0]
        };
        assert!(at(20.0) < at(60.0) && at(60.0) < at(100.0));
        let mut img = scene();
        apply_glow(&mut img, 100.0);
        assert!(img.get_pixel(10, 30)[0] >= 245);
    }

    #[test]
    fn zero_glow_leaves_the_picture_alone() {
        let mut img = scene();
        apply_glow(&mut img, 0.0);
        assert_eq!(img, scene());
    }

    #[test]
    fn a_dark_picture_stays_dark() {
        let mut img = RgbImage::from_pixel(64, 64, Rgb([20, 20, 20]));
        apply_glow(&mut img, 100.0);
        assert_eq!(img.get_pixel(32, 32)[0], 20);
    }
}
