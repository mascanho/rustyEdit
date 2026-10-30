//! Lens blur (see `rustyedit_core::LensBlur`), the CPU path; the shader's
//! `lens_coc` / `lens_blur` kernels do the same on the GPU.
//!
//! Two steps over the photo before the crop:
//!
//! 1. How blurred each pixel should be: its circle of confusion (CoC), from
//!    the focus mask and Falloff, up to the Amount's largest radius.
//! 2. Each pixel gathers [`SAMPLES`] points spread over a disc of its CoC
//!    (a golden-angle spiral; hexagonal for Blades, clipped into a cat's
//!    eye towards the edges, rim-weighted for Bubble), from a lightly
//!    softened copy so the samples blend. A sample counts only as far as
//!    its own CoC reaches back, so the sharp subject never smears into the
//!    blurred background around it.
//! 3. Bokeh: points much brighter than their surroundings (lights,
//!    glints; not a bright sky) are picked out, and their light is spread
//!    over the same discs and added back, scaled by Boost, so each turns
//!    into a bright disc the way a real lens renders it.

use image::RgbImage;
use rustyedit_core::{BokehShape, DevelopSettings, Gradient, LensBlur};

use crate::parallel::for_row_bands;

/// Most points a blurred pixel gathers (at the largest blur).
pub(crate) const SAMPLES: u32 = 64;

/// The sample densities: a pixel gathers the smallest that covers its
/// disc evenly (about two points per pixel of radius, the softened copy
/// filling between them).
pub(crate) const LEVELS: [u32; 3] = [16, 32, SAMPLES];

/// Which of [`LEVELS`] a pixel of CoC `r` uses.
pub(crate) fn level(r: f32) -> usize {
    let wanted = r * 2.0;
    if wanted <= 16.0 {
        0
    } else if wanted <= 32.0 {
        1
    } else {
        2
    }
}

/// Every level's samples for `shape`, one after another: (x, y, weight)
/// within a unit disc, before Cat's eye clipping (which depends on where
/// the pixel is). The GPU reads the same table.
pub(crate) fn sample_table(shape: BokehShape) -> Vec<[f32; 3]> {
    LEVELS
        .iter()
        .flat_map(|&count| {
            (0..count).map(move |i| {
                let ((x, y), w) = sample_offset(i, count, shape, (0.0, 0.0))
                    .expect("no clipping at the centre");
                [x, y, w]
            })
        })
        .collect()
}

/// Where level `level`'s samples start in [`sample_table`].
pub(crate) fn level_start(level: usize) -> usize {
    LEVELS[..level].iter().sum::<u32>() as usize
}

/// Cat's eye: whether a sample at unit offset (`x`, `y`) survives for a
/// pixel at `edge` (-1..=1 from the centre).
pub(crate) fn cat_eye_keeps(x: f32, y: f32, edge: (f32, f32)) -> bool {
    (x + edge.0 * 0.6).powi(2) + (y + edge.1 * 0.6).powi(2) <= 1.0
}
const GOLDEN_ANGLE: f32 = 2.399_963_2;

/// The Gaussian the samples are taken from, so 64 of them spread over a
/// large disc blend smoothly instead of showing as copies: about half
/// the spacing between samples at the largest radius.
pub(crate) fn pre_blur_sigma(max_radius: f32) -> f32 {
    (max_radius * 0.17).max(0.6)
}

/// How much of a sample comes from the pre-blurred copy for a pixel of
/// CoC `r`: none for slight blur (kept crisp), all at the full radius.
pub(crate) fn pre_blur_mix(r: f32, max_radius: f32) -> f32 {
    (r / max_radius.max(1.0) * 1.2).clamp(0.0, 1.0)
}

/// Whether the photo needs a lens blur pass.
pub(crate) fn is_active(s: &DevelopSettings) -> bool {
    s.lens_blur.is_active()
}

/// The focus mask as drawn on the oriented photo.
pub(crate) fn focus_layer(s: &DevelopSettings) -> Gradient {
    LensBlur::focus(&s.gradients).on_display(s)
}

/// The wide blur a point of light must stand out from: about half the
/// largest disc, so bright texture (much of its area bright) doesn't.
pub(crate) fn surround_sigma(max_radius: f32) -> f32 {
    (max_radius * 0.25).max(1.0)
}

