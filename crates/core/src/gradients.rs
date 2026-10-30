//! Gradient masks: local adjustments that fade across the photo, either
//! along a line (a graduated filter, e.g. to darken a sky) or out from an
//! ellipse (a radial filter, e.g. to lift a face).
//!
//! Two more kinds work on the picture itself: a **brush**, painted on in
//! dabs, and a **range** mask that selects by brightness and colour. Any
//! layer can also be refined by brightness and colour ranges, so e.g. a
//! radial can lift only the shadows inside it.
//!
//! Like retouch spots, positions are fractions of the *original* photo
//! and sizes fractions of its long edge, so masks stay put when the photo
//! is rotated; [`Gradient::on_display`] gives them as drawn. They live in a
//! fixed-size array so `DevelopSettings` stays `Copy`.

use serde::{Deserialize, Serialize};

use crate::{grading::ColorGrading, retouch::to_display, DevelopSettings};

/// Most gradients a photo can have.
pub const MAX_GRADIENTS: usize = 8;
/// Most dabs a brush layer holds; a stroke is a run of overlapping dabs.
pub const MAX_DABS: usize = 48;
/// Dabs along a stroke are this far apart, as a fraction of their radius.
pub const DAB_SPACING: f32 = 0.5;

/// One dab of a brush layer.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Dab {
    /// Centre, as a fraction of the original photo.
    pub x: f32,
    pub y: f32,
    /// Radius, as a fraction of the photo's long edge.
    pub radius: f32,
    /// Takes the mask away instead of adding to it.
    pub erase: bool,
}

/// A dab in six bytes (a position and radius as 16-bit fractions, the top
/// bit of the radius marking an eraser): `DevelopSettings` is `Copy` and
/// copied everywhere, and a layer's worth of full-size dabs made it eight
/// times as big, enough to overflow a worker thread's stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PackedDab([u16; 3]);

/// Largest dab radius that can be stored (a fraction of the long edge).
const MAX_DAB_RADIUS: f32 = 0.5;

impl PackedDab {
    fn pack(d: Dab) -> Self {
        let unit = |v: f32| (v.clamp(0.0, 1.0) * 65535.0).round() as u16;
        let radius = ((d.radius / MAX_DAB_RADIUS).clamp(0.0, 1.0) * 32767.0).round() as u16;
        PackedDab([unit(d.x), unit(d.y), radius | if d.erase { 0x8000 } else { 0 }])
    }

    fn unpack(self) -> Dab {
        Dab {
            x: self.0[0] as f32 / 65535.0,
            y: self.0[1] as f32 / 65535.0,
            radius: (self.0[2] & 0x7fff) as f32 / 32767.0 * MAX_DAB_RADIUS,
            erase: self.0[2] & 0x8000 != 0,
        }
    }
}

