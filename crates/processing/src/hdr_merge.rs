//! HDR exposure merge: blends a bracketed set of exposures of the same,
//! already-aligned scene (a tripod sequence) into one well-exposed photo,
//! by exposure fusion (Mertens, Kautz & Van Reeth 2007). Unlike tone
//! mapping a single 32-bit radiance map, fusion works directly on the
//! ordinary (already developed) exposures: at every pixel, each input
//! gets a weight from how well-exposed, contrasty and saturated it is
//! there, and the weighted images are blended across several scales (a
//! Laplacian pyramid) so there's no visible seam where one exposure's
//! contribution fades out and another's takes over.
//!
//! There's no alignment step: the inputs must already share the same
//! pixel grid (a tripod bracket, not a handheld one with drift between
//! frames).

use image::{Rgb, RgbImage};

/// Standard deviation of the well-exposedness curve (in 0..=1), i.e. how
/// far a pixel's value may sit from mid-grey before it stops counting as
/// "well exposed". Mertens' own choice; it works well in practice.
const EXPOSEDNESS_SIGMA: f32 = 0.2;
/// Pyramid levels, at most: each halves the image, and enough of them
/// blend without a seam at typical photo resolutions while staying fast.
/// A small input gets fewer, down to `max_levels`.
const MAX_PYRAMID_LEVELS: usize = 8;
/// A 5-tap binomial blur, used both to anti-alias a pyramid level before
/// it's halved and, source and target swapped, to blur after doubling it
/// back up.
const BINOMIAL: [f32; 5] = [1.0 / 16.0, 4.0 / 16.0, 6.0 / 16.0, 4.0 / 16.0, 1.0 / 16.0];

/// A `width` x `height` x `channels` float buffer, row-major and
/// channel-interleaved: the shape used throughout this module, for both a
/// photo (3 channels) and a weight map (1).
#[derive(Clone)]
struct Buf {
    data: Vec<f32>,
    width: usize,
    height: usize,
    channels: usize,
}

impl Buf {
    fn new(width: usize, height: usize, channels: usize) -> Self {
        Self { data: vec![0.0; width * height * channels], width, height, channels }
    }

    /// Clamp-to-edge sample, so a convolution near a border repeats the
    /// edge pixel instead of reading outside the buffer.
    fn at(&self, x: isize, y: isize, c: usize) -> f32 {
        let x = x.clamp(0, self.width as isize - 1) as usize;
        let y = y.clamp(0, self.height as isize - 1) as usize;
        self.data[(y * self.width + x) * self.channels + c]
    }

    fn set(&mut self, x: usize, y: usize, c: usize, v: f32) {
        self.data[(y * self.width + x) * self.channels + c] = v;
    }
}

/// Fuses `images` (all the same size) into one well-exposed photo. `None`
/// if there are no images, or they don't all share the same dimensions
/// (there's no attempt to align mismatched framing).
pub fn merge_exposures(images: &[RgbImage]) -> Option<RgbImage> {
    let first = images.first()?;
    let (width, height) = first.dimensions();
    if images.iter().any(|img| img.dimensions() != (width, height)) {
        return None;
    }
    if images.len() == 1 {
        return Some(first.clone());
    }

    let n = images.len();
    let mut weights: Vec<Buf> = images.iter().map(|img| weight_map(img)).collect();
    normalize_weights(&mut weights);

    let levels = pyramid_levels(width as usize, height as usize);
    let mut blended: Vec<Buf> = vec![Buf::new(0, 0, 0); levels];

    for i in 0..n {
        let gaussian_image = gaussian_pyramid(&image_to_buf(&images[i]), levels);
        let laplacian_image = laplacian_pyramid(&gaussian_image);
        let gaussian_weight = gaussian_pyramid(&weights[i], levels);
        for level in 0..levels {
            let lap = &laplacian_image[level];
            let wt = &gaussian_weight[level];
            let out = &mut blended[level];
            if out.width == 0 {
                *out = Buf::new(lap.width, lap.height, lap.channels);
            }
            let pixels = lap.width * lap.height;
            for p in 0..pixels {
                let w = wt.data[p];
                for c in 0..lap.channels {
                    out.data[p * lap.channels + c] += w * lap.data[p * lap.channels + c];
                }
            }
        }
    }

    // Collapse: start from the coarsest (smoothest) level and expand each
    // one up into the next finer level, adding its detail back in.
    let mut result = blended[levels - 1].clone();
    for level in (0..levels - 1).rev() {
        let target = &blended[level];
        let expanded = expand(&result, target.width, target.height);
        result = add(&expanded, target);
    }

    Some(buf_to_image(&result))
}

