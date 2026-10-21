//! Noise reduction. Brightness (luminance) noise is smoothed with an
//! edge-preserving bilateral filter; colour noise with a wider filter on
//! the chroma channels that is guided by the luminance, so colours don't
//! bleed across edges. Run on the photo before any tone or sharpening
//! changes, which would otherwise amplify the noise.

use image::RgbImage;

use crate::parallel::for_row_bands;

/// Smooths `image` in place. `luma` and `color` are 0..=100 strengths.
pub(crate) fn reduce_noise(image: &mut RgbImage, luma: f32, color: f32) {
    let (w, h) = (image.width() as usize, image.height() as usize);
    if w < 3 || h < 3 || (luma < 0.5 && color < 0.5) {
        return;
    }
    // Planes: luminance 0..255, chroma centred on 0.
    let n = w * h;
    let (mut y, mut cb, mut cr) = (vec![0f32; n], vec![0f32; n], vec![0f32; n]);
    for (i, px) in image.as_raw().chunks_exact(3).enumerate() {
        let (r, g, b) = (px[0] as f32, px[1] as f32, px[2] as f32);
        let l = 0.299 * r + 0.587 * g + 0.114 * b;
        y[i] = l;
        cb[i] = (b - l) * 0.564;
        cr[i] = (r - l) * 0.713;
    }

    let luma_amount = (luma / 100.0).clamp(0.0, 1.0);
    let color_amount = (color / 100.0).clamp(0.0, 1.0);
    let filtered_y = if luma_amount > 0.0 {
        bilateral(
            &y,
            &y,
            w,
            h,
            2,
            1,
            2.0 + luma_amount,
            3.0 + 40.0 * luma_amount,
        )
    } else {
        y.clone()
    };
    let (filtered_cb, filtered_cr) = if color_amount > 0.0 {
        // Spaced taps reach further without a bigger kernel.
        let sigma_r = 6.0 + 30.0 * color_amount;
        (
            bilateral(&cb, &y, w, h, 2, 3, 4.0, sigma_r),
            bilateral(&cr, &y, w, h, 2, 3, 4.0, sigma_r),
        )
    } else {
        (cb.clone(), cr.clone())
    };

    let raw: &mut [u8] = &mut *image;
    for_row_bands(raw, w * 3, |first_row, band| {
        for (row_i, row) in band.chunks_mut(w * 3).enumerate() {
            let base = (first_row + row_i) * w;
            for (x, px) in row.chunks_exact_mut(3).enumerate() {
                let i = base + x;
                let l = y[i] + (filtered_y[i] - y[i]) * luma_amount.sqrt();
                let b_c = cb[i] + (filtered_cb[i] - cb[i]) * color_amount.sqrt();
                let r_c = cr[i] + (filtered_cr[i] - cr[i]) * color_amount.sqrt();
                let r = l + r_c / 0.713;
                let b = l + b_c / 0.564;
                let g = (l - 0.299 * r - 0.114 * b) / 0.587;
                px[0] = r.round().clamp(0.0, 255.0) as u8;
                px[1] = g.round().clamp(0.0, 255.0) as u8;
                px[2] = b.round().clamp(0.0, 255.0) as u8;
            }
        }
    });
}

