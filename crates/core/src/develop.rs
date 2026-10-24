use serde::{Deserialize, Serialize};

use crate::{
    crop::Crop,
    curves::ToneCurves,
    frame::FrameSettings,
    parametric::ParametricCurve,
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
    /// 0..=100: the sharpening radius (50 is about one pixel).
    pub sharpen_radius: f32,
    /// 0..=100: how much sharpening is held back from flat areas, so only
    /// edges are sharpened and noise isn't.
    pub sharpen_masking: f32,
    /// 0..=100: smoothing of brightness noise.
    pub noise_luma: f32,
    /// 0..=100: smoothing of colour noise.
    pub noise_color: f32,

    // Lens corrections
    /// -100..=100: positive straightens barrel distortion (bulging
    /// edges), negative pincushion.
    pub lens_distortion: f32,
    /// -100..=100: positive brightens the corners, correcting lens
    /// vignetting.
    pub lens_vignetting: f32,
    /// -100..=100: shifts red and blue against green from the centre
    /// outwards, removing lateral colour fringes.
    pub lens_ca: f32,

    /// Spot removal (heal / clone), applied before everything else.
    pub spots: Spots,
    /// Linear and radial gradient masks with local adjustments.
    pub gradients: Gradients,

    // Tone curve, Color Mixer, Color Grading
    pub curves: ToneCurves,
    /// Highlights / lights / darks / shadows sliders, applied before the
    /// point curves.
    pub parametric: ParametricCurve,
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
    /// Straighten and crop, after rotation and flips.
    pub crop: Crop,
    pub rotation_degrees: f32,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,

    /// A border drawn around the finished photo.
    pub frame: FrameSettings,

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
            sharpen_radius: 50.0,
            sharpen_masking: 0.0,
            noise_luma: 0.0,
            noise_color: 0.0,
            lens_distortion: 0.0,
            lens_vignetting: 0.0,
            lens_ca: 0.0,
            spots: Spots::default(),
            gradients: Gradients::default(),
            curves: ToneCurves::default(),
            parametric: ParametricCurve::default(),
            mixer: ColorMixer::default(),
            grading: ColorGrading::default(),
            dehaze: 0.0,
            vignette: 0.0,
            vignette_midpoint: 50.0,
            vignette_feather: 50.0,
            grain: 0.0,
            grain_size: 25.0,
            crop: Crop::default(),
            rotation_degrees: 0.0,
            flip_horizontal: false,
            flip_vertical: false,
            frame: FrameSettings::default(),
            disabled_sections: 0,
        }
    }
}