/// How many times `width` x `height` roughly halves before either side
/// would drop below 8px, capped at [`MAX_PYRAMID_LEVELS`].
fn pyramid_levels(width: usize, height: usize) -> usize {
    let mut levels = 1;
    let (mut w, mut h) = (width, height);
    while w > 8 && h > 8 && levels < MAX_PYRAMID_LEVELS {
        w = w.div_ceil(2);
        h = h.div_ceil(2);
        levels += 1;
    }
    levels
}

/// This image's weight at every pixel: how much local contrast it has,
/// how saturated it is, and how close to mid-grey (neither blown out nor
/// buried in shadow) — the three cues Mertens fusion blends by. Each
/// image's own weight is relative, not absolute; [`normalize_weights`]
/// turns these into a 0..=1 share of the final blend per pixel.
fn weight_map(img: &RgbImage) -> Buf {
    let (w, h) = img.dimensions();
    let (w, h) = (w as usize, h as usize);
    let gray: Vec<f32> = img
        .pixels()
        .map(|p| {
            let [r, g, b] = p.0.map(|v| v as f32 / 255.0);
            0.299 * r + 0.587 * g + 0.114 * b
        })
        .collect();

    let mut weights = Buf::new(w, h, 1);
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let at = |xx: isize, yy: isize| -> f32 {
                let xx = xx.clamp(0, w as isize - 1) as usize;
                let yy = yy.clamp(0, h as isize - 1) as usize;
                gray[yy * w + xx]
            };
            let (xi, yi) = (x as isize, y as isize);
            let laplacian = at(xi - 1, yi) + at(xi + 1, yi) + at(xi, yi - 1) + at(xi, yi + 1)
                - 4.0 * at(xi, yi);
            let contrast = laplacian.abs();

            let Rgb([r, g, b]) = *img.get_pixel(x as u32, y as u32);
            let [r, g, b] = [r, g, b].map(|v| v as f32 / 255.0);
            let mean = (r + g + b) / 3.0;
            let saturation =
                (((r - mean).powi(2) + (g - mean).powi(2) + (b - mean).powi(2)) / 3.0).sqrt();
            let well_exposed = [r, g, b]
                .iter()
                .map(|&v| {
                    let d = v - 0.5;
                    (-d * d / (2.0 * EXPOSEDNESS_SIGMA * EXPOSEDNESS_SIGMA)).exp()
                })
                .product::<f32>();

            // Small floors: a flat, neutral patch (a clear sky, a studio
            // backdrop) shouldn't fall to exactly zero weight in every
            // input at once, which would starve the blend of a pixel.
            weights.data[idx] = (contrast + 1e-4) * (saturation + 0.03) * (well_exposed + 0.03);
        }
    }
    weights
}

/// Scales every image's weight at each pixel so they sum to 1 there (an
/// even 1/n share if every input's weight came out at ~0, e.g. a pixel
/// that's clipped white in every bracket).
fn normalize_weights(weights: &mut [Buf]) {
    let n = weights.len();
    let pixels = weights[0].data.len();
    for p in 0..pixels {
        let sum: f32 = weights.iter().map(|w| w.data[p]).sum();
        if sum > 1e-6 {
            for w in weights.iter_mut() {
                w.data[p] /= sum;
            }
        } else {
            let even = 1.0 / n as f32;
            for w in weights.iter_mut() {
                w.data[p] = even;
            }
        }
    }
}