/// `Serialize` / `Deserialize` for the dabs (as plain lists of [`Dab`]).
mod dab_array {
    use super::{Dab, PackedDab, MAX_DABS};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(dabs: &[PackedDab; MAX_DABS], s: S) -> Result<S::Ok, S::Error> {
        // Only the dabs in use are written; the count is saved separately
        // and trims the rest on load.
        let used = dabs
            .iter()
            .rposition(|d| *d != PackedDab::default())
            .map_or(0, |i| i + 1);
        let list: Vec<Dab> = dabs[..used].iter().map(|d| d.unpack()).collect();
        list.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[PackedDab; MAX_DABS], D::Error> {
        let list = Vec::<Dab>::deserialize(d)?;
        let mut out = [PackedDab::default(); MAX_DABS];
        for (slot, dab) in out.iter_mut().zip(list) {
            *slot = PackedDab::pack(dab);
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GradientKind {
    /// Full strength at `start`, fading to nothing at `end`.
    #[default]
    Linear,
    /// Full strength inside the ellipse around `start`, fading out at its
    /// edge (or the reverse, inverted).
    Radial,
    /// Painted on with the brush: the union of its dabs.
    Brush,
    /// Everywhere, narrowed down by the layer's brightness and colour
    /// ranges.
    Range,
}

impl GradientKind {
    pub fn label(&self) -> &'static str {
        match self {
            GradientKind::Linear => "Linear",
            GradientKind::Radial => "Radial",
            GradientKind::Brush => "Brush",
            GradientKind::Range => "Range",
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
    /// Softens towards a wide blur: lens blur for a background.
    Blur,
}

impl LocalProp {
    pub const ALL: [LocalProp; 11] = [
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
        LocalProp::Blur,
    ];

    /// The slider's value is the stored one times this (see
    /// `Adjustment::display_sign`): a layer's tint reads like the global one.
    pub fn display_sign(&self) -> f32 {
        match self {
            LocalProp::Tint => -1.0,
            _ => 1.0,
        }
    }

    /// The `(min, max)` the slider spans.
    pub fn range(&self) -> (f32, f32) {
        match self {
            LocalProp::Blur => (0.0, 100.0),
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
            LocalProp::Blur => "Blur",
        }
    }
}

/// Each -100..=100, 0 meaning no change; plus the layer's colour grading.
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
    /// 0..=100: how far the area is softened into a wide blur.
    pub blur: f32,
    /// Colour grading of the layer's area: a tint for the shadows,
    /// midtones and highlights.
    pub grading: ColorGrading,
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
            LocalProp::Blur => self.blur,
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
            LocalProp::Blur => self.blur = v,
        }
    }

    /// Whether the layer changes nothing. A grading hue with no saturation
    /// does not.
    pub fn is_neutral(&self) -> bool {
        let mut plain = *self;
        if !plain.grading.is_active() {
            plain.grading = ColorGrading::default();
        }
        plain == Self::default()
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
    /// Brush: the dabs painted so far, and how many of them there are.
    #[serde(with = "dab_array")]
    pub dabs: [PackedDab; MAX_DABS],
    pub dab_count: u8,
    /// Brightness range the layer applies to (0..=1 each), and how softly
    /// its edges fade. `(0, 1)` is everything.
    pub luma_range: (f32, f32),
    pub luma_feather: f32,
    /// Colour range: pixels within `hue_width` degrees of `hue`.
    pub hue_on: bool,
    pub hue: f32,
    pub hue_width: f32,
    pub hue_feather: f32,
    /// This layer's mask is where Lens Blur keeps the photo in focus.
    pub lens_focus: bool,
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
            dabs: [PackedDab::default(); MAX_DABS],
            dab_count: 0,
            luma_range: (0.0, 1.0),
            luma_feather: 0.15,
            hue_on: false,
            hue: 0.0,
            hue_width: 30.0,
            hue_feather: 0.5,
            lens_focus: false,
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
            dabs: {
                let mut dabs = self.dabs;
                for packed in &mut dabs[..self.dab_count()] {
                    let mut dab = packed.unpack();
                    (dab.x, dab.y) = to_display((dab.x, dab.y), settings);
                    *packed = PackedDab::pack(dab);
                }
                dabs
            },
            ..*self
        }
    }

    /// The dabs in use.
    pub fn dab_count(&self) -> usize {
        (self.dab_count as usize).min(MAX_DABS)
    }

    pub fn dabs(&self) -> impl Iterator<Item = Dab> + '_ {
        self.dabs[..self.dab_count()].iter().map(|d| d.unpack())
    }

    /// Adds a dab; `false` when the brush layer is full.
    pub fn add_dab(&mut self, dab: Dab) -> bool {
        let n = self.dab_count();
        if n >= MAX_DABS {
            return false;
        }
        self.dabs[n] = PackedDab::pack(dab);
        self.dab_count += 1;
        true
    }

    /// Takes the last stroke's worth back: removes dabs from the end.
    pub fn pop_dab(&mut self) {
        let n = self.dab_count();
        if n > 0 {
            self.dabs[n - 1] = PackedDab::default();
            self.dab_count -= 1;
        }
    }

    /// Whether the brightness or colour range narrows the layer.
    pub fn has_range(&self) -> bool {
        self.hue_on || self.luma_range.0 > 0.0 || self.luma_range.1 < 1.0
    }

