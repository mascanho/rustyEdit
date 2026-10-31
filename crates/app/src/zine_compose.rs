//! Draws a framed photo the way the PDF will: shadow, mat, rounded corners
//! and tilt, into an in-memory image. The Zines workbench shows these, so
//! what is on screen matches what is printed (the UI toolkit can't turn or
//! round a picture on its own).
//!
//! Everything is measured in millimetres on the page and turned into
//! pixels by `px_per_mm`.

use image::{Rgba, RgbaImage, RgbImage};
use rustyedit_core::ZineRect;

/// What to draw: where the parts of the frame are (from
/// `rustyedit_core::zine_photo_geometry`) and how it looks.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ComposeSpec {
    pub outer: ZineRect,
    pub inner: ZineRect,
    pub image: ZineRect,
    pub outer_radius: f32,
    pub inner_radius: f32,
    /// The mat's colour, when there is a mat.
    pub mat: Option<[u8; 3]>,
    pub shadow: bool,
    /// Degrees clockwise, about `centre`.
    pub rotation: f32,
    pub centre: (f32, f32),
    pub px_per_mm: f32,
}

/// The finished picture and where its top-left corner is on the page.
pub(crate) struct Composed {
    /// Premultiplied RGBA.
    pub image: RgbaImage,
    pub origin_mm: (f32, f32),
}

/// How far a shadow reaches past the frame, in millimetres.
const SHADOW_REACH: f32 = 5.0;
const SHADOW_LAYERS: usize = 5;

/// 1 inside a rounded rectangle, 0 outside, soft across a pixel.
fn coverage(x: f32, y: f32, r: ZineRect, radius: f32, px_per_mm: f32) -> f32 {
    let radius = radius.clamp(0.0, r.w.min(r.h) / 2.0);
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    let (hx, hy) = (r.w / 2.0 - radius, r.h / 2.0 - radius);
    let (dx, dy) = ((x - cx).abs() - hx, (y - cy).abs() - hy);
    let outside = dx.max(0.0).hypot(dy.max(0.0));
    let inside = dx.max(dy).min(0.0);
    let distance = outside + inside - radius;
    (0.5 - distance * px_per_mm).clamp(0.0, 1.0)
}

/// `src` (colour and alpha, not premultiplied) laid over `dst`
/// (premultiplied).
fn over(dst: &mut [f32; 4], src: [f32; 3], alpha: f32) {
    if alpha <= 0.0 {
        return;
    }
    let keep = 1.0 - alpha;
    for c in 0..3 {
        dst[c] = src[c] * alpha + dst[c] * keep;
    }
    dst[3] = alpha + dst[3] * keep;
}

