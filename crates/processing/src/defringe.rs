//! Defringe: purple and green halos (axial chromatic aberration) that
//! appear along high-contrast edges. A pixel is desaturated towards its own
//! brightness when it is near a strong edge *and* its hue is purple or
//! green, so colour elsewhere in the picture is left alone.

use image::RgbImage;

use crate::parallel::for_row_bands;

/// Pixels this far from the one being fixed (a 5x5 window) measure how
/// strong the nearby edge is.
const REACH: i32 = 2;
/// The edge strength (brightest minus darkest, 0..1) where the effect
/// starts and where it is full.
const EDGE_FROM: f32 = 0.10;
const EDGE_TO: f32 = 0.30;
/// Purple and green hue bands in degrees, each fading out over `SOFT`.
const PURPLE: (f32, f32) = (255.0, 335.0);
const GREEN: (f32, f32) = (70.0, 160.0);
const SOFT: f32 = 20.0;
/// Saturation (0..1) below which a colour is too grey to be a fringe.
const SAT_FROM: f32 = 0.12;
const SAT_TO: f32 = 0.35;

pub(crate) fn is_active(purple: f32, green: f32) -> bool {
    purple >= 0.5 || green >= 0.5
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn band(hue: f32, (lo, hi): (f32, f32)) -> f32 {
    smoothstep(lo - SOFT, lo, hue) * (1.0 - smoothstep(hi, hi + SOFT, hue))
}

/// Hue in degrees and HSV saturation of an RGB colour on 0..1.
fn hue_sat(r: f32, g: f32, b: f32) -> (f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    if d <= 0.0 || max <= 0.0 {
        return (0.0, 0.0);
    }
    let hue = if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (hue, d / max)
}

/// How far a pixel of colour `rgb` (0..1) moves towards grey, 0..1, given
/// the strength of the edge near it. `purple` and `green` are 0..1.
pub(crate) fn amount_at(rgb: [f32; 3], edge: f32, purple: f32, green: f32) -> f32 {
    let near_edge = smoothstep(EDGE_FROM, EDGE_TO, edge);
    if near_edge <= 0.0 {
        return 0.0;
    }
    let (hue, sat) = hue_sat(rgb[0], rgb[1], rgb[2]);
    let colour = purple * band(hue, PURPLE) + green * band(hue, GREEN);
    (colour * near_edge * smoothstep(SAT_FROM, SAT_TO, sat)).min(1.0)
}

/// Removes fringes from `image` in place. `purple` and `green` are
/// 0..=100.
pub(crate) fn defringe(image: &mut RgbImage, purple: f32, green: f32) {
    let (w, h) = (image.width() as usize, image.height() as usize);
    if w < 3 || h < 3 || !is_active(purple, green) {
        return;
    }
    let (purple, green) = ((purple / 100.0).clamp(0.0, 1.0), (green / 100.0).clamp(0.0, 1.0));
    let source = image.as_raw().clone();
    let luma: Vec<f32> = source
        .chunks_exact(3)
        .map(|p| (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) / 255.0)
        .collect();
    for_row_bands(&mut **image, w * 3, |first_row, band| {
        for (row, line) in band.chunks_exact_mut(w * 3).enumerate() {
            let y = (first_row + row) as i32;
            for x in 0..w as i32 {
                let (mut lo, mut hi) = (f32::MAX, f32::MIN);
                for j in -REACH..=REACH {
                    let sy = (y + j).clamp(0, h as i32 - 1) as usize;
                    for k in -REACH..=REACH {
                        let sx = (x + k).clamp(0, w as i32 - 1) as usize;
                        let l = luma[sy * w + sx];
                        lo = lo.min(l);
                        hi = hi.max(l);
                    }
                }
                let i = (y as usize * w + x as usize) * 3;
                let rgb = [
                    source[i] as f32 / 255.0,
                    source[i + 1] as f32 / 255.0,
                    source[i + 2] as f32 / 255.0,
                ];
                let amount = amount_at(rgb, hi - lo, purple, green);
                if amount > 0.0 {
                    let grey = luma[y as usize * w + x as usize];
                    let o = x as usize * 3;
                    for c in 0..3 {
                        let v = rgb[c] + (grey - rgb[c]) * amount;
                        line[o + c] = (v * 255.0).round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// Dark left half, bright right half, with a one-pixel-wide purple (or
    /// green) halo on the bright side of the edge.
    fn edge(halo: [u8; 3]) -> RgbImage {
        RgbImage::from_fn(24, 24, |x, _| match x {
            0..=11 => Rgb([20, 20, 20]),
            12 => Rgb(halo),
            _ => Rgb([235, 235, 235]),
        })
    }

    fn saturation(p: &Rgb<u8>) -> f32 {
        let max = *p.0.iter().max().unwrap() as f32;
        let min = *p.0.iter().min().unwrap() as f32;
        (max - min) / max.max(1.0)
    }

    #[test]
    fn purple_halo_is_removed_only_with_the_purple_slider() {
        let mut image = edge([190, 80, 210]);
        let before = saturation(image.get_pixel(12, 8));
        defringe(&mut image, 0.0, 100.0);
        assert_eq!(saturation(image.get_pixel(12, 8)), before, "green slider leaves purple");
        defringe(&mut image, 100.0, 0.0);
        assert!(saturation(image.get_pixel(12, 8)) < before * 0.2);
        // The grey either side is untouched.
        assert_eq!(*image.get_pixel(3, 8), Rgb([20, 20, 20]));
        assert_eq!(*image.get_pixel(20, 8), Rgb([235, 235, 235]));
    }

    #[test]
    fn green_halo_is_removed_by_the_green_slider() {
        let mut image = edge([120, 220, 90]);
        let before = saturation(image.get_pixel(12, 8));
        defringe(&mut image, 100.0, 100.0);
        assert!(saturation(image.get_pixel(12, 8)) < before * 0.2);
    }

    #[test]
    fn colour_away_from_an_edge_is_kept() {
        let mut image = RgbImage::from_pixel(24, 24, Rgb([190, 80, 210]));
        let copy = image.clone();
        defringe(&mut image, 100.0, 100.0);
        assert_eq!(image, copy);
    }
}
