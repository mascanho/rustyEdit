//! "Show mask": paints a mask layer's coverage over the rendered preview
//! in red, as Lightroom's mask overlay does, so you can see what a brush
//! or range mask has caught.

use image::RgbaImage;
use rustyedit_core::DevelopSettings;

/// The overlay colour.
const RED: [f32; 3] = [255.0, 40.0, 40.0];
/// How strongly a fully covered pixel is tinted.
const STRENGTH: f32 = 0.6;

/// Tints `bgra` (a preview of the photo as `settings` shows it) wherever
/// layer `layer` of its gradients covers. `oriented` is the size of the
/// photo with its rotation applied, before any crop. Colour ranges judge
/// the pixel as shown, a little later in the pipeline than the real
/// mask does, which is close enough to see what they catch.
pub fn overlay_mask(
    bgra: &mut RgbaImage,
    settings: &DevelopSettings,
    layer: usize,
    oriented: (f32, f32),
) {
    let Some(gradient) = settings.gradients.as_slice().get(layer) else {
        return;
    };
    let gradient = gradient.on_display(settings);
    let crop = settings.crop;
    let (w, h) = oriented;
    let (out_w, out_h) = (bgra.width() as usize, bgra.height() as usize);
    if out_w == 0 || out_h == 0 || w < 1.0 || h < 1.0 {
        return;
    }
    let (rx, ry, rw, rh) = crop.rect;
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, out_h);
    let rows_per_band = out_h.div_ceil(threads);
    let row_bytes = out_w * 4;
    let (gradient, crop) = (&gradient, &crop);
    std::thread::scope(|scope| {
        for (band, chunk) in bgra.chunks_mut(rows_per_band * row_bytes).enumerate() {
            scope.spawn(move || {
                for (i, row) in chunk.chunks_mut(row_bytes).enumerate() {
                    let oy = band * rows_per_band + i;
                    for (ox, px) in row.chunks_exact_mut(4).enumerate() {
                        // Where this pixel is on the oriented photo.
                        let u = rx + (ox as f32 + 0.5) / out_w as f32 * rw;
                        let v = ry + (oy as f32 + 0.5) / out_h as f32 * rh;
                        let (sx, sy) = crop.source_point((u * w, v * h), w, h);
                        if sx < 0.0 || sy < 0.0 || sx > w || sy > h {
                            continue;
                        }
                        let mut mask = gradient.mask_at(sx, sy, w, h);
                        if mask > 0.0 && gradient.has_range() {
                            let rgb = [px[2] as f32 / 255.0, px[1] as f32 / 255.0, px[0] as f32 / 255.0];
                            mask *= gradient.range_at(rgb);
                        }
                        let a = (mask * STRENGTH).clamp(0.0, 1.0);
                        if a > 0.0 {
                            // BGRA.
                            for (channel, target) in [(0, RED[2]), (1, RED[1]), (2, RED[0])] {
                                let c = px[channel] as f32;
                                px[channel] = (c + (target - c) * a).round() as u8;
                            }
                        }
                    }
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    use rustyedit_core::{Dab, Gradient, GradientKind};

    fn settings_with(gradient: Gradient) -> DevelopSettings {
        let mut s = DevelopSettings::default();
        s.gradients.push(gradient);
        s
    }

    #[test]
    fn a_brush_mask_shows_red_where_it_was_painted() {
        let mut brush = Gradient { kind: GradientKind::Brush, feather: 0.0, ..Default::default() };
        brush.add_dab(Dab { x: 0.25, y: 0.5, radius: 0.1, erase: false });
        let mut img = RgbaImage::from_pixel(100, 100, Rgba([128, 128, 128, 255]));
        overlay_mask(&mut img, &settings_with(brush), 0, (100.0, 100.0));
        let inside = img.get_pixel(25, 50).0;
        assert!(inside[2] > 160 && inside[0] < 128, "{inside:?}");
        assert_eq!(img.get_pixel(80, 50).0, [128, 128, 128, 255]);
    }

    #[test]
    fn a_range_mask_catches_only_the_chosen_tones() {
        let range = Gradient {
            kind: GradientKind::Range,
            luma_range: (0.0, 0.4),
            ..Default::default()
        };
        let mut img = RgbaImage::from_fn(10, 10, |x, _| {
            if x < 5 { Rgba([40, 40, 40, 255]) } else { Rgba([220, 220, 220, 255]) }
        });
        overlay_mask(&mut img, &settings_with(range), 0, (10.0, 10.0));
        assert!(img.get_pixel(1, 1).0[2] > 100, "shadows are caught");
        assert_eq!(img.get_pixel(8, 1).0, [220, 220, 220, 255]);
    }

    #[test]
    fn a_crop_moves_the_mask_with_the_photo() {
        // A dab on the left of the photo, cropped to the right half: gone.
        let mut brush = Gradient { kind: GradientKind::Brush, feather: 0.0, ..Default::default() };
        brush.add_dab(Dab { x: 0.2, y: 0.5, radius: 0.1, erase: false });
        let mut settings = settings_with(brush);
        settings.crop.rect = (0.5, 0.0, 0.5, 1.0);
        let mut img = RgbaImage::from_pixel(50, 100, Rgba([128, 128, 128, 255]));
        overlay_mask(&mut img, &settings, 0, (100.0, 100.0));
        assert!(img.pixels().all(|p| p.0 == [128, 128, 128, 255]));
        // Cropped to the left half instead: the dab fills a good part of it.
        settings.crop.rect = (0.0, 0.0, 0.5, 1.0);
        let mut img = RgbaImage::from_pixel(50, 100, Rgba([128, 128, 128, 255]));
        overlay_mask(&mut img, &settings, 0, (100.0, 100.0));
        assert!(img.get_pixel(20, 50).0[2] > 160);
    }

    #[test]
    fn a_missing_layer_leaves_the_picture_alone() {
        let mut img = RgbaImage::from_pixel(8, 8, Rgba([1, 2, 3, 255]));
        overlay_mask(&mut img, &DevelopSettings::default(), 3, (8.0, 8.0));
        assert!(img.pixels().all(|p| p.0 == [1, 2, 3, 255]));
    }
}
