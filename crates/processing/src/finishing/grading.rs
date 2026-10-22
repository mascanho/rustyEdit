//! Color Grading: tints the shadows, midtones and highlights, each by its
//! own hue and strength, without changing the pixel's brightness.

use rustyedit_core::{ColorGrading, Grade};

use super::{luma, smoothstep};
use crate::color::hsl_to_rgb;

/// Strongest tint, at saturation 100.
const MAX_STRENGTH: f32 = 0.3;

pub(crate) struct Grading {
    /// Brightness-neutral colour offsets, already scaled by strength.
    shadows: [f32; 3],
    midtones: [f32; 3],
    highlights: [f32; 3],
    /// Luma where the shadow and highlight ranges meet.
    pivot: f32,
}

impl Grading {
    pub(crate) fn new(grading: &ColorGrading) -> Option<Self> {
        if !grading.is_active() {
            return None;
        }
        Some(Self {
            shadows: tint(&grading.shadows),
            midtones: tint(&grading.midtones),
            highlights: tint(&grading.highlights),
            // Positive balance lowers the pivot, giving highlights more room.
            pivot: 0.5 - grading.balance / 100.0 * 0.3,
        })
    }

    /// Shadow, midtone and highlight tints, then the pivot: the layout the
    /// GPU shader reads.
    pub(crate) fn packed(&self) -> [f32; 10] {
        let [s, m, h] = [self.shadows, self.midtones, self.highlights];
        [
            s[0], s[1], s[2], m[0], m[1], m[2], h[0], h[1], h[2], self.pivot,
        ]
    }

    pub(crate) fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let l = luma(rgb);
        let shadow_w = smoothstep(self.pivot, 0.0, l);
        let highlight_w = smoothstep(self.pivot, 1.0, l);
        let mid_w = (1.0 - shadow_w - highlight_w).max(0.0);
        let mut out = rgb;
        for (c, value) in out.iter_mut().enumerate() {
            *value += shadow_w * self.shadows[c]
                + mid_w * self.midtones[c]
                + highlight_w * self.highlights[c];
        }
        out
    }
}

/// The colour `grade` pushes towards, minus its own luma so adding it
/// shifts hue without brightening or darkening.
fn tint(grade: &Grade) -> [f32; 3] {
    let (r, g, b) = hsl_to_rgb(grade.hue.rem_euclid(360.0), 1.0, 0.5);
    let offset = [r - 0.5, g - 0.5, b - 0.5];
    let shift = luma(offset);
    let strength = grade.saturation / 100.0 * MAX_STRENGTH;
    offset.map(|c| (c - shift) * strength)
}