/// How much of a pixel is a point of light to bloom into bokeh: bright,
/// and far brighter than its wide surroundings (`around`).
pub(crate) fn highlight(luma: f32, around: f32) -> f32 {
    let bright = {
        let t = ((luma - 0.55) / 0.35).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    bright * ((luma - around - 0.3) / 0.25).clamp(0.0, 1.0)
}

/// How strongly the spread-out highlights are added, at Boost `boost`
/// (0..=1).
pub(crate) fn bokeh_gain(boost: f32) -> f32 {
    boost * 40.0
}

/// Most light the bloom adds, so crowded highlights (bright texture, a
/// row of lamps) level off smoothly instead of burning out.
const BLOOM_CAP: f32 = 0.6;

/// The factor the spread-out light is added at, for `density`, the share
/// of the disc lit by points of light: the full gain for an isolated
/// light, easing off as they crowd.
pub(crate) fn bloom(gain: f32, density: f32) -> f32 {
    gain / (1.0 + density * gain / BLOOM_CAP)
}

/// Sample `i`'s offset within a unit disc for `shape`, and its weight
/// from the shape (Bubble's bright rim). `None` when Cat's eye clips it.
/// `edge` is where the pixel sits: -1..=1 from the photo's centre.
pub(crate) fn sample_offset(
    i: u32,
    count: u32,
    shape: BokehShape,
    edge: (f32, f32),
) -> Option<((f32, f32), f32)> {
    let t = (i as f32 + 0.5) / count as f32;
    let mut r = t.sqrt();
    let angle = i as f32 * GOLDEN_ANGLE;
    let (sin, cos) = angle.sin_cos();
    let mut weight = 1.0;
    match shape {
        BokehShape::Circle => {}
        BokehShape::Blades => {
            // Inside a hexagon, turned a little off level.
            let sector = std::f32::consts::FRAC_PI_3;
            let a = (angle + 0.26).rem_euclid(sector) - sector / 2.0;
            r *= (sector / 2.0).cos() / a.cos();
        }
        BokehShape::CatEye => {}
        BokehShape::Bubble => weight = 0.35 + 1.4 * r.powi(4),
    }
    let offset = (cos * r, sin * r);
    if shape == BokehShape::CatEye {
        // The disc's overlap with a second one pushed outwards: a cat's
        // eye towards the corners, a full disc in the middle.
        let shift = (edge.0 * 0.6, edge.1 * 0.6);
        if (offset.0 + shift.0).powi(2) + (offset.1 + shift.1).powi(2) > 1.0 {
            return None;
        }
    }
    Some((offset, weight))
}

/// Applies the lens blur to `image` (the oriented photo, before the crop).
pub(crate) fn apply(image: &mut RgbImage, s: &DevelopSettings) {
    let lens = s.lens_blur;
    let (w, h) = image.dimensions();
    let max_r = lens.max_radius(w, h);
    if max_r < 0.5 || w < 2 || h < 2 {
        return;
    }
    let focus = focus_layer(s);
    let (wf, hf) = (w as f32, h as f32);
    let colours: Vec<[f32; 3]> = image
        .pixels()
        .map(|p| p.0.map(|v| v as f32 / 255.0))
        .collect();
    let coc: Vec<f32> = (0..(w * h) as usize)
        .map(|i| {
            let (x, y) = ((i as u32 % w) as f32 + 0.5, (i as u32 / w) as f32 + 0.5);
            let mut m = focus.mask_at(x, y, wf, hf);
            if m > 0.001 {
                m *= focus.range_at(colours[i]);
            }
            max_r * lens.blur_at(m)
        })
        .collect();
    let gain = bokeh_gain((lens.boost / 100.0).clamp(0.0, 1.0));
    let sigma = pre_blur_sigma(max_r);
    let soft: Vec<[f32; 3]> = image::imageops::blur(&*image, sigma)
        .pixels()
        .map(|p| p.0.map(|v| v as f32 / 255.0))
        .collect();
    // The points of light, softened like the colours (none without Boost).
    let lights: Vec<[f32; 3]> = if gain > 0.0 {
        let around: Vec<f32> = image::imageops::blur(&*image, surround_sigma(max_r))
            .pixels()
            .map(|p| luma(p.0.map(|v| v as f32 / 255.0)))
            .collect();
        let lights = RgbImage::from_fn(w, h, |x, y| {
            let i = (y * w + x) as usize;
            let k = highlight(luma(colours[i]), around[i]);
            image::Rgb(colours[i].map(|v| (v * k * 255.0).round() as u8))
        });
        image::imageops::blur(&lights, sigma)
            .pixels()
            .map(|p| p.0.map(|v| v as f32 / 255.0))
            .collect()
    } else {
        Vec::new()
    };
    let bilinear = |colours: &[[f32; 3]], x: f32, y: f32| -> [f32; 3] {
        let x = (x - 0.5).clamp(0.0, wf - 1.0);
        let y = (y - 0.5).clamp(0.0, hf - 1.0);
        let (x0, y0) = (x.floor() as u32, y.floor() as u32);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (tx, ty) = (x - x0 as f32, y - y0 as f32);
        let at = |x: u32, y: u32| colours[(y * w + x) as usize];
        let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
        std::array::from_fn(|k| {
            let top = a[k] + (b[k] - a[k]) * tx;
            let bottom = c[k] + (d[k] - c[k]) * tx;
            top + (bottom - top) * ty
        })
    };
    // The nearest pixel: the softened copies are smooth enough that
    // blending neighbours changes nothing visible, at four times the reads.
    let index = |x: f32, y: f32| {
        let xi = (x.floor().max(0.0) as u32).min(w - 1);
        let yi = (y.floor().max(0.0) as u32).min(h - 1);
        (yi * w + xi) as usize
    };
    let coc_at = |x: f32, y: f32| coc[index(x, y)];

    let table = sample_table(lens.shape);
    let raw: &mut [u8] = &mut *image;
    for_row_bands(raw, w as usize * 3, |first_row, band| {
        for (row_i, row) in band.chunks_mut(w as usize * 3).enumerate() {
            let y = first_row + row_i;
            for (x, px) in row.chunks_exact_mut(3).enumerate() {
                let i = y * w as usize + x;
                let r = coc[i];
                if r < 0.5 {
                    continue;
                }
                let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
                let edge = ((cx / wf) * 2.0 - 1.0, (cy / hf) * 2.0 - 1.0);
                let own = colours[i];
                let mut sum = own;
                let mut total = 1.0;
                let mut glow = [0.0f32; 3];
                let mut glow_n = 0.0f32;
                let level = level(r);
                let start = level_start(level);
                let mix = pre_blur_mix(r, max_r);
                for &[ox, oy, shape_w] in &table[start..start + LEVELS[level] as usize] {
                    if lens.shape == BokehShape::CatEye && !cat_eye_keeps(ox, oy, edge) {
                        continue;
                    }
                    let (qx, qy) = (cx + ox * r, cy + oy * r);
                    let distance = (ox * ox + oy * oy).sqrt() * r;
                    glow_n += shape_w;
                    let reach = (coc_at(qx, qy) - distance + 1.0).clamp(0.0, 1.0);
                    if reach <= 0.0 {
                        continue;
                    }
                    if !lights.is_empty() {
                        let light = lights[index(qx, qy)];
                        for k in 0..3 {
                            glow[k] += light[k] * reach * shape_w;
                        }
                    }
                    // Only the copy that counts is read.
                    let c: [f32; 3] = if mix >= 1.0 {
                        soft[index(qx, qy)]
                    } else {
                        let sharp = bilinear(&colours, qx, qy);
                        let blurred = soft[index(qx, qy)];
                        std::array::from_fn(|k| sharp[k] + (blurred[k] - sharp[k]) * mix)
                    };
                    let weight = reach * shape_w;
                    for k in 0..3 {
                        sum[k] += c[k] * weight;
                    }
                    total += weight;
                }
                let glow = glow.map(|g| g / glow_n.max(1.0));
                let strength = bloom(gain, luma(glow));
                for k in 0..3 {
                    let value = sum[k] / total + glow[k] * strength;
                    px[k] = (value * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    });
}

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_focus_stays_sharp_and_the_rest_blurs() {
        // Fine stripes everywhere: sharp, they alternate; blurred, they
        // average to grey.
        let image = RgbImage::from_fn(300, 200, |x, _| image::Rgb([if x % 4 < 2 { 30 } else { 220 }; 3]));
        let mut s = DevelopSettings::default();
        s.lens_blur.amount = 100.0;
        let mut out = image.clone();
        apply(&mut out, &s);
        let contrast = |img: &RgbImage, x0: u32, y: u32| {
            (x0..x0 + 8)
                .map(|x| img.get_pixel(x, y)[0] as i32)
                .fold((255, 0), |(lo, hi), v| (lo.min(v), hi.max(v)))
        };
        let (lo, hi) = contrast(&out, 146, 100);
        assert!(hi - lo > 150, "centre should stay sharp: {lo}..{hi}");
        let (lo, hi) = contrast(&out, 4, 6);
        assert!(hi - lo < 60, "corner should be blurred: {lo}..{hi}");
    }

    #[test]
    fn every_shape_keeps_some_samples() {
        for shape in BokehShape::ALL {
            let kept = (0..SAMPLES)
                .filter(|&i| sample_offset(i, SAMPLES, shape, (1.0, 1.0)).is_some())
                .count();
            assert!(kept > SAMPLES as usize / 4, "{shape:?}: {kept}");
        }
    }
}