/// A bilateral filter of `values`, with edges taken from `guide`: each
/// output is the average of the taps within `radius` steps of `step` pixels,
/// weighted by distance and by how close the guide is at that tap.
#[allow(clippy::too_many_arguments)]
fn bilateral(
    values: &[f32],
    guide: &[f32],
    w: usize,
    h: usize,
    radius: i32,
    step: i32,
    sigma_s: f32,
    sigma_r: f32,
) -> Vec<f32> {
    // Weights by spatial offset, and by guide difference (0..=255).
    let taps = (2 * radius + 1) as usize;
    let mut spatial = vec![0f32; taps * taps];
    for j in 0..taps {
        for i in 0..taps {
            let dx = (i as i32 - radius) as f32;
            let dy = (j as i32 - radius) as f32;
            spatial[j * taps + i] = (-(dx * dx + dy * dy) / (2.0 * sigma_s * sigma_s)).exp();
        }
    }
    let range: Vec<f32> = (0..256)
        .map(|d| (-((d * d) as f32) / (2.0 * sigma_r * sigma_r)).exp())
        .collect();

    let mut out = vec![0f32; w * h];
    for_row_bands(&mut out, w, |first_row, band| {
        for (row_i, row) in band.chunks_mut(w).enumerate() {
            let y = (first_row + row_i) as i32;
            for (x, out_px) in row.iter_mut().enumerate() {
                let x = x as i32;
                let centre = guide[y as usize * w + x as usize];
                let (mut sum, mut weight) = (0.0f32, 0.0f32);
                for j in 0..taps as i32 {
                    let sy = (y + (j - radius) * step).clamp(0, h as i32 - 1) as usize;
                    for i in 0..taps as i32 {
                        let sx = (x + (i - radius) * step).clamp(0, w as i32 - 1) as usize;
                        let idx = sy * w + sx;
                        let d = (guide[idx] - centre).abs().min(255.0) as usize;
                        let wt = spatial[j as usize * taps + i as usize] * range[d];
                        sum += values[idx] * wt;
                        weight += wt;
                    }
                }
                *out_px = sum / weight.max(1e-6);
            }
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn speckle(x: u32, y: u32) -> i32 {
        ((x * 7919 + y * 104_729) % 41) as i32 - 20
    }

    /// A flat grey with brightness noise on top.
    fn noisy(width: u32, height: u32) -> RgbImage {
        RgbImage::from_fn(width, height, |x, y| {
            let v = (128 + speckle(x, y)).clamp(0, 255) as u8;
            Rgb([v, v, v])
        })
    }

    /// A flat grey whose colour (not brightness) is noisy.
    fn colour_noisy(width: u32, height: u32) -> RgbImage {
        RgbImage::from_fn(width, height, |x, y| {
            let n = speckle(x, y) / 2;
            Rgb([(128 + n) as u8, 128, (128 - n) as u8])
        })
    }

    fn spread(img: &RgbImage) -> f32 {
        let mean =
            img.pixels().map(|p| p[0] as f32).sum::<f32>() / (img.width() * img.height()) as f32;
        img.pixels()
            .map(|p| (p[0] as f32 - mean).powi(2))
            .sum::<f32>()
            / (img.width() * img.height()) as f32
    }

    fn colour_spread(img: &RgbImage) -> f32 {
        img.pixels()
            .map(|p| (p[0] as f32 - p[2] as f32).abs())
            .sum::<f32>()
            / (img.width() * img.height()) as f32
    }

    #[test]
    fn luminance_noise_reduction_smooths_flat_noise() {
        let mut img = noisy(48, 48);
        let before = spread(&img);
        reduce_noise(&mut img, 80.0, 0.0);
        assert!(spread(&img) < before * 0.5);
    }

    #[test]
    fn colour_noise_reduction_evens_out_the_colour() {
        let mut img = colour_noisy(48, 48);
        let before = colour_spread(&img);
        reduce_noise(&mut img, 0.0, 80.0);
        assert!(colour_spread(&img) < before * 0.5);
    }

    #[test]
    fn edges_survive_noise_reduction() {
        let mut img = RgbImage::from_fn(40, 40, |x, _| {
            if x < 20 {
                Rgb([20, 20, 20])
            } else {
                Rgb([230, 230, 230])
            }
        });
        reduce_noise(&mut img, 100.0, 100.0);
        assert!(img.get_pixel(5, 20)[0] < 40);
        assert!(img.get_pixel(35, 20)[0] > 210);
    }

    #[test]
    fn zero_strength_changes_nothing() {
        let mut img = noisy(16, 16);
        let copy = img.clone();
        reduce_noise(&mut img, 0.0, 0.0);
        assert_eq!(img, copy);
    }
}
