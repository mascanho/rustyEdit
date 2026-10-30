//! The Develop panel's sections, as groups of settings: what each one
//! resets, and switching one off so its edits stop applying (without
//! losing them), like the panel switches in Lightroom.

use crate::DevelopSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DevelopSection {
    Basic,
    ToneCurve,
    Color,
    ColorMixer,
    ColorGrading,
    Detail,
    Effects,
    // Last, so the on/off bits of the sections above keep their values.
    Retouch,
    Gradients,
    Crop,
    Lens,
    Frames,
    LensBlur,
}

impl DevelopSection {
    pub const ALL: [DevelopSection; 13] = [
        DevelopSection::Crop,
        DevelopSection::Retouch,
        DevelopSection::Gradients,
        DevelopSection::Basic,
        DevelopSection::ToneCurve,
        DevelopSection::Color,
        DevelopSection::ColorMixer,
        DevelopSection::ColorGrading,
        DevelopSection::Detail,
        DevelopSection::Effects,
        DevelopSection::LensBlur,
        DevelopSection::Lens,
        DevelopSection::Frames,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            DevelopSection::Crop => "Crop & Straighten",
            DevelopSection::Retouch => "Retouch",
            DevelopSection::Gradients => "Gradients",
            DevelopSection::Basic => "Basic",
            DevelopSection::ToneCurve => "Tone Curve",
            DevelopSection::Color => "Color",
            DevelopSection::ColorMixer => "Color Mixer",
            DevelopSection::ColorGrading => "Color Grading",
            DevelopSection::Detail => "Detail",
            DevelopSection::Effects => "Effects",
            DevelopSection::Lens => "Lens Corrections",
            DevelopSection::Frames => "Frames",
            DevelopSection::LensBlur => "Lens Blur",
        }
    }

    /// A stable name for saving the panel order.
    pub fn key(&self) -> &'static str {
        match self {
            DevelopSection::Basic => "basic",
            DevelopSection::ToneCurve => "tone_curve",
            DevelopSection::Color => "color",
            DevelopSection::ColorMixer => "color_mixer",
            DevelopSection::ColorGrading => "color_grading",
            DevelopSection::Detail => "detail",
            DevelopSection::Effects => "effects",
            DevelopSection::Crop => "crop",
            DevelopSection::Retouch => "retouch",
            DevelopSection::Gradients => "gradients",
            DevelopSection::Lens => "lens",
            DevelopSection::Frames => "frames",
            DevelopSection::LensBlur => "lens_blur",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|section| section.key() == key)
    }

    /// The panel order from a saved list of keys: the saved sections first,
    /// in their order, then any it doesn't mention (new ones, or a blank
    /// list) in the default order. Unknown and repeated keys are skipped.
    pub fn arrange(saved: &[String]) -> Vec<DevelopSection> {
        let mut order: Vec<DevelopSection> = Vec::new();
        for section in saved.iter().filter_map(|key| Self::from_key(key)) {
            if !order.contains(&section) {
                order.push(section);
            }
        }
        for section in Self::ALL {
            if !order.contains(&section) {
                order.push(section);
            }
        }
        order
    }

    /// `order` with `self` moved to where `onto` is: after it when moving
    /// down, before it when moving up.
    pub fn moved_onto(
        &self,
        onto: DevelopSection,
        order: &[DevelopSection],
    ) -> Vec<DevelopSection> {
        let mut order = order.to_vec();
        let (Some(from), Some(to)) = (
            order.iter().position(|s| s == self),
            order.iter().position(|s| *s == onto),
        ) else {
            return order;
        };
        let section = order.remove(from);
        order.insert(to.min(order.len()), section);
        order
    }

    pub fn bit(&self) -> u16 {
        1 << (*self as u16)
    }

    /// Puts this section's settings in `target` back to their defaults.
    pub fn reset(&self, target: &mut DevelopSettings) {
        self.copy(&DevelopSettings::default(), target);
    }

    /// Whether this section's settings are all at their defaults.
    pub fn is_default(&self, settings: &DevelopSettings) -> bool {
        let mut reset = *settings;
        self.reset(&mut reset);
        reset == *settings
    }

    /// The sections a preset's "look" is made of; geometry, retouching and
    /// local masks belong to the photo, not the look.
    pub const LOOK: [DevelopSection; 7] = [
        DevelopSection::Basic,
        DevelopSection::ToneCurve,
        DevelopSection::Color,
        DevelopSection::ColorMixer,
        DevelopSection::ColorGrading,
        DevelopSection::Detail,
        DevelopSection::Effects,
    ];

    /// Copies this section's settings from `from` into `to`.
    pub fn copy(&self, from: &DevelopSettings, to: &mut DevelopSettings) {
        match self {
            DevelopSection::Crop => to.crop = from.crop,
            DevelopSection::Retouch => to.spots = from.spots,
            DevelopSection::Gradients => to.gradients = from.gradients,
            DevelopSection::Basic => {
                to.exposure = from.exposure;
                to.contrast = from.contrast;
                to.highlights = from.highlights;
                to.shadows = from.shadows;
                to.whites = from.whites;
                to.blacks = from.blacks;
            }
            DevelopSection::ToneCurve => {
                to.curves = from.curves;
                to.parametric = from.parametric;
            }
            DevelopSection::Color => {
                to.temperature = from.temperature;
                to.tint = from.tint;
                to.vibrance = from.vibrance;
                to.saturation = from.saturation;
            }
            DevelopSection::ColorMixer => to.mixer = from.mixer,
            DevelopSection::ColorGrading => to.grading = from.grading,
            DevelopSection::Detail => {
                to.clarity = from.clarity;
                to.sharpening = from.sharpening;
                to.sharpen_radius = from.sharpen_radius;
                to.sharpen_masking = from.sharpen_masking;
                to.noise_luma = from.noise_luma;
                to.noise_color = from.noise_color;
                to.defringe_purple = from.defringe_purple;
                to.defringe_green = from.defringe_green;
            }
            DevelopSection::Frames => to.frame = from.frame,
            DevelopSection::LensBlur => to.lens_blur = from.lens_blur,
            DevelopSection::Lens => {
                to.lens_distortion = from.lens_distortion;
                to.lens_vignetting = from.lens_vignetting;
                to.lens_ca = from.lens_ca;
            }
            DevelopSection::Effects => {
                to.dehaze = from.dehaze;
                to.vignette = from.vignette;
                to.vignette_midpoint = from.vignette_midpoint;
                to.vignette_feather = from.vignette_feather;
                to.vignette_roundness = from.vignette_roundness;
                to.grain = from.grain;
                to.grain_size = from.grain_size;
            }
        }
    }
}

