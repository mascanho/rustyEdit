//! Lens blur: a shallow-depth-of-field look. What's in focus stays sharp,
//! the rest melts into a lens-like blur whose out-of-focus highlights
//! bloom into bokeh discs, unlike a flat Gaussian softening.
//!
//! What's in focus comes from a mask: by default an oval around the
//! middle of the photo, or any mask layer the user marks as the focus
//! (`Gradient::lens_focus`): a radial around a face, a linear for a
//! tilt-shift look, a painted brush, a brightness / colour range. Blur
//! grows with distance from the focus, at a rate set by Falloff.
//!
//! Sizes are fractions of the photo's long edge, so a preview and the
//! full-size export look alike.

use serde::{Deserialize, Serialize};

use crate::{Gradient, GradientKind, Gradients};

/// The shape out-of-focus highlights take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BokehShape {
    /// Round discs, a modern wide-open lens.
    #[default]
    Circle,
    /// Six-sided, an aperture stopped down a little.
    Blades,
    /// Discs squeezed into cat's eyes towards the edges, like many fast
    /// lenses wide open.
    CatEye,
    /// Bright-rimmed "soap bubbles", like old Trioplan and Meyer lenses.
    Bubble,
}

impl BokehShape {
    pub const ALL: [BokehShape; 4] = [
        BokehShape::Circle,
        BokehShape::Blades,
        BokehShape::CatEye,
        BokehShape::Bubble,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            BokehShape::Circle => "Circle",
            BokehShape::Blades => "Blades",
            BokehShape::CatEye => "Cat's eye",
            BokehShape::Bubble => "Bubble",
        }
    }

    pub fn index(&self) -> u32 {
        *self as u32
    }
}

/// The largest blur, at Amount 100, as a fraction of the long edge.
pub const MAX_BLUR: f32 = 0.022;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LensBlur {
    /// 0..=100: how big the blur gets far from the focus.
    pub amount: f32,
    /// 0..=100: how much bright highlights bloom into bokeh.
    pub boost: f32,
    /// 0..=100: 0 turns from sharp to blurred quickly at the focus's edge,
    /// 100 blurs gradually all the way out.
    pub falloff: f32,
    pub shape: BokehShape,
    /// Blur the focus area instead of everything else.
    pub invert: bool,
}

impl Default for LensBlur {
    fn default() -> Self {
        Self {
            amount: 0.0,
            boost: 35.0,
            falloff: 50.0,
            shape: BokehShape::Circle,
            invert: false,
        }
    }
}

impl LensBlur {
    pub fn is_active(&self) -> bool {
        self.amount >= 0.5
    }

    /// The biggest blur radius in pixels on a `width` x `height` image.
    pub fn max_radius(&self, width: u32, height: u32) -> f32 {
        if !self.is_active() {
            return 0.0;
        }
        self.amount.clamp(0.0, 100.0) / 100.0 * MAX_BLUR * width.max(height) as f32
    }

    /// How blurred a pixel is (0 sharp ..= 1 the full Amount), from how
    /// much it is in focus (the focus mask, 0..=1).
    pub fn blur_at(&self, focus: f32) -> f32 {
        let away = if self.invert { focus } else { 1.0 - focus };
        let away = away.clamp(0.0, 1.0);
        // Falloff 0: a quick turn just past the focus's edge; 100: as the
        // mask fades.
        let steep = {
            let t = ((away - 0.15) / 0.35).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        let f = (self.falloff / 100.0).clamp(0.0, 1.0);
        steep + (away - steep) * f
    }

    /// The default focus: an oval around the middle of the photo.
    pub fn centre_focus() -> Gradient {
        Gradient {
            kind: GradientKind::Radial,
            start: (0.5, 0.5),
            end: (0.5, 0.5),
            radius: (0.22, 0.26),
            feather: 0.65,
            ..Gradient::default()
        }
    }

    /// The mask that's in focus: the layer marked as the focus, or the
    /// default oval around the middle.
    pub fn focus(gradients: &Gradients) -> Gradient {
        gradients
            .as_slice()
            .iter()
            .find(|g| g.lens_focus && !g.hidden)
            .copied()
            .unwrap_or_else(Self::centre_focus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_grows_away_from_the_focus() {
        let lens = LensBlur {
            amount: 60.0,
            ..LensBlur::default()
        };
        assert_eq!(lens.blur_at(1.0), 0.0);
        assert!((lens.blur_at(0.0) - 1.0).abs() < 1e-5);
        assert!(lens.blur_at(0.3) > lens.blur_at(0.6));
        let inverted = LensBlur { invert: true, ..lens };
        assert_eq!(inverted.blur_at(0.0), 0.0);
        assert!(lens.max_radius(4000, 3000) > 40.0);
        assert_eq!(LensBlur::default().max_radius(4000, 3000), 0.0);
    }
}
