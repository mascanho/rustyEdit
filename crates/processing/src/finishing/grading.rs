//! Color Grading: tints the shadows, midtones and highlights, each by its
//! own hue and strength, and brightens or darkens each; the Offset wheel
//! does the same to everything. The offset is folded into the three ranges
//! (their weights always add up to one), so the shader needs no extra
//! input.

use rustyedit_core::{ColorGrading, Grade};

use super::{luma, smoothstep};
use crate::color::hsl_to_rgb;

/// Strongest tint, at saturation 100.
const MAX_STRENGTH: f32 = 0.3;
/// How far a range's luminance slider moves it, at its ends.
const MAX_LUMINANCE: f32 = 0.2;

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
        let global = wheel_offset(&grading.global);
        let with_global = |grade: &Grade| {
            let own = wheel_offset(grade);
            [own[0] + global[0], own[1] + global[1], own[2] + global[2]]
        };
        Some(Self {
            shadows: with_global(&grading.shadows),
            midtones: with_global(&grading.midtones),
            highlights: with_global(&grading.highlights),
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

/// What a wheel adds to a pixel: its tint, and its brightening.
fn wheel_offset(grade: &Grade) -> [f32; 3] {
    let tint = tint(grade);
    let lift = grade.luminance / 100.0 * MAX_LUMINANCE;
    tint.map(|c| c + lift)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn grade(hue: f32, saturation: f32, luminance: f32) -> Grade {
        Grade { hue, saturation, luminance }
    }

    #[test]
    fn a_luminance_only_wheel_brightens_just_its_range() {
        let grading = ColorGrading {
            shadows: grade(0.0, 0.0, 100.0),
            ..Default::default()
        };
        let g = Grading::new(&grading).unwrap();
        let dark = g.apply([0.05; 3]);
        let bright = g.apply([0.95; 3]);
        assert!(dark[0] > 0.15, "shadows lifted: {dark:?}");
        assert!((bright[0] - 0.95).abs() < 0.01, "highlights left: {bright:?}");
    }

    #[test]
    fn the_offset_wheel_moves_every_tone() {
        let grading = ColorGrading {
            global: grade(0.0, 0.0, -50.0),
            ..Default::default()
        };
        let g = Grading::new(&grading).unwrap();
        for v in [0.1, 0.5, 0.9] {
            let out = g.apply([v; 3]);
            assert!((out[0] - (v - 0.1)).abs() < 0.01, "{v}: {out:?}");
        }
    }

    #[test]
    fn a_hue_without_strength_or_brightness_changes_nothing() {
        let grading = ColorGrading {
            midtones: grade(200.0, 0.0, 0.0),
            ..Default::default()
        };
        assert!(Grading::new(&grading).is_none());
    }
}
