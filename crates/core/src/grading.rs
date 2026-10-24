//! Color Grading: tint and brighten the shadows, midtones and highlights
//! separately (split toning, extended to three ranges, as the Lift, Gamma
//! and Gain wheels of a colourist's panel), plus an Offset that moves the
//! whole picture, with a balance between the ranges.

use serde::{Deserialize, Serialize};

/// A tonal range color grading tints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToneZone {
    Shadows,
    Midtones,
    Highlights,
    /// The whole picture, whatever its tone: the Offset wheel.
    Global,
}

impl ToneZone {
    /// The three tonal ranges (the Global offset isn't one).
    pub const ALL: [ToneZone; 3] = [ToneZone::Shadows, ToneZone::Midtones, ToneZone::Highlights];
    /// Every wheel, in the order they're shown.
    pub const WHEELS: [ToneZone; 4] = [
        ToneZone::Shadows,
        ToneZone::Midtones,
        ToneZone::Highlights,
        ToneZone::Global,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ToneZone::Shadows => "Shadows",
            ToneZone::Midtones => "Midtones",
            ToneZone::Highlights => "Highlights",
            ToneZone::Global => "Offset",
        }
    }

    /// The colourist's name for the wheel.
    pub fn wheel_name(&self) -> &'static str {
        match self {
            ToneZone::Shadows => "Lift",
            ToneZone::Midtones => "Gamma",
            ToneZone::Highlights => "Gain",
            ToneZone::Global => "Offset",
        }
    }
}

/// Which property of a zone's tint a grading slider changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GradeProp {
    /// 0..=360 degrees.
    Hue,
    /// 0..=100; 0 means no tint.
    Saturation,
    /// -100..=100: makes the range brighter or darker.
    Luminance,
}

/// One zone's tint.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Grade {
    pub hue: f32,
    pub saturation: f32,
    /// -100..=100: brightens or darkens the range, as the wheel's master
    /// slider does.
    pub luminance: f32,
}

impl Grade {
    /// Whether the range is changed at all.
    pub fn is_active(&self) -> bool {
        self.saturation > 0.0 || self.luminance != 0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorGrading {
    pub shadows: Grade,
    pub midtones: Grade,
    pub highlights: Grade,
    /// The Offset wheel: tints and brightens everything alike.
    pub global: Grade,
    /// -100..=100: negative favours the shadows' tint, positive the
    /// highlights'.
    pub balance: f32,
}

impl ColorGrading {
    pub fn zone(&self, zone: ToneZone) -> &Grade {
        match zone {
            ToneZone::Shadows => &self.shadows,
            ToneZone::Midtones => &self.midtones,
            ToneZone::Highlights => &self.highlights,
            ToneZone::Global => &self.global,
        }
    }

    pub fn zone_mut(&mut self, zone: ToneZone) -> &mut Grade {
        match zone {
            ToneZone::Shadows => &mut self.shadows,
            ToneZone::Midtones => &mut self.midtones,
            ToneZone::Highlights => &mut self.highlights,
            ToneZone::Global => &mut self.global,
        }
    }

    /// Whether any wheel changes the picture. Balance alone changes nothing.
    pub fn is_active(&self) -> bool {
        ToneZone::WHEELS
            .iter()
            .any(|zone| self.zone(*zone).is_active())
    }
}
