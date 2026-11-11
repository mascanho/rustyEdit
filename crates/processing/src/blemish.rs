//! Blemish removal: finds small red spots on skin (pimples, redness) and
//! blends them into the skin around them, with no brush strokes.
//!
//! A spot is a place on skin that is redder than the surrounding skin
//! (compared with a blur of the picture: wide enough to be little changed by
//! a spot, narrow enough to follow the skin's own tone), and a little
//! darker, and that is ringed by skin all the way round (which keeps lips,
//! eyes and the edge of the face out of it). Freckles and moles are brown,
//! not red, and eyebrows and eyelashes neutral, so they stay. Where a spot is found, the pixel moves
//! towards the blur by `fade`.
//!
//! This is a heuristic, not a face detector: it works on skin tones, so a
//! red object on a skin-coloured background could be picked up too, which
//! is what the Amount slider is there to rein in.

use image::{imageops, RgbImage};
use rustyedit_core::DevelopSettings;

/// The blur's width at 2048 pixels on the long edge.
const BASE_SIGMA: f32 = 14.0 / 2048.0;
/// How far out (in blurs' widths) the ring checks that a spot sits in skin.
const RING: f32 = 2.2;

pub(crate) fn is_active(s: &DevelopSettings) -> bool {
    s.blemish_amount >= 0.5 && s.blemish_fade >= 0.5
}

/// The surround blur's width for a photo this size.
pub(crate) fn sigma(width: u32, height: u32) -> f32 {
    (width.max(height) as f32 * BASE_SIGMA).max(2.0)
}

/// How much redness above the surroundings counts as a spot, from the
/// Amount slider: more amount finds fainter spots.
pub(crate) fn threshold(amount: f32) -> f32 {
    0.11 - 0.085 * (amount.clamp(0.0, 100.0) / 100.0)
}