fn sample(photo: &RgbImage, u: f32, v: f32) -> [f32; 3] {
    let (w, h) = (photo.width() as f32, photo.height() as f32);
    let x = (u * w - 0.5).clamp(0.0, w - 1.0);
    let y = (v * h - 0.5).clamp(0.0, h - 1.0);
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(photo.width() - 1), (y0 + 1).min(photo.height() - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let at = |x: u32, y: u32| photo.get_pixel(x, y).0.map(f32::from);
    let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
    let mut out = [0.0; 3];
    for i in 0..3 {
        let top = a[i] + (b[i] - a[i]) * fx;
        let bottom = c[i] + (d[i] - c[i]) * fx;
        out[i] = (top + (bottom - top) * fy) / 255.0;
    }
    out
}

/// The frame, unturned, as premultiplied floats.
fn layer(photo: &RgbImage, spec: &ComposeSpec) -> (Vec<[f32; 4]>, u32, u32, (f32, f32)) {
    let s = spec.px_per_mm;
    let pad = if spec.shadow { SHADOW_REACH } else { 0.0 };
    let bounds = spec.outer.grown(pad);
    let (w, h) = (
        (bounds.w * s).ceil().max(1.0) as u32,
        (bounds.h * s).ceil().max(1.0) as u32,
    );
    let mut pixels = vec![[0.0f32; 4]; (w * h) as usize];
    for py in 0..h {
        for px in 0..w {
            let (x, y) = (
                bounds.x + (px as f32 + 0.5) / s,
                bounds.y + (py as f32 + 0.5) / s,
            );
            let mut p = [0.0f32; 4];
            if spec.shadow {
                for k in 1..=SHADOW_LAYERS {
                    let grow = k as f32 * 0.55;
                    let r = spec.outer.grown(grow);
                    let shifted = ZineRect { x: r.x + 0.6, y: r.y + 1.2, ..r };
                    let cov = coverage(x, y, shifted, spec.outer_radius + grow, s);
                    over(&mut p, [0.0; 3], 0.055 * cov);
                }
            }
            if let Some(mat) = spec.mat {
                let cov = coverage(x, y, spec.outer, spec.outer_radius, s);
                over(&mut p, mat.map(|c| c as f32 / 255.0), cov);
            }
            let cov = coverage(x, y, spec.inner, spec.inner_radius, s);
            if cov > 0.0 {
                let u = (x - spec.image.x) / spec.image.w.max(1e-3);
                let v = (y - spec.image.y) / spec.image.h.max(1e-3);
                over(&mut p, sample(photo, u, v), cov);
            }
            pixels[(py * w + px) as usize] = p;
        }
    }
    (pixels, w, h, (bounds.x, bounds.y))
}

/// Draws `photo` as `spec` says.
pub(crate) fn compose(photo: &RgbImage, spec: &ComposeSpec) -> Composed {
    let (pixels, w, h, origin) = layer(photo, spec);
    let to_u8 = |p: [f32; 4]| Rgba(p.map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8));
    if spec.rotation.abs() < 0.01 {
        let image = RgbaImage::from_fn(w, h, |x, y| to_u8(pixels[(y * w + x) as usize]));
        return Composed { image, origin_mm: origin };
    }

    // Turned about the centre: the canvas is the box round the turned layer.
    let s = spec.px_per_mm;
    let (sin, cos) = spec.rotation.to_radians().sin_cos();
    let (cx, cy) = ((spec.centre.0 - origin.0) * s, (spec.centre.1 - origin.1) * s);
    let corners = [(0.0, 0.0), (w as f32, 0.0), (0.0, h as f32), (w as f32, h as f32)].map(|(x, y)| {
        let (dx, dy) = (x - cx, y - cy);
        (dx * cos - dy * sin, dx * sin + dy * cos)
    });
    let min_x = corners.iter().map(|c| c.0).fold(f32::MAX, f32::min);
    let max_x = corners.iter().map(|c| c.0).fold(f32::MIN, f32::max);
    let min_y = corners.iter().map(|c| c.1).fold(f32::MAX, f32::min);
    let max_y = corners.iter().map(|c| c.1).fold(f32::MIN, f32::max);
    let (cw, ch) = (
        (max_x - min_x).ceil().max(1.0) as u32,
        (max_y - min_y).ceil().max(1.0) as u32,
    );
    let at = |x: i64, y: i64| -> [f32; 4] {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
            [0.0; 4]
        } else {
            pixels[(y as u32 * w + x as u32) as usize]
        }
    };
    let image = RgbaImage::from_fn(cw, ch, |px, py| {
        // The destination pixel, relative to the centre, turned back.
        let (dx, dy) = (px as f32 + 0.5 + min_x, py as f32 + 0.5 + min_y);
        let sx = dx * cos + dy * sin + cx - 0.5;
        let sy = -dx * sin + dy * cos + cy - 0.5;
        let (x0, y0) = (sx.floor(), sy.floor());
        let (fx, fy) = (sx - x0, sy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let (a, b, c, d) = (at(x0, y0), at(x0 + 1, y0), at(x0, y0 + 1), at(x0 + 1, y0 + 1));
        let mut p = [0.0; 4];
        for i in 0..4 {
            let top = a[i] + (b[i] - a[i]) * fx;
            let bottom = c[i] + (d[i] - c[i]) * fx;
            p[i] = top + (bottom - top) * fy;
        }
        to_u8(p)
    });
    Composed {
        image,
        origin_mm: (spec.centre.0 + min_x / s, spec.centre.1 + min_y / s),
    }
}