fn image_to_buf(img: &RgbImage) -> Buf {
    let (w, h) = img.dimensions();
    let mut buf = Buf::new(w as usize, h as usize, 3);
    for (i, p) in img.pixels().enumerate() {
        for c in 0..3 {
            buf.data[i * 3 + c] = p.0[c] as f32 / 255.0;
        }
    }
    buf
}

fn buf_to_image(buf: &Buf) -> RgbImage {
    let mut img = RgbImage::new(buf.width as u32, buf.height as u32);
    for (i, px) in img.pixels_mut().enumerate() {
        for c in 0..3 {
            px.0[c] = (buf.data[i * 3 + c].clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    img
}

/// Blurs with the 5-tap binomial kernel, then keeps every other sample:
/// an anti-aliased halving, separable into a horizontal then a vertical
/// pass.
fn reduce(src: &Buf) -> Buf {
    let half_w = src.width.div_ceil(2).max(1);
    let half_h = src.height.div_ceil(2).max(1);

    let mut horizontal = Buf::new(half_w, src.height, src.channels);
    for y in 0..src.height {
        for x in 0..half_w {
            let sx = (x * 2) as isize;
            for c in 0..src.channels {
                let sum: f32 = (0..5)
                    .map(|k| BINOMIAL[k] * src.at(sx + k as isize - 2, y as isize, c))
                    .sum();
                horizontal.set(x, y, c, sum);
            }
        }
    }

    let mut out = Buf::new(half_w, half_h, src.channels);
    for y in 0..half_h {
        let sy = (y * 2) as isize;
        for x in 0..half_w {
            for c in 0..src.channels {
                let sum: f32 = (0..5)
                    .map(|k| BINOMIAL[k] * horizontal.at(x as isize, sy + k as isize - 2, c))
                    .sum();
                out.set(x, y, c, sum);
            }
        }
    }
    out
}

/// Upsamples `src` to exactly `width` x `height` by bilinear interpolation
/// (close enough to invert [`reduce`] for blending purposes, and far
/// simpler and less error-prone than a true zero-stuffed reconstruction).
fn expand(src: &Buf, width: usize, height: usize) -> Buf {
    let mut out = Buf::new(width, height, src.channels);
    let sx_scale = src.width as f32 / width as f32;
    let sy_scale = src.height as f32 / height as f32;
    for y in 0..height {
        let fy = (y as f32 + 0.5) * sy_scale - 0.5;
        let y0 = fy.floor();
        let ty = fy - y0;
        for x in 0..width {
            let fx = (x as f32 + 0.5) * sx_scale - 0.5;
            let x0 = fx.floor();
            let tx = fx - x0;
            for c in 0..src.channels {
                let v00 = src.at(x0 as isize, y0 as isize, c);
                let v10 = src.at(x0 as isize + 1, y0 as isize, c);
                let v01 = src.at(x0 as isize, y0 as isize + 1, c);
                let v11 = src.at(x0 as isize + 1, y0 as isize + 1, c);
                let top = v00 + (v10 - v00) * tx;
                let bottom = v01 + (v11 - v01) * tx;
                out.set(x, y, c, top + (bottom - top) * ty);
            }
        }
    }
    out
}

fn add(a: &Buf, b: &Buf) -> Buf {
    let mut out = a.clone();
    for (v, &bv) in out.data.iter_mut().zip(&b.data) {
        *v += bv;
    }
    out
}

/// `levels` Gaussian pyramid levels, `[0]` the full-size `base`, each
/// after it `reduce`d from the one before.
fn gaussian_pyramid(base: &Buf, levels: usize) -> Vec<Buf> {
    let mut pyramid = Vec::with_capacity(levels);
    pyramid.push(base.clone());
    for _ in 1..levels {
        pyramid.push(reduce(pyramid.last().unwrap()));
    }
    pyramid
}

/// The detail lost at each level of `gaussian` going down one level: the
/// difference between it and the level below expanded back up to its
/// size. The last (coarsest) level has nothing below it, so it's kept as
/// is — fusion blends it exactly like a Laplacian level, since weighted
/// blending plus collapsing works the same whether a level holds detail
/// or (at the coarsest) the smoothed base image itself.
fn laplacian_pyramid(gaussian: &[Buf]) -> Vec<Buf> {
    let levels = gaussian.len();
    let mut laplacian = Vec::with_capacity(levels);
    for level in 0..levels - 1 {
        let fine = &gaussian[level];
        let expanded = expand(&gaussian[level + 1], fine.width, fine.height);
        let mut diff = fine.clone();
        for (v, &ev) in diff.data.iter_mut().zip(&expanded.data) {
            *v -= ev;
        }
        laplacian.push(diff);
    }
    laplacian.push(gaussian[levels - 1].clone());
    laplacian
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, rgb: [u8; 3]) -> RgbImage {
        RgbImage::from_fn(width, height, |_, _| Rgb(rgb))
    }

    #[test]
    fn one_image_is_returned_as_is() {
        let img = solid(8, 8, [120, 130, 140]);
        let merged = merge_exposures(std::slice::from_ref(&img)).unwrap();
        assert_eq!(merged, img);
    }

    #[test]
    fn mismatched_sizes_refuse_to_merge() {
        let a = solid(8, 8, [100, 100, 100]);
        let b = solid(9, 8, [100, 100, 100]);
        assert!(merge_exposures(&[a, b]).is_none());
    }

    #[test]
    fn the_well_exposed_bracket_wins_over_a_blown_out_one() {
        // A bracket of three: a clipped-white frame, a well-exposed mid
        // frame, and a crushed-black frame. The merge should land close to
        // the well-exposed one, not simply average all three towards grey.
        let bright = solid(32, 32, [255, 255, 255]);
        let mid = solid(32, 32, [128, 128, 128]);
        let dark = solid(32, 32, [8, 8, 8]);
        let merged = merge_exposures(&[bright, mid, dark]).unwrap();
        let Rgb([r, g, b]) = *merged.get_pixel(16, 16);
        assert!(
            (100..160).contains(&r) && (100..160).contains(&g) && (100..160).contains(&b),
            "expected close to the well-exposed 128, got {r},{g},{b}"
        );
    }

    #[test]
    fn blending_two_halves_of_a_scene_has_no_sharp_seam() {
        // Two brackets that each only "see" (are well-exposed on) one half
        // of a photo split down the middle: the merge should blend smoothly
        // across the boundary rather than jump abruptly.
        let width = 64;
        let mut left_bright = RgbImage::new(width, 32);
        let mut right_bright = RgbImage::new(width, 32);
        for y in 0..32 {
            for x in 0..width {
                let on_left = x < width / 2;
                left_bright.put_pixel(x, y, Rgb(if on_left { [128, 128, 128] } else { [8, 8, 8] }));
                right_bright
                    .put_pixel(x, y, Rgb(if on_left { [8, 8, 8] } else { [128, 128, 128] }));
            }
        }
        let merged = merge_exposures(&[left_bright, right_bright]).unwrap();
        let mut max_step = 0i32;
        for x in 1..width {
            let Rgb([a, ..]) = *merged.get_pixel(x - 1, 16);
            let Rgb([b, ..]) = *merged.get_pixel(x, 16);
            max_step = max_step.max((a as i32 - b as i32).abs());
        }
        assert!(max_step < 40, "expected a smooth blend, biggest single step was {max_step}");
    }
}