/// How completely a spot is blended away, 0..=1.
pub(crate) fn fade(fade: f32) -> f32 {
    fade.clamp(0.0, 100.0) / 100.0
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much like skin a colour (0..=1 per channel) is: a warm hue (red
/// over green and blue), moderate colour, and not dark.
pub(crate) fn skin(c: [f32; 3]) -> f32 {
    let [r, g, b] = c;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let warm = smoothstep(0.02, 0.06, r - g) * smoothstep(0.02, 0.06, r - b);
    let chroma = max - min;
    let colourful = smoothstep(0.05, 0.10, chroma) * (1.0 - smoothstep(0.42, 0.55, chroma));
    let bright = smoothstep(0.22, 0.34, max);
    warm * colourful * bright
}

/// How far out the ring is, in pixels.
pub(crate) fn ring_reach(sigma: f32) -> i32 {
    (sigma * RING).round().max(2.0) as i32
}

/// How surely a spot is ringed by skin, from the colours `taps` around it.
fn surrounded(taps: &[[f32; 3]; 8]) -> f32 {
    let mean = taps.iter().map(|t| skin(*t)).sum::<f32>() / 8.0;
    smoothstep(0.55, 0.9, mean)
}

/// One pixel: `v` is the picture, `b` its blur and `taps` the picture at
/// eight points around it.
pub(crate) fn blemish_pixel(
    v: [f32; 3],
    b: [f32; 3],
    taps: &[[f32; 3]; 8],
    threshold: f32,
    fade: f32,
) -> [f32; 3] {
    let luma = |c: [f32; 3]| 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
    // Redder than the skin around it, and a little darker.
    let redder = (v[0] - v[1]) - (b[0] - b[1]);
    let darker = luma(b) - luma(v);
    let spot = redder + 0.5 * darker.max(0.0);
    let warm = smoothstep(0.02, 0.06, v[0] - v[1]) * smoothstep(0.0, 0.04, v[0] - v[2]);
    let weight = smoothstep(threshold, threshold + 0.07, spot)
        * (redder > 0.0) as u8 as f32
        * warm
        * surrounded(taps)
        * fade;
    [0, 1, 2].map(|c| v[c] + (b[c] - v[c]) * weight)
}

/// The ring's eight points, as multiples of its reach (the diagonals on
/// the same circle).
const TAPS: [(f32, f32); 8] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    (0.7, 0.7),
    (-0.7, 0.7),
    (0.7, -0.7),
    (-0.7, -0.7),
];

/// Clears the blemishes of `image`. A no-op when off.
pub fn apply_blemish(image: &mut RgbImage, settings: &DevelopSettings) {
    if !is_active(settings) || image.width() < 3 || image.height() < 3 {
        return;
    }
    let (threshold, fade) = (threshold(settings.blemish_amount), fade(settings.blemish_fade));
    let (width, height) = (image.width() as i32, image.height() as i32);
    let sigma = sigma(image.width(), image.height());
    let blurred = imageops::blur(image, sigma);
    let blurred = blurred.as_raw();
    let original = image.as_raw().clone();
    let reach = ring_reach(sigma);
    let raw: &mut [u8] = &mut *image;
    crate::parallel::for_row_bands(raw, 3, |first, band| {
        for (i, px) in band.chunks_exact_mut(3).enumerate() {
            let at = (first + i) * 3;
            let (x, y) = ((i as i32) % width, (first as i32) + (i as i32) / width);
            let v = [0, 1, 2].map(|c| px[c] as f32 / 255.0);
            let b = [0, 1, 2].map(|c| blurred[at + c] as f32 / 255.0);
            let taps = TAPS.map(|(dx, dy)| {
                let tx = (x + (dx * reach as f32).round() as i32).clamp(0, width - 1);
                let ty = (y + (dy * reach as f32).round() as i32).clamp(0, height - 1);
                let o = ((ty * width + tx) * 3) as usize;
                [0, 1, 2].map(|c| original[o + c] as f32 / 255.0)
            });
            let out = blemish_pixel(v, b, &taps, threshold, fade);
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

    const SKIN: [u8; 3] = [205, 152, 122];

    /// A skin-coloured patch with something in the middle.
    fn patch(spot: [u8; 3]) -> RgbImage {
        RgbImage::from_fn(1000, 1000, |x, y| {
            let d = ((x as f32 - 500.0).powi(2) + (y as f32 - 500.0).powi(2)).sqrt();
            Rgb(if d < 4.0 { spot } else { SKIN })
        })
    }

    fn settings(amount: f32, fade: f32) -> DevelopSettings {
        DevelopSettings { blemish_amount: amount, blemish_fade: fade, ..Default::default() }
    }

    fn cleaned(spot: [u8; 3], amount: f32, fade: f32) -> RgbImage {
        let mut img = patch(spot);
        apply_blemish(&mut img, &settings(amount, fade));
        img
    }

    fn distance(a: &Rgb<u8>, b: [u8; 3]) -> i32 {
        (0..3).map(|c| (a[c] as i32 - b[c] as i32).abs()).sum()
    }

    #[test]
    fn a_red_spot_is_blended_into_the_skin() {
        let spot = [214, 98, 92];
        let before = distance(patch(spot).get_pixel(500, 500), SKIN);
        let after = distance(cleaned(spot, 80.0, 100.0).get_pixel(500, 500), SKIN);
        assert!(after < before / 3, "{before} -> {after}");
    }

    #[test]
    fn freckles_moles_and_dark_features_are_left_alone() {
        // A brown freckle, a dark mole and a neutral dark hair.
        for spot in [[168, 118, 88], [60, 42, 34], [40, 40, 40]] {
            let img = cleaned(spot, 100.0, 100.0);
            assert_eq!(*img.get_pixel(500, 500), Rgb(spot), "{spot:?}");
        }
    }

    #[test]
    fn fade_keeps_some_of_the_spot() {
        let spot = [214, 98, 92];
        let at = |fade: f32| distance(cleaned(spot, 80.0, fade).get_pixel(500, 500), SKIN);
        assert!(at(100.0) < at(50.0) && at(50.0) < at(10.0), "{} {} {}", at(100.0), at(50.0), at(10.0));
    }

    #[test]
    fn more_amount_finds_fainter_spots() {
        // A spot only a little redder than the skin.
        let faint = [212, 138, 112];
        let at = |amount: f32| distance(cleaned(faint, amount, 100.0).get_pixel(500, 500), SKIN);
        assert!(at(100.0) < at(20.0), "{} {}", at(100.0), at(20.0));
    }

    #[test]
    fn nothing_changes_when_off_or_off_skin() {
        assert_eq!(cleaned([214, 98, 92], 0.0, 100.0), patch([214, 98, 92]));
        assert_eq!(cleaned([214, 98, 92], 80.0, 0.0), patch([214, 98, 92]));
        // A red spot on blue sky is not a blemish.
        let sky = RgbImage::from_fn(1000, 1000, |x, y| {
            if (x as i32 - 500).abs() < 4 && (y as i32 - 500).abs() < 4 { Rgb([214, 98, 92]) } else { Rgb([90, 140, 220]) }
        });
        let mut img = sky.clone();
        apply_blemish(&mut img, &settings(100.0, 100.0));
        assert_eq!(img, sky);
    }

    #[test]
    fn the_skin_around_is_untouched() {
        let img = cleaned([214, 98, 92], 100.0, 100.0);
        assert_eq!(*img.get_pixel(10, 10), Rgb(SKIN));
        assert_eq!(*img.get_pixel(990, 500), Rgb(SKIN));
    }
}
