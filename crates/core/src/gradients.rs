//! Gradient masks: local adjustments that fade across the photo, either
//! along a line (a graduated filter, e.g. to darken a sky) or out from an
//! ellipse (a radial filter, e.g. to lift a face).
//!
//! Like retouch spots, positions are fractions of the *original* photo
//! and sizes fractions of its long edge, so masks stay put when the photo
//! is rotated; [`Gradient::on_display`] gives them as drawn. They live in a
//! fixed-size array so `DevelopSettings` stays `Copy`.

use serde::{Deserialize, Serialize};

use crate::{retouch::to_display, DevelopSettings};

/// Most gradients a photo can have.
pub const MAX_GRADIENTS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GradientKind {
    /// Full strength at `start`, fading to nothing at `end`.
    #[default]
    Linear,
    /// Full strength inside the ellipse around `start`, fading out at its
    /// edge (or the reverse, inverted).
    Radial,
}

impl GradientKind {
    pub fn label(&self) -> &'static str {
        match self {
            GradientKind::Linear => "Linear",
            GradientKind::Radial => "Radial",
        }
    }
}

/// The adjustments a gradient applies where its mask is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocalProp {
    Exposure,
    Contrast,
    Highlights,
    Shadows,
    Temperature,
    Tint,
    Saturation,
    Dehaze,
    Clarity,
    Sharpening,
    /// How strongly the layer tints its area (0..=100).
    TintAmount,
    /// The tint's hue (0..=360 degrees).
    TintHue,
}

impl LocalProp {
    pub const ALL: [LocalProp; 12] = [
        LocalProp::Exposure,
        LocalProp::Contrast,
        LocalProp::Highlights,
        LocalProp::Shadows,
        LocalProp::Temperature,
        LocalProp::Tint,
        LocalProp::Saturation,
        LocalProp::Dehaze,
        LocalProp::Clarity,
        LocalProp::Sharpening,
        LocalProp::TintAmount,
        LocalProp::TintHue,
    ];

    /// The `(min, max)` the slider spans.
    pub fn range(&self) -> (f32, f32) {
        match self {
            LocalProp::TintAmount => (0.0, 100.0),
            LocalProp::TintHue => (0.0, 360.0),
            _ => (-100.0, 100.0),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            LocalProp::Exposure => "Exposure",
            LocalProp::Contrast => "Contrast",
            LocalProp::Highlights => "Highlights",
            LocalProp::Shadows => "Shadows",
            LocalProp::Temperature => "Temp",
            LocalProp::Tint => "Tint",
            LocalProp::Saturation => "Saturation",
            LocalProp::Dehaze => "Dehaze",
            LocalProp::Clarity => "Clarity",
            LocalProp::Sharpening => "Sharpening",
            LocalProp::TintAmount => "Colour tint",
            LocalProp::TintHue => "Tint hue",
        }
    }
}

/// Each -100..=100 (the tint 0..=360 / 0..=100), 0 meaning no change.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalAdjust {
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub temperature: f32,
    pub tint: f32,
    pub saturation: f32,
    pub dehaze: f32,
    pub clarity: f32,
    /// Local sharpening; negative values soften.
    pub sharpening: f32,
    /// 0..=100: how strongly the layer tints its area.
    pub tint_amount: f32,
    /// 0..=360: the colour of that tint.
    pub tint_hue: f32,
}

impl LocalAdjust {
    pub fn get(&self, prop: LocalProp) -> f32 {
        match prop {
            LocalProp::Exposure => self.exposure,
            LocalProp::Contrast => self.contrast,
            LocalProp::Highlights => self.highlights,
            LocalProp::Shadows => self.shadows,
            LocalProp::Temperature => self.temperature,
            LocalProp::Tint => self.tint,
            LocalProp::Saturation => self.saturation,
            LocalProp::Dehaze => self.dehaze,
            LocalProp::Clarity => self.clarity,
            LocalProp::Sharpening => self.sharpening,
            LocalProp::TintAmount => self.tint_amount,
            LocalProp::TintHue => self.tint_hue,
        }
    }

    pub fn set(&mut self, prop: LocalProp, value: f32) {
        let (min, max) = prop.range();
        let v = value.clamp(min, max);
        match prop {
            LocalProp::Exposure => self.exposure = v,
            LocalProp::Contrast => self.contrast = v,
            LocalProp::Highlights => self.highlights = v,
            LocalProp::Shadows => self.shadows = v,
            LocalProp::Temperature => self.temperature = v,
            LocalProp::Tint => self.tint = v,
            LocalProp::Saturation => self.saturation = v,
            LocalProp::Dehaze => self.dehaze = v,
            LocalProp::Clarity => self.clarity = v,
            LocalProp::Sharpening => self.sharpening = v,
            LocalProp::TintAmount => self.tint_amount = v,
            LocalProp::TintHue => self.tint_hue = v,
        }
    }

