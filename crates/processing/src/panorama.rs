//! Panorama stitching for a row of overlapping shots taken in order.
//!
//! Each shot is matched to the one before it by cross-correlation (see
//! `align`), which gives its place on a shared canvas; brightness is evened
//! out along the chain from the overlaps; and shots are blended with
//! weights that fall towards their own edges, so the transition happens
//! across the overlap. The result is cropped to the rows every shot covers.
//!
//! This places shots by translation only, so it suits a pan from a tripod
//! or a steady hand with a moderate lens; a very wide pan, or a close
//! subject with parallax, would need the shots reprojected first.

use image::{Rgb, RgbImage};

use crate::{align::shift_between, parallel::for_row_bands};

/// Shots may be offset this much (fractions of their size) from the last.
const MAX_SHIFT: (f32, f32) = (0.92, 0.15);
/// Neighbours must overlap by at least this fraction of a shot.
const MIN_OVERLAP: f32 = 0.08;
/// Below this correlation two shots are taken not to overlap.
const MIN_CORRELATION: f32 = 0.35;
/// Largest stitched result, in pixels.
const MAX_PIXELS: u64 = 400_000_000;

/// Stitches `images` (in shooting order) into one wide photo, or says why
/// it can't.
pub fn stitch_panorama(images: &[RgbImage]) -> Result<RgbImage, String> {
    let Some(first) = images.first() else {
        return Err("There are no photos to stitch.".into());
    };
    if images.len() == 1 {
        return Ok(first.clone());
    }

    // Where each shot's top-left falls, relative to the first's.
    let mut positions = vec![(0i32, 0i32)];
    for pair in 1..images.len() {
        let (dx, dy, correlation) =
            shift_between(&images[pair - 1], &images[pair], MAX_SHIFT, MIN_OVERLAP);
        if correlation < MIN_CORRELATION {
            return Err(format!(
                "Photo {} doesn't overlap photo {} enough to stitch (they need about a third in common).",
                pair + 1,
                pair
            ));
        }
        let (px, py) = positions[pair - 1];
        positions.push((px + dx, py + dy));
    }

    let min_x = positions.iter().map(|p| p.0).min().unwrap_or(0);
    let min_y = positions.iter().map(|p| p.1).min().unwrap_or(0);
    let positions: Vec<(i32, i32)> = positions.iter().map(|p| (p.0 - min_x, p.1 - min_y)).collect();
    let width = positions
        .iter()
        .zip(images)
        .map(|(p, i)| p.0 + i.width() as i32)
        .max()
        .unwrap_or(0);
    // Rows every shot covers.
    let top = positions.iter().map(|p| p.1).max().unwrap_or(0);
    let bottom = positions
        .iter()
        .zip(images)
        .map(|(p, i)| p.1 + i.height() as i32)
        .min()
        .unwrap_or(0);
    if bottom - top < 16 {
        return Err("The photos drift too far apart vertically to stitch.".into());
    }
    let height = bottom - top;
    if width as u64 * height as u64 > MAX_PIXELS {
        return Err("The stitched photo would be too large.".into());
    }

    let gains = brightness_gains(images, &positions);

    let (width, height) = (width as usize, height as usize);
    let mut out = RgbImage::new(width as u32, height as u32);
    let buffer: &mut [u8] = &mut out;
    for_row_bands(buffer, width * 3, |first_row, band| {
        let rows = band.len() / (width * 3);
        let mut sum = vec![0.0f32; width * 3];
        let mut weight = vec![0.0f32; width];
        for r in 0..rows {
            let y = first_row as i32 + r as i32 + top;
            sum.fill(0.0);
            weight.fill(0.0);
            for ((img, &(px, py)), &gain) in images.iter().zip(&positions).zip(&gains) {
                let ly = y - py;
                if ly < 0 || ly >= img.height() as i32 {
                    continue;
                }
                let wy = (ly + 1).min(img.height() as i32 - ly) as f32;
                let w = img.width() as i32;
                for lx in 0..w {
                    let x = (px + lx) as usize;
                    let wx = (lx + 1).min(w - lx) as f32;
                    // Squared across, so the hand-over is quick in the
                    // middle of an overlap and ghosting stays short.
                    let weight_here = wx * wx * wy;
                    let Rgb(p) = *img.get_pixel(lx as u32, ly as u32);
                    for c in 0..3 {
                        sum[x * 3 + c] += p[c] as f32 * gain[c] * weight_here;
                    }
                    weight[x] += weight_here;
                }
            }
            let row = &mut band[r * width * 3..(r + 1) * width * 3];
            for x in 0..width {
                let w = weight[x].max(1e-6);
                for c in 0..3 {
                    row[x * 3 + c] = (sum[x * 3 + c] / w).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    });
    Ok(out)
}

/// A per-channel gain for each shot that evens out its brightness with the
/// shot before it, chained from the first (gain 1).
fn brightness_gains(images: &[RgbImage], positions: &[(i32, i32)]) -> Vec<[f32; 3]> {
    let mut gains = vec![[1.0f32; 3]];
    for i in 1..images.len() {
        let (a, b) = (&images[i - 1], &images[i]);
        let (ax, ay) = positions[i - 1];
        let (bx, by) = positions[i];
        let x0 = ax.max(bx);
        let x1 = (ax + a.width() as i32).min(bx + b.width() as i32);
        let y0 = ay.max(by);
        let y1 = (ay + a.height() as i32).min(by + b.height() as i32);
        let mut mean_a = [0.0f64; 3];
        let mut mean_b = [0.0f64; 3];
        let mut n = 0.0;
        let step = (((x1 - x0).max(1) as usize * (y1 - y0).max(1) as usize) / 40_000)
            .max(1)
            .isqrt() as i32;
        let step = step.max(1);
        let mut y = y0;
        while y < y1 {
            let mut x = x0;
            while x < x1 {
                let pa = a.get_pixel((x - ax) as u32, (y - ay) as u32).0;
                let pb = b.get_pixel((x - bx) as u32, (y - by) as u32).0;
                for c in 0..3 {
                    mean_a[c] += pa[c] as f64 * gains[i - 1][c] as f64;
                    mean_b[c] += pb[c] as f64;
                }
                n += 1.0;
                x += step;
            }
            y += step;
        }
        let mut g = [1.0f32; 3];
        if n > 0.0 {
            for c in 0..3 {
                if mean_b[c] > 1.0 {
                    g[c] = ((mean_a[c] / mean_b[c]) as f32).clamp(0.6, 1.6);
                }
            }
        }
        gains.push(g);
    }
    gains
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| {
            let (fx, fy) = (x as f32, y as f32);
            let v = 128.0
                + 60.0 * (fx * 0.11).sin() * (fy * 0.07).cos()
                + 40.0 * (fx * 0.031 + fy * 0.047).sin()
                + 25.0 * ((fx * 0.23).sin() * (fy * 0.19).sin());
            let v = v.clamp(0.0, 255.0) as u8;
            Rgb([v, v.saturating_sub(10), v / 2])
        })
    }

    fn window(img: &RgbImage, x: u32, y: u32, w: u32, h: u32) -> RgbImage {
        image::imageops::crop_imm(img, x, y, w, h).to_image()
    }

    #[test]
    fn three_overlapping_shots_become_one_wide_photo() {
        let big = scene(1100, 320);
        let shots = [
            window(&big, 0, 10, 480, 280),
            window(&big, 320, 14, 480, 280),
            window(&big, 620, 8, 480, 280),
        ];
        let pano = stitch_panorama(&shots).unwrap();
        assert_eq!(pano.width(), 1100);
        // Rows covered by every shot: 14 .. 10 + 280.
        assert_eq!(pano.height(), 274);
        // Matches the scene it was cut from.
        let mut worst = 0i32;
        for x in (0..pano.width()).step_by(7) {
            for y in (0..pano.height()).step_by(11) {
                let got = pano.get_pixel(x, y).0[0] as i32;
                let want = big.get_pixel(x, y + 14).0[0] as i32;
                worst = worst.max((got - want).abs());
            }
        }
        assert!(worst <= 4, "worst difference {worst}");
    }

    #[test]
    fn unrelated_photos_are_refused() {
        let a = scene(300, 200);
        let b = RgbImage::from_fn(300, 200, |x, y| {
            let v = ((x * 37 + y * 91 + (x * y) % 17) % 255) as u8;
            Rgb([v, v, v])
        });
        assert!(stitch_panorama(&[a, b]).is_err());
    }

    #[test]
    fn a_darker_middle_shot_is_evened_out() {
        let big = scene(900, 300);
        let mut dark = window(&big, 330, 10, 480, 280);
        for p in dark.pixels_mut() {
            for c in p.0.iter_mut() {
                *c = (*c as f32 * 0.8) as u8;
            }
        }
        let pano = stitch_panorama(&[window(&big, 0, 10, 480, 280), dark]).unwrap();
        let (got, want) = (pano.get_pixel(700, 100).0[0] as i32, big.get_pixel(700, 110).0[0] as i32);
        assert!((got - want).abs() <= 8, "{got} vs {want}");
    }
}
