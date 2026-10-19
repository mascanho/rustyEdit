use serde::{Deserialize, Serialize};

/// The full set of non-destructive edit adjustments for a single asset.
///
/// Every field is a "delta" from the neutral value of `0.0`, mirroring how
/// Lightroom's Basic/Color/Detail panels represent adjustments, so a fresh
/// `DevelopSettings::default()` always means "no edits applied".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

    // Geometry
    pub rotation_degrees: f32,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
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
            rotation_degrees: 0.0,
            flip_horizontal: false,
            flip_vertical: false,
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
}

/// Identifies a single adjustable slider in the Develop panel. Used by the
/// UI layer so widgets stay generic over "which value am I editing".
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
        }
    }

    /// The `(min, max)` range this adjustment is displayed and dragged within.
    pub fn range(&self) -> (f32, f32) {
        match self {
            Adjustment::Temperature => (-100.0, 100.0),
            Adjustment::Tint => (-100.0, 100.0),
            _ => (-100.0, 100.0),
        }
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
        }
    }

    pub fn set(&self, settings: &mut DevelopSettings, value: f32) {
        let (min, max) = self.range();
        let clamped = value.clamp(min, max);
        match self {
            Adjustment::Exposure => settings.exposure = clamped,
            Adjustment::Contrast => settings.contrast = clamped,
            Adjustment::Highlights => settings.highlights = clamped,
            Adjustment::Shadows => settings.shadows = clamped,
            Adjustment::Whites => settings.whites = clamped,
            Adjustment::Blacks => settings.blacks = clamped,
            Adjustment::Temperature => settings.temperature = clamped,
            Adjustment::Tint => settings.tint = clamped,
            Adjustment::Vibrance => settings.vibrance = clamped,
            Adjustment::Saturation => settings.saturation = clamped,
            Adjustment::Clarity => settings.clarity = clamped,
            Adjustment::Sharpening => settings.sharpening = clamped,
        }
    }
}
