//! The looks the Glow effect can take. The Glow slider sets how much; the
//! style sets what kind of light.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GlowStyle {
    /// Highlights spill soft light around themselves.
    #[default]
    Bloom,
    /// The whole picture softens, as through a pro-mist filter.
    Diffusion,
    /// Bright edges glow warm red-orange, as light scattered in film.
    Halation,
    /// A dreamy, saturated softness: the picture overlaid on its own blur.
    Dreamy,
    /// A low, milky haze: shadows lift and contrast falls.
    Mist,
    /// A warm, wide bloom for a cinematic look.
    Cinema,
}

impl GlowStyle {
    pub const ALL: [GlowStyle; 6] = [
        GlowStyle::Bloom,
        GlowStyle::Diffusion,
        GlowStyle::Halation,
        GlowStyle::Dreamy,
        GlowStyle::Mist,
        GlowStyle::Cinema,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            GlowStyle::Bloom => "Bloom",
            GlowStyle::Diffusion => "Diffusion",
            GlowStyle::Halation => "Halation",
            GlowStyle::Dreamy => "Dreamy",
            GlowStyle::Mist => "Mist",
            GlowStyle::Cinema => "Cinema",
        }
    }

    /// The style's number in the GPU's parameters.
    pub fn index(&self) -> u32 {
        Self::ALL.iter().position(|s| s == self).unwrap_or(0) as u32
    }

    /// Whether the glow comes from the highlights only (blurred and laid
    /// back), as opposed to the whole picture softening.
    pub fn from_highlights(&self) -> bool {
        matches!(self, GlowStyle::Bloom | GlowStyle::Halation | GlowStyle::Cinema)
    }
}
