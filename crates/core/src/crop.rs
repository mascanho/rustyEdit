//! Crop and straighten.
//!
//! Both work on the photo after its 90° rotation and flips (the *oriented*
//! photo). Straightening turns the picture by up to ±45° about its centre
//! on a canvas of the same size; the crop rectangle is then taken from
//! that canvas, as fractions of its width and height.
//!
//! Perspective correction (vertical and horizontal keystone) is applied to
//! the picture before it is turned: see [`Crop::source_point`].
//!
//! [`CropView`] maps points between the oriented photo and the cropped,
//! straightened result as shown, so tools drawn on the photo (spots,
//! gradients) stay on the right pixels.

use serde::{Deserialize, Serialize};

/// Largest straightening angle, either way.
pub const MAX_STRAIGHTEN: f32 = 45.0;
/// Largest perspective correction, either way, on each axis.
pub const MAX_PERSPECTIVE: f32 = 100.0;
/// How much of the picture's half size a full-strength correction pulls
/// the far edge in by.
const PERSPECTIVE_STRENGTH: f32 = 0.35;
/// Smallest crop side, as a fraction of the photo.
const MIN_SIDE: f32 = 0.02;

/// A crop's shape constraint.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum AspectLock {
    /// Any shape.
    #[default]
    Free,
    /// The photo's own proportions.
    Original,
    /// Width ÷ height, e.g. 1.5 for 3:2.
    Ratio(f32),
}

