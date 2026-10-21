//! Color Mixer (HSL): shift the hue, saturation and luminance of one
//! colour range at a time, e.g. deepen blue skies without touching skin.

use serde::{Deserialize, Serialize};

/// The eight colour ranges, around the hue wheel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorBand {
    Red,
    Orange,
    Yellow,
    Green,
    Aqua,
    Blue,
    Purple,
    Magenta,
}

impl ColorBand {
    pub const ALL: [ColorBand; 8] = [
        ColorBand::Red,
        ColorBand::Orange,
        ColorBand::Yellow,
        ColorBand::Green,
        ColorBand::Aqua,
        ColorBand::Blue,
        ColorBand::Purple,
        ColorBand::Magenta,
    ];

    pub fn index(&self) -> usize {
        *self as usize
    }

    pub fn label(&self) -> &'static str {
        match self {
            ColorBand::Red => "Red",
            ColorBand::Orange => "Orange",
            ColorBand::Yellow => "Yellow",
            ColorBand::Green => "Green",
            ColorBand::Aqua => "Aqua",
            ColorBand::Blue => "Blue",
            ColorBand::Purple => "Purple",
            ColorBand::Magenta => "Magenta",
        }
    }

    /// Centre of the range on the hue wheel, in degrees.
    pub fn hue(&self) -> f32 {
        match self {
            ColorBand::Red => 0.0,
            ColorBand::Orange => 30.0,
            ColorBand::Yellow => 60.0,
            ColorBand::Green => 120.0,
            ColorBand::Aqua => 180.0,
            ColorBand::Blue => 225.0,
            ColorBand::Purple => 270.0,
            ColorBand::Magenta => 315.0,
        }
    }
}

/// Which property of a colour range a mixer slider changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum MixerProp {
    #[default]
    Hue,
    Saturation,
    Luminance,
}

impl MixerProp {
    pub const ALL: [MixerProp; 3] = [MixerProp::Hue, MixerProp::Saturation, MixerProp::Luminance];

    pub fn label(&self) -> &'static str {
        match self {
            MixerProp::Hue => "Hue",
            MixerProp::Saturation => "Saturation",
            MixerProp::Luminance => "Luminance",
        }
    }
}

/// One colour range's shifts, each -100..=100.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BandAdjust {
    pub hue: f32,
    pub saturation: f32,
    pub luminance: f32,
}

impl BandAdjust {
    pub fn get(&self, prop: MixerProp) -> f32 {
        match prop {
            MixerProp::Hue => self.hue,
            MixerProp::Saturation => self.saturation,
            MixerProp::Luminance => self.luminance,
        }
    }

    pub fn set(&mut self, prop: MixerProp, value: f32) {
        match prop {
            MixerProp::Hue => self.hue = value,
            MixerProp::Saturation => self.saturation = value,
            MixerProp::Luminance => self.luminance = value,
        }
    }
}

/// All eight ranges, indexed by [`ColorBand::index`].
pub type ColorMixer = [BandAdjust; 8];

/// How strongly a pixel of hue `hue` (degrees) belongs to `band`: 1 at the
/// band's centre, fading to 0 at its neighbours' centres.
pub fn band_weight(band: ColorBand, hue: f32) -> f32 {
    let ix = band.index();
    let all = ColorBand::ALL;
    let prev = all[(ix + all.len() - 1) % all.len()].hue();
    let next = all[(ix + 1) % all.len()].hue();
    let centre = band.hue();
    let diff = |a: f32, b: f32| ((a - b + 540.0) % 360.0) - 180.0;
    let d = diff(hue, centre);
    let span = if d >= 0.0 {
        diff(next, centre).abs()
    } else {
        diff(centre, prev).abs()
    };
    (1.0 - d.abs() / span.max(1.0)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_peak_at_centres_and_sum_to_one() {
        for band in ColorBand::ALL {
            assert!((band_weight(band, band.hue()) - 1.0).abs() < 1e-5);
        }
        for hue in (0..360).step_by(7) {
            let total: f32 = ColorBand::ALL
                .iter()
                .map(|b| band_weight(*b, hue as f32))
                .sum();
            assert!((total - 1.0).abs() < 1e-3, "hue {hue}: {total}");
        }
    }
}
