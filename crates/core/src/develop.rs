use serde::{Deserialize, Serialize};

use crate::{
    curves::ToneCurves,
    gradients::Gradients,
    retouch::Spots,
    grading::{ColorGrading, GradeProp, ToneZone},
    mixer::{ColorBand, ColorMixer, MixerProp},
};

/// The full set of non-destructive edit adjustments for a single asset.
///
/// Most fields are a "delta" from a neutral `0.0`, mirroring how
/// Lightroom's panels represent adjustments, so a fresh
/// `DevelopSettings::default()` always means "no edits applied". The
/// exceptions (vignette shape, grain size) default to the middle of their
/// range and do nothing while their effect's amount is zero.
///
/// `#[serde(default)]` so catalogs, presets and snapshots saved before a
/// field existed still load, with that field neutral.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DevelopSettings {
    // Basic / Tone
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,

    // Color
    pub temperature: f32,
    pub tint: f32,
    pub vibrance: f32,
    pub saturation: f32,

    // Detail
    pub clarity: f32,
    pub sharpening: f32,

    /// Spot removal (heal / clone), applied before everything else.
    pub spots: Spots,
    /// Linear and radial gradient masks with local adjustments.
    pub gradients: Gradients,

    // Tone curve, Color Mixer, Color Grading
    pub curves: ToneCurves,
    pub mixer: ColorMixer,
    pub grading: ColorGrading,

    // Effects
    pub dehaze: f32,
    /// -100 darkens the corners, +100 lightens them.
    pub vignette: f32,
    /// 0..=100: how far in from the corners the vignette starts.
    pub vignette_midpoint: f32,
    /// 0..=100: how gradually it fades in.
    pub vignette_feather: f32,
    pub grain: f32,
    pub grain_size: f32,

    // Geometry
    pub rotation_degrees: f32,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,

    /// Sections switched off in the panel, one bit per
    /// [`crate::DevelopSection`]: their values are kept but not applied
    /// (see [`DevelopSettings::effective`]).
    pub disabled_sections: u16,
}

impl Default for DevelopSettings {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            temperature: 0.0,
            tint: 0.0,
            vibrance: 0.0,
            saturation: 0.0,
            clarity: 0.0,
            sharpening: 0.0,
            spots: Spots::default(),
            gradients: Gradients::default(),
            curves: ToneCurves::default(),
            mixer: ColorMixer::default(),
            grading: ColorGrading::default(),
            dehaze: 0.0,
            vignette: 0.0,
            vignette_midpoint: 50.0,
            vignette_feather: 50.0,
            grain: 0.0,
            grain_size: 25.0,
            rotation_degrees: 0.0,
            flip_horizontal: false,
            flip_vertical: false,
            disabled_sections: 0,
        }
    }
}

impl DevelopSettings {
    /// Whether any adjustment differs from the neutral default.
    pub fn is_edited(&self) -> bool {
        *self != Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// These settings with `look`'s tone, colour and detail values, keeping
    /// this photo's own rotation and flips. How presets are applied.
    pub fn with_look_of(&self, look: &DevelopSettings) -> DevelopSettings {
        DevelopSettings {
            rotation_degrees: self.rotation_degrees,
            flip_horizontal: self.flip_horizontal,
            flip_vertical: self.flip_vertical,
            ..*look
        }
    }
}

/// Identifies a single slider in the Develop panel. Used by the UI layer
/// so widgets stay generic over "which value am I editing".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adjustment {
    Exposure,
    Contrast,
    Highlights,
    Shadows,
    Whites,
    Blacks,
    Temperature,
    Tint,
    Vibrance,
    Saturation,
    Clarity,
    Sharpening,
    /// One colour range's hue, saturation or luminance in the Color Mixer.
    Mixer(ColorBand, MixerProp),
    /// One tonal range's tint in Color Grading.
    Grade(ToneZone, GradeProp),
    GradingBalance,
    Dehaze,
    Vignette,
    VignetteMidpoint,
    VignetteFeather,
    Grain,
    GrainSize,
}

