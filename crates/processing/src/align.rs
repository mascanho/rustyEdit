//! Finds how far one photo is shifted against another, for lining up the
//! frames of a focus stack and the overlapping shots of a panorama.
//!
//! The search is coarse to fine on box-reduced copies: an exhaustive scan
//! of the allowed shifts on a small copy, then a small neighbourhood at
//! each finer size. Shifts are scored by normalised cross-correlation of
//! brightness over the overlap (so a difference in exposure between the
//! frames doesn't throw it off), on a thinned grid of samples to keep each
//! score cheap. Only translation is estimated.

use image::RgbImage;

/// Longest side of the smallest copy the search starts on.
const COARSE_EDGE: f32 = 128.0;
/// Samples taken per score at most.
const SAMPLE_BUDGET: usize = 60_000;
/// How many pixels around the doubled estimate each finer level tries.
const REFINE_RADIUS: i32 = 2;

/// A brightness copy of a photo, shrunk by a whole factor.
struct Gray {
    width: usize,
    height: usize,
    data: Vec<f32>,
}

impl Gray {
    fn of(img: &RgbImage, factor: usize) -> Gray {
        let (w, h) = img.dimensions();
        let (width, height) = ((w as usize / factor).max(1), (h as usize / factor).max(1));
        let mut data = vec![0.0; width * height];
        let raw = img.as_raw();
        let step = factor.max(1);
        for y in 0..height {
            for x in 0..width {
                let mut sum = 0.0;
                let mut count = 0.0;
                // Thin the box to a few taps: plenty to average out noise.
                let taps = step.min(4);
                for j in 0..taps {
                    for i in 0..taps {
                        let sx = (x * step + i * step / taps).min(w as usize - 1);
                        let sy = (y * step + j * step / taps).min(h as usize - 1);
                        let p = (sy * w as usize + sx) * 3;
                        sum += 0.299 * raw[p] as f32 + 0.587 * raw[p + 1] as f32 + 0.114 * raw[p + 2] as f32;
                        count += 1.0;
                    }
                }
                data[y * width + x] = sum / count / 255.0;
            }
        }
        Gray { width, height, data }
    }
}

/// Correlation of `b` shifted by (`dx`, `dy`) onto `a` (so `b`'s pixel
/// (x, y) sits at (x + dx, y + dy) of `a`), or `None` when they overlap by
/// less than `min_overlap` of the smaller photo.
fn score(a: &Gray, b: &Gray, dx: i32, dy: i32, min_overlap: f32) -> Option<f32> {
    let x0 = dx.max(0);
    let x1 = (a.width as i32).min(b.width as i32 + dx);
    let y0 = dy.max(0);
    let y1 = (a.height as i32).min(b.height as i32 + dy);
    if x1 - x0 < 4 || y1 - y0 < 4 {
        return None;
    }
    let overlap = ((x1 - x0) * (y1 - y0)) as f32;
    if overlap < min_overlap * (a.width * a.height).min(b.width * b.height) as f32 {
        return None;
    }
    let stride = ((overlap as usize / SAMPLE_BUDGET) as f32).sqrt().ceil().max(1.0) as i32;
    let (mut n, mut sa, mut sb, mut saa, mut sbb, mut sab) = (0.0f64, 0.0, 0.0, 0.0, 0.0, 0.0);
    let mut y = y0;
    while y < y1 {
        let mut x = x0;
        while x < x1 {
            let va = a.data[y as usize * a.width + x as usize] as f64;
            let vb = b.data[(y - dy) as usize * b.width + (x - dx) as usize] as f64;
            n += 1.0;
            sa += va;
            sb += vb;
            saa += va * va;
            sbb += vb * vb;
            sab += va * vb;
            x += stride;
        }
        y += stride;
    }
    let cov = sab - sa * sb / n;
    let var = (saa - sa * sa / n) * (sbb - sb * sb / n);
    // A flat overlap (clear sky) says nothing about the shift.
    (var > 1e-9).then(|| (cov / var.sqrt()) as f32)
}

/// How far `image` is shifted against `reference`: pixel (x, y) of `image`
/// shows what is at (x + dx, y + dy) of `reference`. Shifts up to
/// `max_shift` (fractions of width and height) are tried, with the photos
/// overlapping by at least `min_overlap` of their area. Returns
/// `(dx, dy, correlation)`; the correlation (about 1 for a good match) says
/// how much to trust it.
pub(crate) fn shift_between(
    reference: &RgbImage,
    image: &RgbImage,
    max_shift: (f32, f32),
    min_overlap: f32,
) -> (i32, i32, f32) {
    let long = reference.width().max(reference.height()) as f32;
    let mut factor = 1usize;
    while long / (factor as f32 * 2.0) >= COARSE_EDGE {
        factor *= 2;
    }

    let (mut dx, mut dy) = (0, 0);
    let mut best;
    let mut first = true;
    loop {
        let a = Gray::of(reference, factor);
        let b = Gray::of(image, factor);
        let (cx, cy, radius_x, radius_y) = if first {
            (
                0,
                0,
                (max_shift.0 * a.width as f32).ceil() as i32,
                (max_shift.1 * a.height as f32).ceil() as i32,
            )
        } else {
            (dx * 2, dy * 2, REFINE_RADIUS, REFINE_RADIUS)
        };
        best = f32::MIN;
        let (mut bx, mut by) = (cx, cy);
        for sy in cy - radius_y..=cy + radius_y {
            for sx in cx - radius_x..=cx + radius_x {
                if let Some(s) = score(&a, &b, sx, sy, min_overlap) {
                    if s > best {
                        best = s;
                        (bx, by) = (sx, sy);
                    }
                }
            }
        }
        (dx, dy) = (bx, by);
        first = false;
        if factor == 1 {
            break;
        }
        factor /= 2;
    }
    (dx, dy, best)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// A smooth-ish textured scene, wider than the window cut from it.
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
    fn a_small_shift_is_found() {
        let big = scene(420, 320);
        let a = window(&big, 20, 20, 360, 280);
        let b = window(&big, 27, 16, 360, 280);
        // b's content sits 7 right and 4 up of a's: b(x,y) = a(x+7, y-4).
        let (dx, dy, c) = shift_between(&a, &b, (0.1, 0.1), 0.5);
        assert_eq!((dx, dy), (7, -4), "corr {c}");
        assert!(c > 0.9);
    }

    #[test]
    fn an_overlapping_panorama_pair_is_found() {
        let big = scene(900, 300);
        let a = window(&big, 0, 10, 500, 280);
        let b = window(&big, 330, 14, 500, 280);
        let (dx, dy, c) = shift_between(&a, &b, (0.9, 0.1), 0.1);
        assert_eq!((dx, dy), (330, 4), "corr {c}");
    }

    #[test]
    fn a_brightness_change_does_not_move_the_match() {
        let big = scene(420, 320);
        let a = window(&big, 20, 20, 360, 280);
        let mut b = window(&big, 25, 20, 360, 280);
        for p in b.pixels_mut() {
            for c in p.0.iter_mut() {
                *c = (*c as f32 * 0.6) as u8;
            }
        }
        let (dx, dy, _) = shift_between(&a, &b, (0.1, 0.1), 0.5);
        assert_eq!((dx, dy), (5, 0));
    }
}
