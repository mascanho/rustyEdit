//! Straighten, correct perspective and crop: turns the picture about its centre and takes the
//! crop rectangle out of the turned canvas, sampling bilinearly so
//! straightened edges stay smooth. Points off the picture (the corners a
//! turn leaves empty, only visible while editing the crop) are black.

use image::{imageops, RgbImage};
use rustyedit_core::Crop;

pub(crate) fn straighten_and_crop(image: RgbImage, crop: &Crop) -> RgbImage {
    if crop.is_identity() {
        return image;
    }
    let (width, height) = (image.width() as f32, image.height() as f32);
    let (x, y, w, h) = crop.rect;
    let (x0, y0) = (x * width, y * height);
    let out_w = ((w * width).round() as u32).max(1);
    let out_h = ((h * height).round() as u32).max(1);

    if crop.angle == 0.0 && !crop.has_perspective() {
        let left = (x0.round() as u32).min(image.width() - 1);
        let top = (y0.round() as u32).min(image.height() - 1);
        let out_w = out_w.min(image.width() - left);
        let out_h = out_h.min(image.height() - top);
        return imageops::crop_imm(&image, left, top, out_w, out_h).to_image();
    }

    let mut out = RgbImage::new(out_w, out_h);
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(out_h as usize)
        .max(1);
    let rows_per_band = (out_h as usize).div_ceil(threads);
    let row_bytes = out_w as usize * 3;
    let source = &image;

    std::thread::scope(|scope| {
        for (band, chunk) in out.chunks_mut(rows_per_band * row_bytes).enumerate() {
            scope.spawn(move || {
                for (i, row) in chunk.chunks_mut(row_bytes).enumerate() {
                    let oy = (band * rows_per_band + i) as f32;
                    for (ox, px) in row.chunks_exact_mut(3).enumerate() {
                        let canvas = (x0 + ox as f32 + 0.5, y0 + oy + 0.5);
                        let (sx, sy) = crop.source_point(canvas, width, height);
                        px.copy_from_slice(&bilinear(source, sx - 0.5, sy - 0.5));
                    }
                }
            });
        }
    });
    out
}

/// The colour at (`x`, `y`) between pixel centres; black off the image.
fn bilinear(image: &RgbImage, x: f32, y: f32) -> [u8; 3] {
    let (w, h) = (image.width() as i64, image.height() as i64);
    if x < -1.0 || y < -1.0 || x > w as f32 || y > h as f32 {
        return [0, 0, 0];
    }
    let (fx, fy) = (x.floor(), y.floor());
    let (tx, ty) = (x - fx, y - fy);
    let pixel = |px: i64, py: i64| {
        let p = image
            .get_pixel(px.clamp(0, w - 1) as u32, py.clamp(0, h - 1) as u32)
            .0;
        [p[0] as f32, p[1] as f32, p[2] as f32]
    };
    let (ix, iy) = (fx as i64, fy as i64);
    let (a, b, c, d) = (
        pixel(ix, iy),
        pixel(ix + 1, iy),
        pixel(ix, iy + 1),
        pixel(ix + 1, iy + 1),
    );
    std::array::from_fn(|ch| {
        let top = a[ch] + (b[ch] - a[ch]) * tx;
        let bottom = c[ch] + (d[ch] - c[ch]) * tx;
        (top + (bottom - top) * ty).round().clamp(0.0, 255.0) as u8
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyedit_core::AspectLock;

    #[test]
    fn plain_crop_takes_the_rectangle() {
        let image = RgbImage::from_fn(100, 50, |x, _| image::Rgb([x as u8, 0, 0]));
        let crop = Crop {
            rect: (0.5, 0.0, 0.5, 1.0),
            angle: 0.0,
            aspect: AspectLock::Free,
            ..Default::default()
        };
        let out = straighten_and_crop(image, &crop);
        assert_eq!(out.dimensions(), (50, 50));
        assert_eq!(out.get_pixel(0, 0)[0], 50);
    }

    #[test]
    fn straightening_keeps_the_centre_in_place() {
        let image = RgbImage::from_fn(101, 101, |x, y| {
            image::Rgb([if x == 50 && y == 50 { 255 } else { 0 }; 3])
        });
        let crop = Crop {
            angle: 30.0,
            ..Default::default()
        };
        let out = straighten_and_crop(image, &crop);
        assert!(out.get_pixel(50, 50)[0] > 200);
    }
}