    /// Whether the layer changes nothing. The tint's hue alone does not.
    pub fn is_neutral(&self) -> bool {
        Self {
            tint_hue: 0.0,
            ..*self
        } == Self::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Gradient {
    pub kind: GradientKind,
    /// Linear: where the effect is full. Radial: the centre.
    pub start: (f32, f32),
    /// Linear: where the effect has faded out. Radial: unused.
    pub end: (f32, f32),
    /// Radial: the ellipse's horizontal and vertical radii, as fractions
    /// of the long edge (on the original photo).
    pub radius: (f32, f32),
    /// Radial: 0..=1, how much of the radius fades out.
    pub feather: f32,
    /// Apply outside the mask instead of inside it.
    pub invert: bool,
    pub adjust: LocalAdjust,
    /// The layer's eye is closed: kept, but not applied.
    pub hidden: bool,
    /// The layer's opacity, 0..=100: how much of its effect is applied.
    pub opacity: f32,
}

impl Default for Gradient {
    fn default() -> Self {
        Self {
            kind: GradientKind::default(),
            start: (0.0, 0.0),
            end: (0.0, 0.0),
            radius: (0.0, 0.0),
            feather: 0.0,
            invert: false,
            adjust: LocalAdjust::default(),
            hidden: false,
            opacity: 100.0,
        }
    }
}

impl Gradient {
    /// The gradient on the photo as shown (rotated and flipped): points as
    /// fractions of the displayed photo, radii as fractions of its long
    /// edge, swapped for quarter turns.
    pub fn on_display(&self, settings: &DevelopSettings) -> Gradient {
        let quarters = ((settings.rotation_degrees / 90.0).round() as i32).rem_euclid(4);
        Gradient {
            start: to_display(self.start, settings),
            end: to_display(self.end, settings),
            radius: if quarters % 2 == 1 {
                (self.radius.1, self.radius.0)
            } else {
                self.radius
            },
            ..*self
        }
    }

    /// Mask strength (0..=1) at pixel (`x`, `y`) of a `width` × `height`
    /// image. `self` must already be [`Gradient::on_display`].
    pub fn mask_at(&self, x: f32, y: f32, width: f32, height: f32) -> f32 {
        let strength = match self.kind {
            GradientKind::Linear => {
                let (ax, ay) = (self.start.0 * width, self.start.1 * height);
                let (bx, by) = (self.end.0 * width, self.end.1 * height);
                let (dx, dy) = (bx - ax, by - ay);
                let length = (dx * dx + dy * dy).max(1.0);
                let t = ((x - ax) * dx + (y - ay) * dy) / length;
                1.0 - smoothstep(t.clamp(0.0, 1.0))
            }
            GradientKind::Radial => {
                let long = width.max(height);
                let (cx, cy) = (self.start.0 * width, self.start.1 * height);
                let rx = (self.radius.0 * long).max(1.0);
                let ry = (self.radius.1 * long).max(1.0);
                let d = (((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2)).sqrt();
                let inner = 1.0 - self.feather.clamp(0.0, 1.0);
                if d <= inner {
                    1.0
                } else if d >= 1.0 {
                    0.0
                } else {
                    1.0 - smoothstep((d - inner) / (1.0 - inner).max(1e-3))
                }
            }
        };
        let strength = if self.invert {
            1.0 - strength
        } else {
            strength
        };
        strength * (self.opacity / 100.0).clamp(0.0, 1.0)
    }

    /// Whether this layer changes the picture at all.
    pub fn is_visible(&self) -> bool {
        !self.hidden && self.opacity > 0.0 && !self.adjust.is_neutral()
    }
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// A photo's gradients, oldest first.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Gradients {
    items: [Gradient; MAX_GRADIENTS],
    len: u8,
}

impl Default for Gradients {
    fn default() -> Self {
        Self {
            items: [Gradient::default(); MAX_GRADIENTS],
            len: 0,
        }
    }
}

impl Gradients {
    pub fn as_slice(&self) -> &[Gradient] {
        &self.items[..(self.len as usize).min(MAX_GRADIENTS)]
    }

    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut Gradient> {
        let len = self.len();
        self.items[..len].get_mut(index)
    }

    /// Adds a gradient and returns its index, or `None` when full.
    pub fn push(&mut self, gradient: Gradient) -> Option<usize> {
        let len = self.len();
        if len >= MAX_GRADIENTS {
            return None;
        }
        self.items[len] = gradient;
        self.len += 1;
        Some(len)
    }

    pub fn remove(&mut self, index: usize) {
        let len = self.len();
        if index >= len {
            return;
        }
        self.items.copy_within(index + 1..len, index);
        self.items[len - 1] = Gradient::default();
        self.len -= 1;
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Whether any gradient actually changes something.
    pub fn is_active(&self) -> bool {
        self.as_slice().iter().any(Gradient::is_visible)
    }

    /// Moves a layer one place later (`1`, towards the front) or earlier
    /// (`-1`) in the order they are applied. Returns its new index.
    pub fn move_by(&mut self, index: usize, delta: i32) -> Option<usize> {
        let target = index as i64 + delta as i64;
        if index >= self.len() || target < 0 || target as usize >= self.len() {
            return None;
        }
        self.items.swap(index, target as usize);
        Some(target as usize)
    }

    /// Copies a layer to just in front of itself. Returns the copy's index,
    /// or `None` when the list is full.
    pub fn duplicate(&mut self, index: usize) -> Option<usize> {
        let len = self.len();
        if index >= len || len >= MAX_GRADIENTS {
            return None;
        }
        self.items.copy_within(index..len, index + 1);
        self.len += 1;
        Some(index + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_mask_fades_from_start_to_end() {
        let g = Gradient {
            kind: GradientKind::Linear,
            start: (0.5, 0.0),
            end: (0.5, 1.0),
            ..Default::default()
        };
        assert_eq!(g.mask_at(50.0, 0.0, 100.0, 100.0), 1.0);
        assert!((g.mask_at(50.0, 50.0, 100.0, 100.0) - 0.5).abs() < 1e-3);
        assert_eq!(g.mask_at(50.0, 100.0, 100.0, 100.0), 0.0);
        let inverted = Gradient { invert: true, ..g };
        assert_eq!(inverted.mask_at(50.0, 0.0, 100.0, 100.0), 0.0);
    }

    #[test]
    fn radial_mask_is_full_inside_and_off_outside() {
        let g = Gradient {
            kind: GradientKind::Radial,
            start: (0.5, 0.5),
            radius: (0.2, 0.2),
            feather: 0.5,
            ..Default::default()
        };
        assert_eq!(g.mask_at(50.0, 50.0, 100.0, 100.0), 1.0);
        assert_eq!(g.mask_at(95.0, 50.0, 100.0, 100.0), 0.0);
        let edge = g.mask_at(65.0, 50.0, 100.0, 100.0);
        assert!(edge > 0.0 && edge < 1.0);
    }

    #[test]
    fn layers_can_be_hidden_faded_reordered_and_duplicated() {
        let mut layers = Gradients::default();
        let mut shadows = Gradient::default();
        shadows.adjust.exposure = 20.0;
        let mut sky = Gradient::default();
        sky.adjust.exposure = -30.0;
        layers.push(shadows);
        layers.push(sky);

        // Hidden or fully faded layers don't count.
        assert!(layers.is_active());
        for ix in 0..2 {
            layers.get_mut(ix).unwrap().hidden = true;
        }
        assert!(!layers.is_active());
        layers.get_mut(0).unwrap().hidden = false;
        layers.get_mut(0).unwrap().opacity = 0.0;
        assert!(!layers.is_active());

        // Reordering swaps; the ends stay put.
        assert_eq!(layers.move_by(0, 1), Some(1));
        assert_eq!(layers.as_slice()[1].adjust.exposure, 20.0);
        assert_eq!(layers.move_by(1, 1), None);

        // Duplicating puts the copy in front of the original.
        assert_eq!(layers.duplicate(0), Some(1));
        assert_eq!(layers.len(), 3);
        assert_eq!(layers.as_slice()[1], layers.as_slice()[0]);
        while layers.duplicate(0).is_some() {}
        assert_eq!(layers.len(), MAX_GRADIENTS);
    }

    #[test]
    fn opacity_scales_the_mask() {
        let g = Gradient {
            kind: GradientKind::Radial,
            start: (0.5, 0.5),
            radius: (0.2, 0.2),
            opacity: 40.0,
            ..Default::default()
        };
        assert!((g.mask_at(50.0, 50.0, 100.0, 100.0) - 0.4).abs() < 1e-4);
    }

    #[test]
    fn quarter_turn_swaps_radial_radii() {
        let settings = DevelopSettings {
            rotation_degrees: 90.0,
            ..Default::default()
        };
        let g = Gradient {
            kind: GradientKind::Radial,
            radius: (0.3, 0.1),
            ..Default::default()
        };
        assert_eq!(g.on_display(&settings).radius, (0.1, 0.3));
    }
}
