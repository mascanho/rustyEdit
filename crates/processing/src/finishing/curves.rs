//! Tone curves as lookup tables: the master curve composed with each
//! channel's own, so a pixel costs three table reads.

use rustyedit_core::{ParametricCurve, ToneCurve, ToneCurves};

pub(crate) struct CurveLuts([[u8; 256]; 3]);

impl CurveLuts {
    /// `None` when every curve is the straight diagonal.
    pub(crate) fn new(curves: &ToneCurves, parametric: &ParametricCurve) -> Option<Self> {
        if curves.is_identity() && parametric.is_identity() {
            return None;
        }
        // The parametric sliders come first, then the master point curve.
        let shaped = parametric.lut();
        let mut master = lut(&curves.master);
        if !parametric.is_identity() {
            let point = master;
            for (i, out) in master.iter_mut().enumerate() {
                let x = (shaped[i] * 255.0).round().clamp(0.0, 255.0) as usize;
                *out = point[x];
            }
        }
        let channel = |curve: &ToneCurve| {
            let own = lut(curve);
            let mut composed = [0u8; 256];
            for (i, out) in composed.iter_mut().enumerate() {
                *out = own[master[i] as usize];
            }
            composed
        };
        Some(Self([
            channel(&curves.red),
            channel(&curves.green),
            channel(&curves.blue),
        ]))
    }

    pub(crate) fn apply(&self, px: &mut [u8]) {
        for (value, table) in px.iter_mut().zip(&self.0) {
            *value = table[*value as usize];
        }
    }
}

fn lut(curve: &ToneCurve) -> [u8; 256] {
    let mut table = [0u8; 256];
    for (i, out) in table.iter_mut().enumerate() {
        *out = (curve.evaluate(i as f32 / 255.0) * 255.0).round() as u8;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parametric_sliders_shape_the_tones_before_the_point_curves() {
        let parametric = ParametricCurve {
            shadows: 100.0,
            ..Default::default()
        };
        let luts = CurveLuts::new(&ToneCurves::default(), &parametric).unwrap();
        let mut dark = [32, 32, 32];
        luts.apply(&mut dark);
        assert!(dark[0] > 40, "shadows lifted: {}", dark[0]);
        let mut bright = [220, 220, 220];
        luts.apply(&mut bright);
        assert_eq!(bright[0], 220);
        assert!(CurveLuts::new(&ToneCurves::default(), &ParametricCurve::default()).is_none());
    }

    #[test]
    fn channel_curve_applies_on_top_of_master() {
        let mut curves = ToneCurves::default();
        // Master: everything to mid grey; red: invert.
        curves.master.move_point(0, 0.0, 0.5);
        curves.master.move_point(1, 1.0, 0.5);
        curves.red.move_point(0, 0.0, 1.0);
        curves.red.move_point(1, 1.0, 0.0);
        let luts = CurveLuts::new(&curves, &ParametricCurve::default()).unwrap();
        let mut px = [255, 0, 10];
        luts.apply(&mut px);
        assert!((px[0] as i32 - 127).abs() <= 1);
        assert!((px[1] as i32 - 128).abs() <= 1);
    }
}