/// The picture as gpui wants it: premultiplied, blue first.
pub(crate) fn to_bgra(mut image: RgbaImage) -> RgbaImage {
    for p in image.pixels_mut() {
        p.0.swap(0, 2);
    }
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo() -> RgbImage {
        RgbImage::from_pixel(40, 30, image::Rgb([200, 40, 40]))
    }

    fn spec() -> ComposeSpec {
        let outer = ZineRect { x: 10.0, y: 10.0, w: 40.0, h: 30.0 };
        ComposeSpec {
            outer,
            inner: outer,
            image: outer,
            outer_radius: 0.0,
            inner_radius: 0.0,
            mat: None,
            shadow: false,
            rotation: 0.0,
            centre: (30.0, 25.0),
            px_per_mm: 4.0,
        }
    }

    #[test]
    fn a_plain_photo_fills_its_frame() {
        let c = compose(&photo(), &spec());
        assert_eq!(c.origin_mm, (10.0, 10.0));
        assert_eq!((c.image.width(), c.image.height()), (160, 120));
        assert_eq!(c.image.get_pixel(80, 60).0, [200, 40, 40, 255]);
    }

    #[test]
    fn rounded_corners_are_see_through_and_a_mat_surrounds_the_photo() {
        let mut s = spec();
        s.outer_radius = 8.0;
        s.inner_radius = 8.0;
        let c = compose(&photo(), &s);
        assert_eq!(c.image.get_pixel(0, 0).0[3], 0, "the corner is cut away");
        assert_eq!(c.image.get_pixel(80, 60).0[3], 255);
        // With a white mat 3 mm wide.
        s.outer_radius = 0.0;
        s.inner_radius = 0.0;
        s.mat = Some([255, 255, 255]);
        s.inner = s.outer.shrunk(3.0);
        s.image = s.inner;
        let c = compose(&photo(), &s);
        assert_eq!(c.image.get_pixel(4, 60).0, [255, 255, 255, 255], "mat");
        assert_eq!(c.image.get_pixel(80, 60).0, [200, 40, 40, 255], "photo");
    }

    #[test]
    fn a_shadow_makes_the_picture_larger_and_leaves_the_centre_alone() {
        let mut s = spec();
        s.shadow = true;
        let c = compose(&photo(), &s);
        assert!(c.image.width() > 160 && c.origin_mm.0 < 10.0);
        let (cx, cy) = (c.image.width() / 2, c.image.height() / 2);
        assert_eq!(c.image.get_pixel(cx, cy).0, [200, 40, 40, 255]);
        // Faintly dark a millimetre below the frame (4 px a millimetre).
        let frame_bottom = ((10.0 + 30.0 - c.origin_mm.1) * 4.0) as u32;
        let below = c.image.get_pixel(cx, frame_bottom + 4).0;
        assert!(below[3] > 0 && below[3] < 120, "{below:?}");
    }

    #[test]
    fn turning_grows_the_canvas_about_the_centre() {
        let mut s = spec();
        // A quarter turn swaps width and height.
        s.rotation = 90.0;
        let c = compose(&photo(), &s);
        assert!((c.image.width() as i32 - 120).abs() <= 2 && (c.image.height() as i32 - 160).abs() <= 2);
        let (cx, cy) = (c.image.width() / 2, c.image.height() / 2);
        assert_eq!(c.image.get_pixel(cx, cy).0, [200, 40, 40, 255]);
        // Its middle is still the frame's centre.
        let middle_x = c.origin_mm.0 + c.image.width() as f32 / 2.0 / s.px_per_mm;
        assert!((middle_x - 30.0).abs() < 0.6);

        // Tilted, the corners of the canvas are empty.
        s.rotation = 30.0;
        let c = compose(&photo(), &s);
        assert!(c.image.width() > 160 && c.image.height() > 120);
        assert_eq!(c.image.get_pixel(0, 0).0[3], 0);
        let (cx, cy) = (c.image.width() / 2, c.image.height() / 2);
        assert_eq!(c.image.get_pixel(cx, cy).0, [200, 40, 40, 255]);
        let bgra = to_bgra(c.image);
        assert_eq!(bgra.get_pixel(cx, cy).0, [40, 40, 200, 255]);
    }
}