    /// How much the range lets through (0..=1) for a pixel of colour
    /// `rgb`: 1 when the layer has no range.
    pub fn range_at(&self, rgb: [f32; 3]) -> f32 {
        let mut weight = 1.0;
        let (lo, hi) = self.luma_range;
        if lo > 0.0 || hi < 1.0 {
            let l = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
            let f = (self.luma_feather * 0.5).max(0.002);
            let rise = if lo > 0.0 { smoothstep(((l - (lo - f)) / (f * 2.0)).clamp(0.0, 1.0)) } else { 1.0 };
            let fall = if hi < 1.0 { 1.0 - smoothstep(((l - (hi - f)) / (f * 2.0)).clamp(0.0, 1.0)) } else { 1.0 };
            weight *= rise * fall;
        }
        if self.hue_on {
            let (max, min) = (
                rgb[0].max(rgb[1]).max(rgb[2]),
                rgb[0].min(rgb[1]).min(rgb[2]),
            );
            let d = max - min;
            if d < 1e-5 || max <= 0.0 {
                return 0.0;
            }
            let h = if max == rgb[0] {
                ((rgb[1] - rgb[2]) / d).rem_euclid(6.0)
            } else if max == rgb[1] {
                (rgb[2] - rgb[0]) / d + 2.0
            } else {
                (rgb[0] - rgb[1]) / d + 4.0
            } * 60.0;
            let mut delta = (h - self.hue).rem_euclid(360.0);
            if delta > 180.0 {
                delta = 360.0 - delta;
            }
            let width = self.hue_width.max(1.0);
            let inner = width * (1.0 - self.hue_feather.clamp(0.0, 1.0));
            let w_hue = if delta <= inner {
                1.0
            } else if delta >= width {
                0.0
            } else {
                1.0 - smoothstep((delta - inner) / (width - inner).max(1e-3))
            };
            // Greys have no real hue.
            let w_sat = ((d / max) / 0.12).clamp(0.0, 1.0);
            weight *= w_hue * w_sat;
        }
        weight
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
            GradientKind::Brush => {
                let long = width.max(height);
                let mut strength = 0.0f32;
                for dab in self.dabs() {
                    let r = (dab.radius * long).max(1.0);
                    let d = ((x - dab.x * width).powi(2) + (y - dab.y * height).powi(2)).sqrt() / r;
                    let inner = 1.0 - self.feather.clamp(0.0, 1.0);
                    let f = if d <= inner {
                        1.0
                    } else if d >= 1.0 {
                        0.0
                    } else {
                        1.0 - smoothstep((d - inner) / (1.0 - inner).max(1e-3))
                    };
                    strength = if dab.erase {
                        strength * (1.0 - f)
                    } else {
                        strength + f - strength * f
                    };
                }
                strength
            }
            GradientKind::Range => 1.0,
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

    /// Whether any visible layer blurs.
    pub fn any_blur(&self) -> bool {
        self.as_slice()
            .iter()
            .any(|g| g.is_visible() && g.adjust.blur != 0.0)
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
    fn a_layer_with_only_a_grading_hue_is_neutral() {
        let mut layer = Gradient::default();
        layer.adjust.grading.shadows.hue = 200.0;
        assert!(!layer.is_visible());
        layer.adjust.grading.shadows.saturation = 30.0;
        assert!(layer.is_visible());
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

#[cfg(test)]
mod brush_and_range_tests {
    use super::*;

    #[test]
    fn a_brush_covers_its_dabs_and_erasing_takes_it_away() {
        let mut g = Gradient {
            kind: GradientKind::Brush,
            feather: 0.0,
            ..Default::default()
        };
        assert_eq!(g.mask_at(50.0, 50.0, 100.0, 100.0), 0.0);
        assert!(g.add_dab(Dab { x: 0.5, y: 0.5, radius: 0.1, erase: false }));
        assert_eq!(g.mask_at(50.0, 50.0, 100.0, 100.0), 1.0);
        assert_eq!(g.mask_at(80.0, 50.0, 100.0, 100.0), 0.0);
        g.add_dab(Dab { x: 0.5, y: 0.5, radius: 0.05, erase: true });
        assert_eq!(g.mask_at(50.0, 50.0, 100.0, 100.0), 0.0);
        assert_eq!(g.mask_at(57.0, 50.0, 100.0, 100.0), 1.0);
        g.pop_dab();
        assert_eq!(g.mask_at(50.0, 50.0, 100.0, 100.0), 1.0);
        while g.add_dab(Dab::default()) {}
        assert_eq!(g.dab_count(), MAX_DABS);
    }

    #[test]
    fn dabs_survive_a_save_and_load() {
        let mut g = Gradient { kind: GradientKind::Brush, ..Default::default() };
        g.add_dab(Dab { x: 0.25, y: 0.75, radius: 0.05, erase: false });
        g.add_dab(Dab { x: 0.5, y: 0.5, radius: 0.1, erase: true });
        let json = serde_json::to_string(&g).unwrap();
        let back: Gradient = serde_json::from_str(&json).unwrap();
        assert_eq!(back.dab_count(), 2);
        let dab = back.dabs().next().unwrap();
        assert!((dab.x - 0.25).abs() < 1e-3 && (dab.y - 0.75).abs() < 1e-3 && !dab.erase);
        assert!(back.dabs().nth(1).unwrap().erase);
        // Old saves without any of the new fields still load.
        let old: Gradient = serde_json::from_str(r#"{"kind":"Linear"}"#).unwrap();
        assert_eq!(old.dab_count(), 0);
        assert!(!old.has_range());
    }

    #[test]
    fn luminance_range_keeps_only_the_chosen_tones() {
        let g = Gradient {
            kind: GradientKind::Range,
            luma_range: (0.0, 0.4),
            luma_feather: 0.1,
            ..Default::default()
        };
        assert!(g.range_at([0.1; 3]) > 0.99);
        assert!(g.range_at([0.9; 3]) < 0.01);
        let edge = g.range_at([0.4; 3]);
        assert!(edge > 0.2 && edge < 0.8);
    }

    #[test]
    fn colour_range_picks_a_hue_and_ignores_greys() {
        let g = Gradient {
            hue_on: true,
            hue: 120.0,
            hue_width: 30.0,
            ..Default::default()
        };
        assert!(g.range_at([0.1, 0.8, 0.1]) > 0.99, "green is in");
        assert!(g.range_at([0.8, 0.1, 0.1]) < 0.01, "red is out");
        assert_eq!(g.range_at([0.5; 3]), 0.0, "grey has no hue");
    }
}

#[cfg(test)]
mod size_tests {
    #[test]
    fn settings_stay_small_enough_to_copy_freely() {
        // Copied around on worker threads with small stacks.
        assert!(std::mem::size_of::<crate::DevelopSettings>() < 6000);
    }
}
