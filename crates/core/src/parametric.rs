//! The parametric tone curve: four sliders (highlights, lights, darks,
//! shadows) that lift or lower a band of tones each, as an alternative to
//! placing points on the curve. It is applied before the point curves.

use serde::{Deserialize, Serialize};

/// Each -100..=100, 0 meaning no change.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ParametricCurve {
    pub highlights: f32,
    pub lights: f32,
    pub darks: f32,
    pub shadows: f32,
}

/// The tone each slider is centred on, shadows to highlights.
const CENTRES: [f32; 4] = [0.125, 0.375, 0.625, 0.875];
/// How far a slider at full moves its centre, as a fraction of the range.
const REACH: f32 = 0.22;

impl ParametricCurve {
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }

    /// The output for input `x` (`0..=1`).
    pub fn evaluate(&self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        let amounts = [self.shadows, self.darks, self.lights, self.highlights];
        let mut y = x;
        for (centre, amount) in CENTRES.iter().zip(amounts) {
            // A raised-cosine bump, one quarter of the range either side.
            let d = ((x - centre) / 0.25).abs();
            if d < 1.0 {
                let bump = 0.5 * (1.0 + (d * std::f32::consts::PI).cos());
                y += amount / 100.0 * REACH * bump;
            }
        }
        y.clamp(0.0, 1.0)
    }

    /// The curve as a 256-entry table, forced to rise so no slider can
    /// invert tones.
    pub fn lut(&self) -> [f32; 256] {
        let mut table = [0.0f32; 256];
        let mut last = 0.0f32;
        for (i, out) in table.iter_mut().enumerate() {
            last = last.max(self.evaluate(i as f32 / 255.0));
            *out = last;
        }
        table
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_is_the_diagonal() {
        let curve = ParametricCurve::default();
        assert!(curve.is_identity());
        for i in 0..=10 {
            let x = i as f32 / 10.0;
            assert!((curve.evaluate(x) - x).abs() < 1e-6);
        }
    }

    #[test]
    fn each_slider_moves_its_own_band() {
        let lift_shadows = ParametricCurve {
            shadows: 100.0,
            ..Default::default()
        };
        assert!(lift_shadows.evaluate(0.125) > 0.125 + 0.1);
        assert!((lift_shadows.evaluate(0.8) - 0.8).abs() < 1e-6);

        let dim_highlights = ParametricCurve {
            highlights: -100.0,
            ..Default::default()
        };
        assert!(dim_highlights.evaluate(0.875) < 0.875 - 0.1);
        assert!((dim_highlights.evaluate(0.2) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn lut_never_falls() {
        let extreme = ParametricCurve {
            shadows: 100.0,
            darks: -100.0,
            lights: 100.0,
            highlights: -100.0,
        };
        let lut = extreme.lut();
        assert!(lut.windows(2).all(|w| w[1] >= w[0]));
    }
}
