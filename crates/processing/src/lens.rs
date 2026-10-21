//! Lens corrections, on the photo before it is rotated or cropped:
//! distortion (radial remap), lateral chromatic aberration (red and blue
//! scaled against green) and corner brightening (vignetting).

use image::RgbImage;
use rustyedit_core::DevelopSettings;

use crate::parallel::for_row_bands;

/// Whether any lens correction is on.
pub(crate) fn is_active(s: &DevelopSettings) -> bool {
    s.lens_distortion != 0.0 || s.lens_vignetting != 0.0 || s.lens_ca != 0.0
}

pub(crate) fn correct(source: &RgbImage, s: &DevelopSettings) -> RgbImage {
    let (w, h) = (source.width() as usize, source.height() as usize);
    if w < 2 || h < 2 {
        return source.clone();
    }
    let k = s.lens_distortion / 100.0 * 0.25;
    let ca = s.lens_ca / 100.0 * 0.01;
    let vignette = s.lens_vignetting / 100.0 * 0.9;
    let (cx, cy) = ((w as f32 - 1.0) / 2.0, (h as f32 - 1.0) / 2.0);
    // Distances are in units of the half-diagonal, so 1.0 is a corner.
    let norm = (cx * cx + cy * cy).sqrt().max(1.0);
    let src = source.as_raw();

    let sample = |channel: usize, fx: f32, fy: f32| -> f32 {
        let fx = fx.clamp(0.0, w as f32 - 1.0);
        let fy = fy.clamp(0.0, h as f32 - 1.0);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let at = |x: usize, y: usize| src[(y * w + x) * 3 + channel] as f32;
        let top = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
        let bottom = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
        top * (1.0 - ty) + bottom * ty
    };

    let mut out = RgbImage::new(w as u32, h as u32);
    let raw: &mut [u8] = &mut *out;
    for_row_bands(raw, w * 3, |first_row, band| {
        for (row_i, row) in band.chunks_mut(w * 3).enumerate() {
            let y = (first_row + row_i) as f32;
            for (x, px) in row.chunks_exact_mut(3).enumerate() {
                let x = x as f32;
                let (dx, dy) = (x - cx, y - cy);
                let r2 = (dx * dx + dy * dy) / (norm * norm);
                // Positive distortion pulls samples inward at the edges,
                // which straightens barrel distortion.
                let scale = 1.0 - k * r2;
                let gain = (1.0 + vignette * r2).max(0.0);
                for (c, factor) in [scale * (1.0 + ca), scale, scale * (1.0 - ca)]
                    .into_iter()
                    .enumerate()
                {
                    let v = sample(c, cx + dx * factor, cy + dy * factor) * gain;
                    px[c] = v.round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn with(f: impl Fn(&mut DevelopSettings)) -> DevelopSettings {
        let mut s = DevelopSettings::default();
        f(&mut s);
        s
    }

    #[test]
    fn neutral_settings_are_inactive() {
        assert!(!is_active(&DevelopSettings::default()));
        assert!(is_active(&with(|s| s.lens_ca = 5.0)));
    }

    #[test]
    fn vignette_correction_brightens_corners_not_the_centre() {
        let img = RgbImage::from_pixel(41, 41, Rgb([100, 100, 100]));
        let out = correct(&img, &with(|s| s.lens_vignetting = 100.0));
        assert_eq!(out.get_pixel(20, 20)[0], 100);
        assert!(out.get_pixel(0, 0)[0] > 150);
    }

    #[test]
    fn distortion_correction_moves_content_outwards_at_the_edges() {
        // A bright dot near the right edge moves further out when barrel
        // distortion is straightened (the edge is stretched).
        let mut img = RgbImage::from_pixel(101, 101, Rgb([0, 0, 0]));
        for dy in 0..3 {
            for dx in 0..3 {
                img.put_pixel(88 + dx, 49 + dy, Rgb([255, 255, 255]));
            }
        }
        let out = correct(&img, &with(|s| s.lens_distortion = 100.0));
        let brightest_x = (0..101).max_by_key(|&x| out.get_pixel(x, 50)[0]).unwrap();
        assert!(brightest_x > 89, "dot moved from 89 to {brightest_x}");
        // The centre doesn't move.
        let mut centre = RgbImage::from_pixel(101, 101, Rgb([0, 0, 0]));
        centre.put_pixel(50, 50, Rgb([255, 255, 255]));
        let moved = correct(&centre, &with(|s| s.lens_distortion = 100.0));
        assert_eq!(moved.get_pixel(50, 50)[0], 255);
    }

    #[test]
    fn chromatic_aberration_shifts_red_and_blue_apart_at_the_edges() {
        let mut img = RgbImage::from_pixel(401, 21, Rgb([0, 0, 0]));
        for y in 0..21 {
            img.put_pixel(390, y, Rgb([255, 255, 255]));
        }
        let out = correct(&img, &with(|s| s.lens_ca = 100.0));
        let peak = |c: usize| (0..401).max_by_key(|&x| out.get_pixel(x, 10)[c]).unwrap();
        assert!(peak(0) < peak(2), "red and blue should separate");
        assert_eq!(peak(1), 390);
    }
}
