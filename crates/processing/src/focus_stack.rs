//! Focus stacking: combines shots of one scene taken at different focus
//! distances into a single photo that is sharp throughout, by taking each
//! part from the frame where it is sharpest.
//!
//! Frames are first lined up (focusing moves the image a little, and a
//! handheld set drifts), cropping to the area every frame covers. Each
//! frame's sharpness at a pixel is its smoothed local contrast; weights
//! are sharpened (raised to a power) so the sharpest frame dominates, and
//! the frames are blended across a pyramid, which hides the seams between
//! regions taken from different frames.

use image::RgbImage;

use crate::{
    align::shift_between,
    hdr_merge::{fuse, normalize_weights, Buf},
};

/// Sharpness is raised to this, so the sharpest frame takes nearly all of a
/// pixel instead of sharing it evenly.
const SHARPNESS_POWER: f32 = 4.0;
/// Frames this much (fraction of the size) out of line are searched for.
const MAX_SHIFT: f32 = 0.04;
/// Alignments with a weaker correlation are left alone.
const MIN_CORRELATION: f32 = 0.5;

/// Stacks `images` (the same scene, focused differently). Frames are
/// aligned to the first. `None` if there are no frames or their sizes
/// differ.
pub fn stack_focus(images: &[RgbImage]) -> Option<RgbImage> {
    let first = images.first()?;
    let size = first.dimensions();
    if images.iter().any(|i| i.dimensions() != size) {
        return None;
    }
    if images.len() == 1 {
        return Some(first.clone());
    }

    // Offsets of every frame against the first.
    let offsets: Vec<(i32, i32)> = images
        .iter()
        .enumerate()
        .map(|(i, img)| {
            if i == 0 {
                return (0, 0);
            }
            let (dx, dy, c) = shift_between(first, img, (MAX_SHIFT, MAX_SHIFT), 0.8);
            if c >= MIN_CORRELATION {
                (dx, dy)
            } else {
                (0, 0)
            }
        })
        .collect();

    // The area covered by every frame, in the first frame's coordinates.
    let (w, h) = (size.0 as i32, size.1 as i32);
    let left = offsets.iter().map(|o| o.0).max()?.max(0);
    let top = offsets.iter().map(|o| o.1).max()?.max(0);
    let right = w + offsets.iter().map(|o| o.0).min()?.min(0);
    let bottom = h + offsets.iter().map(|o| o.1).min()?.min(0);
    if right - left < 16 || bottom - top < 16 {
        return None;
    }
    let (cw, ch) = ((right - left) as u32, (bottom - top) as u32);
    let aligned: Vec<RgbImage> = images
        .iter()
        .zip(&offsets)
        .map(|(img, &(dx, dy))| {
            // Frame pixel (x, y) is at (x + dx, y + dy) of the first.
            image::imageops::crop_imm(img, (left - dx) as u32, (top - dy) as u32, cw, ch).to_image()
        })
        .collect();

    let mut weights: Vec<Buf> = aligned.iter().map(sharpness).collect();
    normalize_weights(&mut weights);
    Some(fuse(&aligned, &weights))
}

/// How sharp the frame is at each pixel: local contrast, spread over a
/// small neighbourhood so noise in a flat area doesn't win, then raised to
/// [`SHARPNESS_POWER`].
fn sharpness(img: &RgbImage) -> Buf {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let gray: Vec<f32> = img
        .pixels()
        .map(|p| (0.299 * p.0[0] as f32 + 0.587 * p.0[1] as f32 + 0.114 * p.0[2] as f32) / 255.0)
        .collect();
    let at = |x: isize, y: isize| {
        gray[y.clamp(0, h as isize - 1) as usize * w + x.clamp(0, w as isize - 1) as usize]
    };
    let mut contrast = Buf::new(w, h, 1);
    for y in 0..h {
        for x in 0..w {
            let (xi, yi) = (x as isize, y as isize);
            let lap = at(xi - 1, yi) + at(xi + 1, yi) + at(xi, yi - 1) + at(xi, yi + 1)
                - 4.0 * at(xi, yi);
            contrast.data[y * w + x] = lap.abs();
        }
    }
    // Spread: box blur 5x5 (two passes of a separable mean).
    let blurred = box_blur(&contrast, 2);
    let mut out = blurred;
    for v in out.data.iter_mut() {
        *v = (*v + 1e-5).powf(SHARPNESS_POWER);
    }
    out
}

fn box_blur(src: &Buf, radius: isize) -> Buf {
    let (w, h) = (src.width, src.height);
    let mut tmp = Buf::new(w, h, 1);
    let norm = 1.0 / (2 * radius + 1) as f32;
    for y in 0..h {
        for x in 0..w {
            let mut s = 0.0;
            for k in -radius..=radius {
                s += src.at(x as isize + k, y as isize, 0);
            }
            tmp.data[y * w + x] = s * norm;
        }
    }
    let mut out = Buf::new(w, h, 1);
    for y in 0..h {
        for x in 0..w {
            let mut s = 0.0;
            for k in -radius..=radius {
                s += tmp.at(x as isize, y as isize + k, 0);
            }
            out.data[y * w + x] = s * norm;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// Fine texture, blurred wherever `sharp` says not.
    fn frame(w: u32, h: u32, sharp: impl Fn(u32, u32) -> bool) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| {
            let texture = if (x / 2 + y / 2) % 2 == 0 { 200 } else { 60 };
            let v = if sharp(x, y) { texture } else { 130 };
            Rgb([v, v, v])
        })
    }

    #[test]
    fn each_half_comes_from_the_frame_where_it_is_sharp() {
        let a = frame(96, 64, |x, _| x < 48);
        let b = frame(96, 64, |x, _| x >= 48);
        let out = stack_focus(&[a, b]).unwrap();
        // Contrast (spread of values) in each half should be high.
        let spread = |x0: u32, x1: u32| {
            let vals: Vec<u8> = (x0..x1).map(|x| out.get_pixel(x, 32).0[0]).collect();
            vals.iter().max().unwrap() - vals.iter().min().unwrap()
        };
        assert!(spread(8, 32) > 100, "left {}", spread(8, 32));
        assert!(spread(64, 88) > 100, "right {}", spread(64, 88));
    }

    #[test]
    fn differently_sized_frames_cannot_be_stacked() {
        let a = frame(40, 40, |_, _| true);
        let b = frame(44, 40, |_, _| true);
        assert!(stack_focus(&[a, b]).is_none());
    }
}
