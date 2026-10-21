//! Color Mixer: per-colour-range hue, saturation and luminance shifts.
//! Each pixel blends the shifts of the two ranges its hue falls between,
//! so edits fade smoothly across the wheel, and greys are left alone.

use rustyedit_core::{band_weight, BandAdjust, ColorBand, ColorMixer};

use crate::color::{hsl_to_rgb, rgb_to_hsl};

/// Largest hue shift, in degrees, at ±100.
const MAX_HUE_SHIFT: f32 = 30.0;
/// Largest lightness shift at ±100, for a fully saturated colour.
const MAX_LUMINANCE_SHIFT: f32 = 0.25;

pub(crate) struct Mixer(ColorMixer);

impl Mixer {
    pub(crate) fn new(mixer: &ColorMixer) -> Option<Self> {
        let neutral = mixer.iter().all(|band| *band == BandAdjust::default());
        (!neutral).then_some(Self(*mixer))
    }

    pub(crate) fn apply(&self, [r, g, b]: [f32; 3]) -> [f32; 3] {
        let (h, s, l) = rgb_to_hsl(r, g, b);
        if s < 0.02 {
            return [r, g, b];
        }
        let (mut dh, mut ds, mut dl) = (0.0, 0.0, 0.0);
        for band in ColorBand::ALL {
            let w = band_weight(band, h);
            if w <= 0.0 {
                continue;
            }
            let adjust = &self.0[band.index()];
            dh += w * adjust.hue / 100.0 * MAX_HUE_SHIFT;
            ds += w * adjust.saturation / 100.0;
            dl += w * adjust.luminance / 100.0;
        }
        let h = (h + dh).rem_euclid(360.0);
        let s2 = (s * (1.0 + ds)).clamp(0.0, 1.0);
        // Scaled by the original saturation so near-greys barely move.
        let l = (l + dl * MAX_LUMINANCE_SHIFT * s).clamp(0.0, 1.0);
        let (r, g, b) = hsl_to_rgb(h, s2, l);
        [r, g, b]
    }
}
