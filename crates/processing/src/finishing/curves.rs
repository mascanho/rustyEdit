//! Tone curves as lookup tables: the master curve composed with each
//! channel's own, so a pixel costs three table reads.

use rustyedit_core::{ToneCurve, ToneCurves};

pub(crate) struct CurveLuts([[u8; 256]; 3]);

impl CurveLuts {
    /// `None` when every curve is the straight diagonal.
    pub(crate) fn new(curves: &ToneCurves) -> Option<Self> {
        if curves.is_identity() {
            return None;
        }
        let master = lut(&curves.master);
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
    fn channel_curve_applies_on_top_of_master() {
        let mut curves = ToneCurves::default();
        // Master: everything to mid grey; red: invert.
        curves.master.move_point(0, 0.0, 0.5);
        curves.master.move_point(1, 1.0, 0.5);
        curves.red.move_point(0, 0.0, 1.0);
        curves.red.move_point(1, 1.0, 0.0);
        let luts = CurveLuts::new(&curves).unwrap();
        let mut px = [255, 0, 10];
        luts.apply(&mut px);
        assert!((px[0] as i32 - 127).abs() <= 1);
        assert!((px[1] as i32 - 128).abs() <= 1);
    }
}
