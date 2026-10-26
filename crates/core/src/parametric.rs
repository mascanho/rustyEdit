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

    /// The band (0 shadows, 1 darks, 2 lights, 3 highlights) input `x`
    /// falls in.
    pub fn band_at(x: f32) -> usize {
        ((x.clamp(0.0, 1.0) * 4.0) as usize).min(3)
    }

    /// Band `band`'s amount (-100..=100).
    pub fn amount(&self, band: usize) -> f32 {
        match band {
            0 => self.shadows,
            1 => self.darks,
            2 => self.lights,
            _ => self.highlights,
        }
    }

    pub fn set_amount(&mut self, band: usize, value: f32) {
        let value = value.clamp(-100.0, 100.0);
        match band {
            0 => self.shadows = value,
            1 => self.darks = value,
            2 => self.lights = value,
            _ => self.highlights = value,
        }
    }

    /// Where band `band`'s handle sits on the curve: its centre tone and
    /// that tone's output. Only that band moves the curve there, so the
    /// handle's height is its amount.
    pub fn handle(&self, band: usize) -> (f32, f32) {
        let x = CENTRES[band.min(3)];
        (x, self.evaluate(x))
    }

    /// How far the output moves per unit of amount, so a drag of `dy`
    /// (fractions of the graph) changes the amount by `dy * amount_per_output()`.
    pub fn amount_per_output() -> f32 {
        100.0 / REACH
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
    fn a_handle_sits_at_its_band_amount() {
        let mut curve = ParametricCurve::default();
        curve.set_amount(1, 50.0);
        assert_eq!(curve.darks, 50.0);
        let (x, y) = curve.handle(1);
        assert!((y - x - 0.5 * REACH).abs() < 1e-5);
        // Neighbouring bands are untouched at their own handles.
        let (x0, y0) = curve.handle(0);
        assert!((y0 - x0).abs() < 1e-5);
        assert_eq!(ParametricCurve::band_at(0.3), 1);
        assert_eq!(ParametricCurve::band_at(1.0), 3);
        curve.set_amount(3, 400.0);
        assert_eq!(curve.highlights, 100.0);
    }

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
