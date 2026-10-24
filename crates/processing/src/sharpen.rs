//! Sharpening with a radius and an edge mask. The mask holds sharpening
//! back from flat areas (where it would only emphasise noise), so only
//! edges and texture are sharpened.

use image::{imageops, RgbImage};

use crate::parallel::for_row_bands;

/// The blur sharpening works against, from the Radius slider: 0.5 px at the
/// left end, about 1.1 at the default, 2.5 at the right.
pub(crate) fn sharpen_sigma(radius: f32) -> f32 {
    0.5 * 5.0f32.powf(radius.clamp(0.0, 100.0) / 100.0)
}

/// Sharpens `image` in place. `amount` is 0..=100, `radius` 0..=100 (50 is
/// about one pixel) and `masking` 0..=100 (0 sharpens everything).
pub(crate) fn sharpen(image: &mut RgbImage, amount: f32, radius: f32, masking: f32) {
    let (w, h) = (image.width() as usize, image.height() as usize);
    if w < 3 || h < 3 || amount < 0.5 {
        return;
    }
    let strength = amount / 100.0 * 1.6;
    let sigma = sharpen_sigma(radius);
    let blurred = imageops::blur(image, sigma);

    // Edge strength from the luminance gradient, 0..=1.
    let mask: Option<Vec<f32>> = (masking >= 0.5).then(|| {
        let luma: Vec<f32> = image
            .as_raw()
            .chunks_exact(3)
            .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
            .collect();
        let at = |x: i32, y: i32| {
            luma[y.clamp(0, h as i32 - 1) as usize * w + x.clamp(0, w as i32 - 1) as usize]
        };
        // Where the mask starts to open and where it is fully open.
        let threshold = masking / 100.0 * 40.0;
        let mut mask = vec![0f32; w * h];
        for_row_bands(&mut mask, w, |first_row, band| {
            for (row_i, row) in band.chunks_mut(w).enumerate() {
                let y = (first_row + row_i) as i32;
                for (x, m) in row.iter_mut().enumerate() {
                    let x = x as i32;
                    let gx = at(x + 1, y) - at(x - 1, y);
                    let gy = at(x, y + 1) - at(x, y - 1);
                    let magnitude = (gx * gx + gy * gy).sqrt();
                    let t =
                        ((magnitude - threshold * 0.5) / (threshold * 1.5 + 4.0)).clamp(0.0, 1.0);
                    *m = t * t * (3.0 - 2.0 * t);
                }
            }
        });
        mask
    });

    let blurred = blurred.as_raw();
    let raw: &mut [u8] = &mut *image;
    for_row_bands(raw, w * 3, |first_row, band| {
        for (row_i, row) in band.chunks_mut(w * 3).enumerate() {
            let y = first_row + row_i;
            for (x, px) in row.chunks_exact_mut(3).enumerate() {
                let i = y * w + x;
                let m = mask.as_ref().map_or(1.0, |m| m[i]) * strength;
                if m <= 0.0 {
                    continue;
                }
                for c in 0..3 {
                    let v = px[c] as f32;
                    let detail = v - blurred[i * 3 + c] as f32;
                    px[c] = (v + detail * m).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// A soft vertical edge in a field of faint noise.
    fn scene() -> RgbImage {
        RgbImage::from_fn(60, 40, |x, y| {
            let base = if x < 30 { 80 } else { 170 };
            let noise = ((x * 31 + y * 17) % 5) as i32 - 2;
            let v = (base + noise) as u8;
            Rgb([v, v, v])
        })
    }

    fn noise_energy(img: &RgbImage) -> f32 {
        // Differences between neighbours in the flat left half.
        (1..28)
            .flat_map(|x| (0..40).map(move |y| (x, y)))
            .map(|(x, y)| (img.get_pixel(x, y)[0] as f32 - img.get_pixel(x - 1, y)[0] as f32).abs())
            .sum()
    }

    #[test]
    fn masking_leaves_flat_noise_alone_but_sharpens_edges() {
        let original = scene();
        let mut unmasked = original.clone();
        sharpen(&mut unmasked, 100.0, 50.0, 0.0);
        let mut masked = original.clone();
        sharpen(&mut masked, 100.0, 50.0, 80.0);

        // Sharpening everything amplifies the speckle; masking mostly doesn't.
        assert!(noise_energy(&unmasked) > noise_energy(&masked) * 1.3);
        // The edge itself still gets crisper than the original.
        let step =
            |img: &RgbImage| img.get_pixel(30, 20)[0] as i32 - img.get_pixel(29, 20)[0] as i32;
        assert!(step(&masked) > step(&original));
    }

    #[test]
    fn zero_amount_is_a_no_op() {
        let mut img = scene();
        let copy = img.clone();
        sharpen(&mut img, 0.0, 50.0, 0.0);
        assert_eq!(img, copy);
    }
}