impl DevelopSettings {
    /// Whether any adjustment differs from the neutral default.
    pub fn is_edited(&self) -> bool {
        // Asked of every photo on screen every frame: build the defaults
        // once, not each time.
        static NEUTRAL: std::sync::OnceLock<DevelopSettings> = std::sync::OnceLock::new();
        self != NEUTRAL.get_or_init(Self::default)
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// These settings with `look`'s tone, colour and detail values, keeping
    /// this photo's own crop, rotation and flips. How presets are applied.
    pub fn with_look_of(&self, look: &DevelopSettings) -> DevelopSettings {
        DevelopSettings {
            crop: self.crop,
            rotation_degrees: self.rotation_degrees,
            flip_horizontal: self.flip_horizontal,
            flip_vertical: self.flip_vertical,
            frame: self.frame,
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
    SharpenRadius,
    SharpenMasking,
    NoiseLuma,
    NoiseColor,
    LensDistortion,
    LensVignetting,
    LensCa,
    CurveHighlights,
    CurveLights,
    CurveDarks,
    CurveShadows,
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

    pub const DETAIL: [Adjustment; 6] = [
        Adjustment::Clarity,
        Adjustment::Sharpening,
        Adjustment::SharpenRadius,
        Adjustment::SharpenMasking,
        Adjustment::NoiseLuma,
        Adjustment::NoiseColor,
    ];

    pub const LENS: [Adjustment; 3] = [
        Adjustment::LensDistortion,
        Adjustment::LensVignetting,
        Adjustment::LensCa,
    ];

    /// The parametric tone curve's sliders, top of the tonal range first.
    pub const PARAMETRIC: [Adjustment; 4] = [
        Adjustment::CurveHighlights,
        Adjustment::CurveLights,
        Adjustment::CurveDarks,
        Adjustment::CurveShadows,
    ];

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
            Adjustment::SharpenRadius => "Radius",
            Adjustment::SharpenMasking => "Masking",
            Adjustment::NoiseLuma => "Luminance",
            Adjustment::NoiseColor => "Colour",
            Adjustment::LensDistortion => "Distortion",
            Adjustment::LensVignetting => "Vignetting",
            Adjustment::LensCa => "Chromatic aberration",
            Adjustment::CurveHighlights => "Highlights",
            Adjustment::CurveLights => "Lights",
            Adjustment::CurveDarks => "Darks",
            Adjustment::CurveShadows => "Shadows",
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
            Adjustment::SharpenRadius => "Sharpening radius".to_string(),
            Adjustment::SharpenMasking => "Sharpening masking".to_string(),
            Adjustment::NoiseLuma => "Luminance noise reduction".to_string(),
            Adjustment::NoiseColor => "Colour noise reduction".to_string(),
            Adjustment::LensDistortion => "Lens distortion".to_string(),
            Adjustment::LensVignetting => "Lens vignetting".to_string(),
            Adjustment::LensCa => "Chromatic aberration".to_string(),
            Adjustment::CurveHighlights => "Curve highlights".to_string(),
            Adjustment::CurveLights => "Curve lights".to_string(),
            Adjustment::CurveDarks => "Curve darks".to_string(),
            Adjustment::CurveShadows => "Curve shadows".to_string(),
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
            | Adjustment::GrainSize
            | Adjustment::SharpenRadius
            | Adjustment::SharpenMasking
            | Adjustment::NoiseLuma
            | Adjustment::NoiseColor => (0.0, 100.0),
            _ => (-100.0, 100.0),
        }
    }

    /// The neutral value a double-click resets to.
    pub fn default_value(&self) -> f32 {
        self.get(&DevelopSettings::default())
    }

    /// The slider's value is the stored one times this. Tint is stored with
    /// positive as green, but its slider runs green to magenta, so it reads
    /// the other way round: moving towards green adds green.
    pub fn display_sign(&self) -> f32 {
        match self {
            Adjustment::Tint => -1.0,
            _ => 1.0,
        }
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
            Adjustment::SharpenRadius => settings.sharpen_radius,
            Adjustment::SharpenMasking => settings.sharpen_masking,
            Adjustment::NoiseLuma => settings.noise_luma,
            Adjustment::NoiseColor => settings.noise_color,
            Adjustment::LensDistortion => settings.lens_distortion,
            Adjustment::LensVignetting => settings.lens_vignetting,
            Adjustment::LensCa => settings.lens_ca,
            Adjustment::CurveHighlights => settings.parametric.highlights,
            Adjustment::CurveLights => settings.parametric.lights,
            Adjustment::CurveDarks => settings.parametric.darks,
            Adjustment::CurveShadows => settings.parametric.shadows,
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
            Adjustment::SharpenRadius => settings.sharpen_radius = v,
            Adjustment::SharpenMasking => settings.sharpen_masking = v,
            Adjustment::NoiseLuma => settings.noise_luma = v,
            Adjustment::NoiseColor => settings.noise_color = v,
            Adjustment::LensDistortion => settings.lens_distortion = v,
            Adjustment::LensVignetting => settings.lens_vignetting = v,
            Adjustment::LensCa => settings.lens_ca = v,
            Adjustment::CurveHighlights => settings.parametric.highlights = v,
            Adjustment::CurveLights => settings.parametric.lights = v,
            Adjustment::CurveDarks => settings.parametric.darks = v,
            Adjustment::CurveShadows => settings.parametric.shadows = v,
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
            &Adjustment::LENS[..],
            &Adjustment::PARAMETRIC[..],
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

#[cfg(test)]
mod tint_tests {
    use super::*;

    #[test]
    fn the_tint_slider_reads_green_to_magenta() {
        // Stored, positive tint adds green; the slider is green on its left
        // (negative) end, so it shows the opposite sign.
        assert_eq!(Adjustment::Tint.display_sign(), -1.0);
        assert_eq!(crate::LocalProp::Tint.display_sign(), -1.0);
        for adjustment in [Adjustment::Temperature, Adjustment::Exposure, Adjustment::Vibrance] {
            assert_eq!(adjustment.display_sign(), 1.0);
        }
    }
}