impl DevelopSettings {
    pub fn is_section_enabled(&self, section: DevelopSection) -> bool {
        self.disabled_sections & section.bit() == 0
    }

    pub fn set_section_enabled(&mut self, section: DevelopSection, enabled: bool) {
        if enabled {
            self.disabled_sections &= !section.bit();
        } else {
            self.disabled_sections |= section.bit();
        }
    }

    /// The settings as rendered: switched-off sections count as untouched.
    pub fn effective(&self) -> DevelopSettings {
        let mut effective = *self;
        for section in DevelopSection::ALL {
            if !self.is_section_enabled(section) {
                section.reset(&mut effective);
            }
        }
        effective.disabled_sections = 0;
        effective
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrange_keeps_saved_order_and_appends_the_rest() {
        let saved = vec![
            "effects".to_string(),
            "nonsense".into(),
            "basic".into(),
            "effects".into(),
        ];
        let order = DevelopSection::arrange(&saved);
        assert_eq!(
            &order[..2],
            &[DevelopSection::Effects, DevelopSection::Basic]
        );
        assert_eq!(order.len(), DevelopSection::ALL.len());
        assert_eq!(DevelopSection::arrange(&[]), DevelopSection::ALL.to_vec());
    }

    #[test]
    fn dropping_onto_a_section_takes_its_place() {
        use DevelopSection::*;
        let order = vec![Retouch, Basic, Color, Detail];
        // Down: lands after the target.
        assert_eq!(
            Retouch.moved_onto(Color, &order),
            vec![Basic, Color, Retouch, Detail]
        );
        // Up: lands before it.
        assert_eq!(
            Detail.moved_onto(Basic, &order),
            vec![Retouch, Detail, Basic, Color]
        );
    }

    #[test]
    fn switching_a_section_off_keeps_its_values_but_not_their_effect() {
        let mut settings = DevelopSettings {
            exposure: 30.0,
            clarity: 20.0,
            ..Default::default()
        };
        settings.set_section_enabled(DevelopSection::Basic, false);
        assert_eq!(settings.exposure, 30.0);
        let effective = settings.effective();
        assert_eq!(effective.exposure, 0.0);
        assert_eq!(effective.clarity, 20.0);

        settings.set_section_enabled(DevelopSection::Basic, true);
        assert_eq!(settings.effective().exposure, 30.0);
    }

    #[test]
    fn reset_touches_only_its_section() {
        let mut settings = DevelopSettings {
            vignette: -40.0,
            grain_size: 80.0,
            tint: 12.0,
            ..Default::default()
        };
        assert!(!DevelopSection::Effects.is_default(&settings));
        DevelopSection::Effects.reset(&mut settings);
        assert!(DevelopSection::Effects.is_default(&settings));
        assert_eq!(settings.vignette, 0.0);
        assert_eq!(settings.grain_size, 25.0);
        assert_eq!(settings.tint, 12.0);
    }
}
