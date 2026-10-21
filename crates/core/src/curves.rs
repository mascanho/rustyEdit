//! Tone curves: a master RGB curve plus one per channel, each a handful
//! of control points joined by a smooth, monotone spline.
//!
//! Points are stored in a fixed-size array so `DevelopSettings` stays
//! `Copy` (presets, snapshots, history and undo all copy it freely).

use serde::{Deserialize, Serialize};

/// Most control points a curve can have, endpoints included.
pub const MAX_CURVE_POINTS: usize = 8;
/// Closest two points may sit horizontally, so the spline stays defined.
const MIN_GAP: f32 = 0.02;

/// One curve: `(input, output)` points in `0..=1`, sorted by input. The
/// first and last points always sit at input 0 and 1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ToneCurve {
    points: [[f32; 2]; MAX_CURVE_POINTS],
    len: u8,
}

impl Default for ToneCurve {
    fn default() -> Self {
        let mut points = [[0.0; 2]; MAX_CURVE_POINTS];
        points[1] = [1.0, 1.0];
        Self { points, len: 2 }
    }
}

impl ToneCurve {
    pub fn points(&self) -> &[[f32; 2]] {
        &self.points[..self.len()]
    }

    fn len(&self) -> usize {
        (self.len as usize).clamp(2, MAX_CURVE_POINTS)
    }

    /// The straight diagonal, which leaves the image unchanged.
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }

    /// Adds a point at (`x`, `y`) and returns its index, or `None` when the
    /// curve is full or `x` is too close to an existing point.
    pub fn insert(&mut self, x: f32, y: f32) -> Option<usize> {
        let len = self.len();
        if len >= MAX_CURVE_POINTS {
            return None;
        }
        let (x, y) = (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0));
        let at = self.points().iter().position(|p| p[0] > x)?;
        if at == 0 || x - self.points[at - 1][0] < MIN_GAP || self.points[at][0] - x < MIN_GAP {
            return None;
        }
        self.points.copy_within(at..len, at + 1);
        self.points[at] = [x, y];
        self.len += 1;
        Some(at)
    }

    /// Moves point `index`, keeping it between its neighbours. Endpoints
    /// only move up and down.
    pub fn move_point(&mut self, index: usize, x: f32, y: f32) {
        let len = self.len();
        if index >= len {
            return;
        }
        let x = if index == 0 {
            0.0
        } else if index == len - 1 {
            1.0
        } else {
            let lo = self.points[index - 1][0] + MIN_GAP;
            let hi = self.points[index + 1][0] - MIN_GAP;
            x.clamp(lo, hi.max(lo))
        };
        self.points[index] = [x, y.clamp(0.0, 1.0)];
    }

    /// Removes point `index`; endpoints can't be removed.
    pub fn remove(&mut self, index: usize) {
        let len = self.len();
        if index == 0 || index >= len - 1 {
            return;
        }
        self.points.copy_within(index + 1..len, index);
        self.points[len - 1] = [0.0; 2];
        self.len -= 1;
    }

    /// The curve's output for input `x`: a monotone cubic (Fritsch-Carlson)
    /// through the points, so it never overshoots between them.
    pub fn evaluate(&self, x: f32) -> f32 {
        let pts = self.points();
        let x = x.clamp(0.0, 1.0);
        let n = pts.len();
        let seg = pts.windows(2).position(|w| x <= w[1][0]).unwrap_or(n - 2);

        let slope = |i: usize| {
            let dx = pts[i + 1][0] - pts[i][0];
            if dx <= f32::EPSILON {
                0.0
            } else {
                (pts[i + 1][1] - pts[i][1]) / dx
            }
        };
        let tangent = |i: usize| {
            if i == 0 {
                slope(0)
            } else if i == n - 1 {
                slope(n - 2)
            } else {
                let (a, b) = (slope(i - 1), slope(i));
                if a * b <= 0.0 {
                    0.0
                } else {
                    // Harmonic mean keeps the spline monotone.
                    2.0 * a * b / (a + b)
                }
            }
        };

        let [x0, y0] = pts[seg];
        let [x1, y1] = pts[seg + 1];
        let h = (x1 - x0).max(f32::EPSILON);
        let t = ((x - x0) / h).clamp(0.0, 1.0);
        let (m0, m1) = (tangent(seg) * h, tangent(seg + 1) * h);
        let (t2, t3) = (t * t, t * t * t);
        let y = (2.0 * t3 - 3.0 * t2 + 1.0) * y0
            + (t3 - 2.0 * t2 + t) * m0
            + (-2.0 * t3 + 3.0 * t2) * y1
            + (t3 - t2) * m1;
        y.clamp(0.0, 1.0)
    }
}

/// Which curve is being edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CurveChannel {
    #[default]
    Master,
    Red,
    Green,
    Blue,
}

impl CurveChannel {
    pub const ALL: [CurveChannel; 4] = [
        CurveChannel::Master,
        CurveChannel::Red,
        CurveChannel::Green,
        CurveChannel::Blue,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            CurveChannel::Master => "RGB",
            CurveChannel::Red => "Red",
            CurveChannel::Green => "Green",
            CurveChannel::Blue => "Blue",
        }
    }
}

/// The four curves a photo has.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ToneCurves {
    pub master: ToneCurve,
    pub red: ToneCurve,
    pub green: ToneCurve,
    pub blue: ToneCurve,
}

impl ToneCurves {
    pub fn get(&self, channel: CurveChannel) -> &ToneCurve {
        match channel {
            CurveChannel::Master => &self.master,
            CurveChannel::Red => &self.red,
            CurveChannel::Green => &self.green,
            CurveChannel::Blue => &self.blue,
        }
    }

    pub fn get_mut(&mut self, channel: CurveChannel) -> &mut ToneCurve {
        match channel {
            CurveChannel::Master => &mut self.master,
            CurveChannel::Red => &mut self.red,
            CurveChannel::Green => &mut self.green,
            CurveChannel::Blue => &mut self.blue,
        }
    }

    pub fn is_identity(&self) -> bool {
        CurveChannel::ALL
            .iter()
            .all(|channel| self.get(*channel).is_identity())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_maps_input_to_itself() {
        let curve = ToneCurve::default();
        for i in 0..=10 {
            let x = i as f32 / 10.0;
            assert!((curve.evaluate(x) - x).abs() < 1e-5);
        }
    }

    #[test]
    fn insert_keeps_order_and_passes_through_points() {
        let mut curve = ToneCurve::default();
        let ix = curve.insert(0.5, 0.7).unwrap();
        assert_eq!(ix, 1);
        assert!((curve.evaluate(0.5) - 0.7).abs() < 1e-5);
        assert!(
            curve.insert(0.51, 0.2).is_none(),
            "too close to a neighbour"
        );
        assert_eq!(curve.insert(0.25, 0.3), Some(1));
        assert_eq!(curve.points()[2], [0.5, 0.7]);
    }

    #[test]
    fn endpoints_stay_pinned_and_cannot_be_removed() {
        let mut curve = ToneCurve::default();
        curve.move_point(0, 0.4, 0.1);
        assert_eq!(curve.points()[0], [0.0, 0.1]);
        curve.remove(0);
        assert_eq!(curve.points().len(), 2);
    }

    #[test]
    fn monotone_points_give_a_monotone_curve() {
        let mut curve = ToneCurve::default();
        curve.insert(0.3, 0.1);
        curve.insert(0.6, 0.9);
        let mut last = 0.0;
        for i in 0..=100 {
            let y = curve.evaluate(i as f32 / 100.0);
            assert!(y + 1e-5 >= last);
            last = y;
        }
    }
}
