//! Text watermarks stamped on exported photos.
//!
//! The text is laid out with the system fonts (through resvg, which the UI
//! already uses for its icons), sized relative to the photo's long edge so
//! it looks the same at any export size, and blended on at the chosen
//! opacity with a faint offset shadow so it stays legible on busy photos.

use std::sync::{Arc, OnceLock};

use image::RgbImage;
use resvg::{tiny_skia, usvg};
use rustyedit_core::{Watermark, WatermarkPosition};

/// The system fonts, loaded once (it takes a moment).
fn fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone()
}

fn escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&apos;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// The watermark text rendered on its own, tightly cropped (premultiplied
/// RGBA). `None` for empty text or when no font could draw it.
fn render_text(watermark: &Watermark, font_px: f32) -> Option<tiny_skia::Pixmap> {
    let text = watermark.text.trim();
    if text.is_empty() {
        return None;
    }
    let (fill, shadow) = if watermark.white {
        ("#ffffff", "#000000")
    } else {
        ("#000000", "#ffffff")
    };
    let offset = (font_px * 0.05).max(1.0);
    let (x, y) = (font_px, font_px * 1.5);
    let canvas_w = font_px * (text.chars().count() as f32 + 4.0);
    let canvas_h = font_px * 3.0;
    let font = r#"font-family="Helvetica Neue, Helvetica, Arial, sans-serif" font-weight="500""#;
    let escaped = escape(text);
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{canvas_w}" height="{canvas_h}">
<text x="{sx}" y="{sy}" {font} font-size="{font_px}" fill="{shadow}" fill-opacity="0.35">{escaped}</text>
<text x="{x}" y="{y}" {font} font-size="{font_px}" fill="{fill}">{escaped}</text>
</svg>"#,
        sx = x + offset,
        sy = y + offset,
    );
    let options = usvg::Options {
        fontdb: fonts(),
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(&svg, &options).ok()?;
    let bounds = tree.root().abs_bounding_box();
    if bounds.width() < 1.0 || bounds.height() < 1.0 {
        return None;
    }
    let pad = (font_px * 0.1).ceil();
    let width = (bounds.width() + 2.0 * pad).ceil() as u32;
    let height = (bounds.height() + 2.0 * pad).ceil() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    let transform = tiny_skia::Transform::from_translate(pad - bounds.x(), pad - bounds.y());
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(pixmap)
}

/// Stamps `watermark` on `image`. Does nothing for empty text.
pub fn apply_watermark(image: &mut RgbImage, watermark: &Watermark) {
    let (w, h) = (image.width(), image.height());
    if w == 0 || h == 0 {
        return;
    }
    let font_px = (w.max(h) as f32 * watermark.size.clamp(1, 50) as f32 / 100.0).max(8.0);
    let Some(text) = render_text(watermark, font_px) else {
        return;
    };
    let (tw, th) = (text.width() as i64, text.height() as i64);
    let margin = (font_px * 0.8) as i64;
    let (w, h) = (w as i64, h as i64);
    let (left, top) = match watermark.position {
        WatermarkPosition::BottomRight => (w - margin - tw, h - margin - th),
        WatermarkPosition::BottomLeft => (margin, h - margin - th),
        WatermarkPosition::TopRight => (w - margin - tw, margin),
        WatermarkPosition::TopLeft => (margin, margin),
        WatermarkPosition::Centre => ((w - tw) / 2, (h - th) / 2),
    };
    let opacity = watermark.opacity.min(100) as f32 / 100.0;
    let pixels = text.data();
    for ty in 0..th {
        let y = top + ty;
        if !(0..h).contains(&y) {
            continue;
        }
        for tx in 0..tw {
            let x = left + tx;
            if !(0..w).contains(&x) {
                continue;
            }
            let i = ((ty * tw + tx) * 4) as usize;
            let alpha = pixels[i + 3] as f32 / 255.0 * opacity;
            if alpha <= 0.0 {
                continue;
            }
            let dst = image.get_pixel_mut(x as u32, y as u32);
            for c in 0..3 {
                // Premultiplied source over the photo.
                let src = pixels[i + c] as f32 * opacity;
                dst.0[c] = (src + dst.0[c] as f32 * (1.0 - alpha)).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn stamps_text_in_the_chosen_corner_only() {
        if fonts().faces().next().is_none() {
            return; // No system fonts to draw with (e.g. a bare CI box).
        }
        let grey = Rgb([100u8, 100, 100]);
        let mut image = RgbImage::from_pixel(800, 600, grey);
        let watermark = Watermark {
            text: "© Test".into(),
            position: WatermarkPosition::BottomRight,
            size: 6,
            opacity: 100,
            white: true,
        };
        apply_watermark(&mut image, &watermark);
        let changed = |x0: u32, y0: u32, x1: u32, y1: u32| {
            (y0..y1)
                .flat_map(|y| (x0..x1).map(move |x| (x, y)))
                .filter(|&(x, y)| *image.get_pixel(x, y) != grey)
                .count()
        };
        // Some bright text in the bottom-right quarter, none top-left.
        assert!(changed(400, 300, 800, 600) > 50);
        assert_eq!(changed(0, 0, 400, 300), 0);
    }

    #[test]
    fn empty_text_leaves_the_photo_alone() {
        let mut image = RgbImage::from_pixel(64, 64, Rgb([10, 20, 30]));
        let before = image.clone();
        apply_watermark(
            &mut image,
            &Watermark {
                text: "   ".into(),
                ..Default::default()
            },
        );
        assert_eq!(image, before);
    }
}
