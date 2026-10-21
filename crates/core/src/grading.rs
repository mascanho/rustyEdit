//! Color Grading: tint the shadows, midtones and highlights separately
//! (split toning, extended to three ranges), with a balance between them.

use serde::{Deserialize, Serialize};

/// A tonal range color grading tints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToneZone {
    Shadows,
    Midtones,
    Highlights,
}

impl ToneZone {
    pub const ALL: [ToneZone; 3] = [ToneZone::Shadows, ToneZone::Midtones, ToneZone::Highlights];

    pub fn label(&self) -> &'static str {
        match self {
            ToneZone::Shadows => "Shadows",
            ToneZone::Midtones => "Midtones",
            ToneZone::Highlights => "Highlights",
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
}

/// One zone's tint.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Grade {
    pub hue: f32,
    pub saturation: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorGrading {
    pub shadows: Grade,
    pub midtones: Grade,
    pub highlights: Grade,
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
        }
    }

    pub fn zone_mut(&mut self, zone: ToneZone) -> &mut Grade {
        match zone {
            ToneZone::Shadows => &mut self.shadows,
            ToneZone::Midtones => &mut self.midtones,
            ToneZone::Highlights => &mut self.highlights,
        }
    }

    /// Whether any zone is tinted. Balance alone changes nothing.
    pub fn is_active(&self) -> bool {
        ToneZone::ALL
            .iter()
            .any(|zone| self.zone(*zone).saturation > 0.0)
    }
}