impl AspectLock {
    /// The presets offered, with their labels.
    pub const PRESETS: [(&'static str, AspectLock); 9] = [
        ("Free", AspectLock::Free),
        ("Original", AspectLock::Original),
        ("1:1", AspectLock::Ratio(1.0)),
        ("4:5", AspectLock::Ratio(0.8)),
        ("5:4", AspectLock::Ratio(1.25)),
        ("7:5", AspectLock::Ratio(1.4)),
        ("3:2", AspectLock::Ratio(1.5)),
        ("4:3", AspectLock::Ratio(4.0 / 3.0)),
        ("16:9", AspectLock::Ratio(16.0 / 9.0)),
    ];

    /// The width ÷ height the crop must keep on a `width` × `height`
    /// photo, if any.
    pub fn ratio(&self, width: f32, height: f32) -> Option<f32> {
        match self {
            AspectLock::Free => None,
            AspectLock::Original => Some(width / height.max(1.0)),
            AspectLock::Ratio(r) => Some(*r),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Crop {
    /// `(x, y, width, height)` as fractions of the straightened canvas.
    pub rect: (f32, f32, f32, f32),
    /// Straightening, in degrees; positive turns the picture clockwise.
    pub angle: f32,
    pub aspect: AspectLock,
    /// -100..=100: corrects converging verticals (a building seen from
    /// below); positive narrows the top.
    pub vertical: f32,
    /// -100..=100: corrects converging horizontals; positive narrows the
    /// right side.
    pub horizontal: f32,
}

impl Default for Crop {
    fn default() -> Self {
        Self {
            rect: (0.0, 0.0, 1.0, 1.0),
            angle: 0.0,
            aspect: AspectLock::Free,
            vertical: 0.0,
            horizontal: 0.0,
        }
    }
}

impl Crop {
    /// Whether the crop changes anything (its aspect lock alone doesn't).
    pub fn is_identity(&self) -> bool {
        self.rect == (0.0, 0.0, 1.0, 1.0) && self.angle == 0.0 && !self.has_perspective()
    }

    /// Whether any perspective correction is set.
    pub fn has_perspective(&self) -> bool {
        self.vertical != 0.0 || self.horizontal != 0.0
    }

    /// The correction as the `(x, y)` strengths [`Crop::source_point`]
    /// uses, for renderers that apply the same mapping.
    pub fn perspective_strength(&self) -> (f32, f32) {
        // Positive vertical narrows the top: the picture grows with y.
        (
            self.horizontal.clamp(-MAX_PERSPECTIVE, MAX_PERSPECTIVE) / MAX_PERSPECTIVE
                * PERSPECTIVE_STRENGTH,
            self.vertical.clamp(-MAX_PERSPECTIVE, MAX_PERSPECTIVE) / MAX_PERSPECTIVE
                * PERSPECTIVE_STRENGTH,
        )
    }

    /// The point of a `width` × `height` photo that canvas pixel `p` (on
    /// the straightened, corrected canvas) shows: turned back by the
    /// angle, then taken through the perspective correction.
    pub fn source_point(&self, p: (f32, f32), width: f32, height: f32) -> (f32, f32) {
        let (rx, ry) = rotate_about_centre(p, -self.angle, width, height);
        let (cx, cy) = (width / 2.0, height / 2.0);
        let (px, py) = self.perspective_strength();
        let w = (1.0 + px * (rx - cx) / cx.max(1e-6) + py * (ry - cy) / cy.max(1e-6)).max(0.05);
        (cx + (rx - cx) / w, cy + (ry - cy) / w)
    }

    /// The inverse of [`Crop::source_point`]: where photo point `s` lands
    /// on the canvas.
    pub fn canvas_point(&self, s: (f32, f32), width: f32, height: f32) -> (f32, f32) {
        let (cx, cy) = (width / 2.0, height / 2.0);
        let (px, py) = self.perspective_strength();
        let (dx, dy) = (s.0 - cx, s.1 - cy);
        let w = 1.0 / (1.0 - px * dx / cx.max(1e-6) - py * dy / cy.max(1e-6)).max(0.05);
        rotate_about_centre((cx + dx * w, cy + dy * w), self.angle, width, height)
    }

    /// The crop after the photo turns a quarter clockwise (or back), so it
    /// keeps framing the same part of the picture.
    pub fn quarter_turned(&self, clockwise: bool) -> Crop {
        let (x, y, w, h) = self.rect;
        let rect = if clockwise {
            (1.0 - y - h, x, h, w)
        } else {
            (y, 1.0 - x - w, h, w)
        };
        let aspect = match self.aspect {
            AspectLock::Ratio(r) => AspectLock::Ratio(1.0 / r),
            other => other,
        };
        Crop {
            rect,
            aspect,
            ..*self
        }
    }

    /// Shrinks the rectangle about its centre until every corner lies on
    /// the picture (not the empty corners straightening leaves), on a
    /// `width` × `height` photo.
    pub fn fit_inside(&mut self, width: f32, height: f32) {
        let (x, y, w, h) = self.rect;
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let corners_ok = |s: f32| {
            let (hw, hh) = (w * s / 2.0, h * s / 2.0);
            [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)]
                .iter()
                .all(|(dx, dy)| self.on_picture((cx + dx, cy + dy), width, height))
        };
        if corners_ok(1.0) {
            return;
        }
        // Binary search for the largest scale that fits.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if corners_ok(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let (nw, nh) = ((w * lo).max(MIN_SIDE), (h * lo).max(MIN_SIDE));
        self.rect = (cx - nw / 2.0, cy - nh / 2.0, nw, nh);
    }

    /// The largest rectangle of the locked shape that fits the picture,
    /// centred: what choosing an aspect or a new angle starts from.
    pub fn reframed(&self, width: f32, height: f32) -> Crop {
        let mut crop = Crop {
            rect: (0.0, 0.0, 1.0, 1.0),
            ..*self
        };
        if let Some(ratio) = self.aspect.ratio(width, height) {
            // In fractions, the shape's width ÷ height is ratio ÷ photo's.
            let r = ratio / (width / height.max(1.0));
            let (w, h) = if r >= 1.0 { (1.0, 1.0 / r) } else { (r, 1.0) };
            crop.rect = ((1.0 - w) / 2.0, (1.0 - h) / 2.0, w, h);
        }
        crop.fit_inside(width, height);
        crop
    }

    /// Whether canvas point `p` (fractions) shows picture rather than an
    /// empty corner, on a `width` × `height` canvas.
    fn on_picture(&self, p: (f32, f32), width: f32, height: f32) -> bool {
        let (x, y) = self.source_point((p.0 * width, p.1 * height), width, height);
        let eps = 1e-3;
        x >= -eps && y >= -eps && x <= width + eps && y <= height + eps
    }

    /// Clamps the rectangle onto the canvas and to a minimum size.
    pub fn clamped(mut self) -> Crop {
        let (x, y, w, h) = self.rect;
        let w = w.clamp(MIN_SIDE, 1.0);
        let h = h.clamp(MIN_SIDE, 1.0);
        self.rect = (x.clamp(0.0, 1.0 - w), y.clamp(0.0, 1.0 - h), w, h);
        self.angle = self.angle.clamp(-MAX_STRAIGHTEN, MAX_STRAIGHTEN);
        self
    }
}

/// Turns pixel point `p` by `degrees` (clockwise on screen) about the
/// centre of a `width` × `height` canvas.
pub fn rotate_about_centre(p: (f32, f32), degrees: f32, width: f32, height: f32) -> (f32, f32) {
    let (cx, cy) = (width / 2.0, height / 2.0);
    let (s, c) = degrees.to_radians().sin_cos();
    let (dx, dy) = (p.0 - cx, p.1 - cy);
    (cx + dx * c - dy * s, cy + dx * s + dy * c)
}

/// Maps between the oriented photo and the cropped, straightened view, in
/// pixels of a view `width` × `height` of the whole oriented photo.
#[derive(Debug, Clone, Copy)]
pub struct CropView {
    pub crop: Crop,
    /// Size of the whole oriented photo, in whatever unit the caller uses
    /// (screen pixels, image pixels).
    pub width: f32,
    pub height: f32,
}

impl CropView {
    /// A point on the oriented photo (fractions) → the view (in units,
    /// from the crop's top-left).
    pub fn to_view(&self, p: (f32, f32)) -> (f32, f32) {
        let (x, y) = self
            .crop
            .canvas_point((p.0 * self.width, p.1 * self.height), self.width, self.height);
        (
            x - self.crop.rect.0 * self.width,
            y - self.crop.rect.1 * self.height,
        )
    }

    /// The inverse of [`CropView::to_view`].
    pub fn from_view(&self, v: (f32, f32)) -> (f32, f32) {
        let canvas = (
            v.0 + self.crop.rect.0 * self.width,
            v.1 + self.crop.rect.1 * self.height,
        );
        let (x, y) = self.crop.source_point(canvas, self.width, self.height);
        (x / self.width.max(1e-6), y / self.height.max(1e-6))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_round_trips() {
        let view = CropView {
            crop: Crop {
                rect: (0.1, 0.2, 0.6, 0.5),
                angle: 7.0,
                aspect: AspectLock::Free,
                vertical: 40.0,
                horizontal: -25.0,
            },
            width: 300.0,
            height: 200.0,
        };
        let p = (0.4, 0.6);
        let back = view.from_view(view.to_view(p));
        assert!((back.0 - p.0).abs() < 1e-4 && (back.1 - p.1).abs() < 1e-4);
    }

    #[test]
    fn perspective_maps_back_and_forth() {
        let crop = Crop {
            angle: -4.0,
            vertical: 60.0,
            horizontal: 30.0,
            ..Default::default()
        };
        let p = (130.0, 70.0);
        let back = crop.canvas_point(crop.source_point(p, 300.0, 200.0), 300.0, 200.0);
        assert!((back.0 - p.0).abs() < 1e-2 && (back.1 - p.1).abs() < 1e-2);
        // The centre stays put.
        let c = crop.source_point((150.0, 100.0), 300.0, 200.0);
        assert!((c.0 - 150.0).abs() < 1e-3 && (c.1 - 100.0).abs() < 1e-3);
    }

    #[test]
    fn perspective_crop_stays_on_the_picture() {
        let mut crop = Crop {
            vertical: 80.0,
            ..Default::default()
        };
        assert!(!crop.is_identity());
        crop.fit_inside(300.0, 200.0);
        let (x, y, w, h) = crop.rect;
        for corner in [(x, y), (x + w, y), (x + w, y + h), (x, y + h)] {
            assert!(crop.on_picture(corner, 300.0, 200.0));
        }
    }

    #[test]
    fn straightening_shrinks_the_crop_onto_the_picture() {
        let mut crop = Crop {
            angle: 10.0,
            ..Default::default()
        };
        crop.fit_inside(300.0, 200.0);
        let (x, y, w, h) = crop.rect;
        assert!(w < 1.0 && h < 1.0);
        for corner in [(x, y), (x + w, y), (x + w, y + h), (x, y + h)] {
            assert!(crop.on_picture(corner, 300.0, 200.0));
        }
    }

    #[test]
    fn aspect_lock_frames_the_right_shape() {
        let crop = Crop {
            aspect: AspectLock::Ratio(1.0),
            ..Default::default()
        }
        .reframed(300.0, 200.0);
        let (_, _, w, h) = crop.rect;
        assert!(((w * 300.0) / (h * 200.0) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn quarter_turns_cancel_out() {
        let crop = Crop {
            rect: (0.1, 0.2, 0.3, 0.4),
            ..Default::default()
        };
        let back = crop.quarter_turned(true).quarter_turned(false);
        for (a, b) in [
            (back.rect.0, 0.1),
            (back.rect.1, 0.2),
            (back.rect.2, 0.3),
            (back.rect.3, 0.4),
        ] {
            assert!((a - b).abs() < 1e-6);
        }
    }
}