impl Adjustment {
    pub const BASIC: [Adjustment; 6] = [
        Adjustment::Exposure,
        Adjustment::Contrast,
        Adjustment::Highlights,
        Adjustment::Shadows,
        Adjustment::Whites,
        Adjustment::Blacks,
    ];

    pub const COLOR: [Adjustment; 4] = [
        Adjustment::Temperature,
        Adjustment::Tint,
        Adjustment::Vibrance,
        Adjustment::Saturation,
    ];

    pub const DETAIL: [Adjustment; 2] = [Adjustment::Clarity, Adjustment::Sharpening];

    pub const EFFECTS: [Adjustment; 6] = [
        Adjustment::Dehaze,
        Adjustment::Vignette,
        Adjustment::VignetteMidpoint,
        Adjustment::VignetteFeather,
        Adjustment::Grain,
        Adjustment::GrainSize,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Adjustment::Exposure => "Exposure",
            Adjustment::Contrast => "Contrast",
            Adjustment::Highlights => "Highlights",
            Adjustment::Shadows => "Shadows",
            Adjustment::Whites => "Whites",
            Adjustment::Blacks => "Blacks",
            Adjustment::Temperature => "Temp",
            Adjustment::Tint => "Tint",
            Adjustment::Vibrance => "Vibrance",
            Adjustment::Saturation => "Saturation",
            Adjustment::Clarity => "Clarity",
            Adjustment::Sharpening => "Sharpening",
            // Shown under a Hue / Saturation / Luminance tab, so the
            // colour alone identifies the slider.
            Adjustment::Mixer(band, _) => band.label(),
            Adjustment::Grade(_, GradeProp::Hue) => "Hue",
            Adjustment::Grade(_, GradeProp::Saturation) => "Saturation",
            Adjustment::GradingBalance => "Balance",
            Adjustment::Dehaze => "Dehaze",
            Adjustment::Vignette => "Vignette",
            Adjustment::VignetteMidpoint => "Midpoint",
            Adjustment::VignetteFeather => "Feather",
            Adjustment::Grain => "Grain",
            Adjustment::GrainSize => "Grain size",
        }
    }

    /// A self-contained name for history and undo, e.g. "Blue Saturation".
    pub fn history_label(&self) -> String {
        match self {
            Adjustment::Mixer(band, prop) => format!("{} {}", band.label(), prop.label()),
            Adjustment::Grade(zone, GradeProp::Hue) => format!("{} hue", zone.label()),
            Adjustment::Grade(zone, GradeProp::Saturation) => {
                format!("{} saturation", zone.label())
            }
            Adjustment::GradingBalance => "Grading balance".to_string(),
            other => other.label().to_string(),
        }
    }

    /// The `(min, max)` range this adjustment is displayed and dragged within.
    pub fn range(&self) -> (f32, f32) {
        match self {
            Adjustment::Grade(_, GradeProp::Hue) => (0.0, 360.0),
            Adjustment::Grade(_, GradeProp::Saturation)
            | Adjustment::VignetteMidpoint
            | Adjustment::VignetteFeather
            | Adjustment::Grain
            | Adjustment::GrainSize => (0.0, 100.0),
            _ => (-100.0, 100.0),
        }
    }

    /// The neutral value a double-click resets to.
    pub fn default_value(&self) -> f32 {
        self.get(&DevelopSettings::default())
    }

    /// Whether values read as offsets ("+12") rather than amounts ("12").
    pub fn is_signed(&self) -> bool {
        self.range().0 < 0.0
    }

    pub fn get(&self, settings: &DevelopSettings) -> f32 {
        match self {
            Adjustment::Exposure => settings.exposure,
            Adjustment::Contrast => settings.contrast,
            Adjustment::Highlights => settings.highlights,
            Adjustment::Shadows => settings.shadows,
            Adjustment::Whites => settings.whites,
            Adjustment::Blacks => settings.blacks,
            Adjustment::Temperature => settings.temperature,
            Adjustment::Tint => settings.tint,
            Adjustment::Vibrance => settings.vibrance,
            Adjustment::Saturation => settings.saturation,
            Adjustment::Clarity => settings.clarity,
            Adjustment::Sharpening => settings.sharpening,
            Adjustment::Mixer(band, prop) => settings.mixer[band.index()].get(*prop),
            Adjustment::Grade(zone, GradeProp::Hue) => settings.grading.zone(*zone).hue,
            Adjustment::Grade(zone, GradeProp::Saturation) => {
                settings.grading.zone(*zone).saturation
            }
            Adjustment::GradingBalance => settings.grading.balance,
            Adjustment::Dehaze => settings.dehaze,
            Adjustment::Vignette => settings.vignette,
            Adjustment::VignetteMidpoint => settings.vignette_midpoint,
            Adjustment::VignetteFeather => settings.vignette_feather,
            Adjustment::Grain => settings.grain,
            Adjustment::GrainSize => settings.grain_size,
        }
    }

    pub fn set(&self, settings: &mut DevelopSettings, value: f32) {
        let (min, max) = self.range();
        let v = value.clamp(min, max);
        match self {
            Adjustment::Exposure => settings.exposure = v,
            Adjustment::Contrast => settings.contrast = v,
            Adjustment::Highlights => settings.highlights = v,
            Adjustment::Shadows => settings.shadows = v,
            Adjustment::Whites => settings.whites = v,
            Adjustment::Blacks => settings.blacks = v,
            Adjustment::Temperature => settings.temperature = v,
            Adjustment::Tint => settings.tint = v,
            Adjustment::Vibrance => settings.vibrance = v,
            Adjustment::Saturation => settings.saturation = v,
            Adjustment::Clarity => settings.clarity = v,
            Adjustment::Sharpening => settings.sharpening = v,
            Adjustment::Mixer(band, prop) => settings.mixer[band.index()].set(*prop, v),
            Adjustment::Grade(zone, GradeProp::Hue) => settings.grading.zone_mut(*zone).hue = v,
            Adjustment::Grade(zone, GradeProp::Saturation) => {
                settings.grading.zone_mut(*zone).saturation = v
            }
            Adjustment::GradingBalance => settings.grading.balance = v,
            Adjustment::Dehaze => settings.dehaze = v,
            Adjustment::Vignette => settings.vignette = v,
            Adjustment::VignetteMidpoint => settings.vignette_midpoint = v,
            Adjustment::VignetteFeather => settings.vignette_feather = v,
            Adjustment::Grain => settings.grain = v,
            Adjustment::GrainSize => settings.grain_size = v,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_without_new_fields_still_load() {
        let json = r#"{"exposure":12.0,"contrast":0.0,"highlights":0.0,"shadows":0.0,
            "whites":0.0,"blacks":0.0,"temperature":0.0,"tint":0.0,"vibrance":0.0,
            "saturation":0.0,"clarity":0.0,"sharpening":0.0,"rotation_degrees":90.0,
            "flip_horizontal":false,"flip_vertical":false}"#;
        let settings: DevelopSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.exposure, 12.0);
        assert_eq!(settings.vignette_midpoint, 50.0);
        assert!(settings.curves.is_identity());
    }

    #[test]
    fn every_adjustment_round_trips_and_defaults_to_neutral() {
        let mut all: Vec<Adjustment> = [
            &Adjustment::BASIC[..],
            &Adjustment::COLOR[..],
            &Adjustment::DETAIL[..],
            &Adjustment::EFFECTS[..],
        ]
        .concat();
        for band in ColorBand::ALL {
            for prop in MixerProp::ALL {
                all.push(Adjustment::Mixer(band, prop));
            }
        }
        for zone in ToneZone::ALL {
            all.push(Adjustment::Grade(zone, GradeProp::Hue));
            all.push(Adjustment::Grade(zone, GradeProp::Saturation));
        }
        all.push(Adjustment::GradingBalance);

        for adjustment in all {
            let mut settings = DevelopSettings::default();
            let (_, max) = adjustment.range();
            adjustment.set(&mut settings, max);
            assert_eq!(adjustment.get(&settings), max, "{adjustment:?}");
            assert!(settings.is_edited(), "{adjustment:?}");
            adjustment.set(&mut settings, adjustment.default_value());
            assert!(!settings.is_edited(), "{adjustment:?}");
        }
    }
}
