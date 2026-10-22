//! Develop presets: named looks (tone, colour and detail settings) that
//! can be applied to any photo. A good set ships with the app; the rest are
//! saved by the user and stored app-wide. Presets can be favourited,
//! categorised, renamed, duplicated, reordered, exported and imported, and
//! applied at partial strength or to just some sections of the look.

use serde::{Deserialize, Serialize};

use crate::{ColorBand, DevelopSection, DevelopSettings, ToneCurve, ToneZone};

/// Categories presets are filed under, in the order they are shown. Saved
/// presets default to "Custom".
pub const PRESET_CATEGORIES: [&str; 10] = [
    "Basic",
    "B&W",
    "Portrait",
    "Landscape",
    "Film",
    "Cinematic",
    "Color Grade",
    "Curves",
    "Creative",
    "Custom",
];

/// A named look. Only its tone, colour and detail values are used when
/// applying it; a photo's rotation and flips are its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DevelopPreset {
    pub name: String,
    pub settings: DevelopSettings,
    /// One of [`PRESET_CATEGORIES`] (anything else is treated as Custom).
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub favorite: bool,
    /// Free text shown with the preset, e.g. the recipe it came from.
    #[serde(default)]
    pub notes: String,
}

impl DevelopPreset {
    pub fn category_label(&self) -> &str {
        PRESET_CATEGORIES
            .iter()
            .copied()
            .find(|c| c.eq_ignore_ascii_case(&self.category))
            .unwrap_or("Custom")
    }
}

/// The user's saved presets, in the order they were created.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Presets {
    pub user: Vec<DevelopPreset>,
    /// Names of built-in presets the user has starred.
    pub builtin_favorites: Vec<String>,
}

impl Presets {
    /// Saves the look of `settings` (only the `sections` listed) as a preset
    /// called `name` in `category`, replacing any preset of the same name
    /// (case-insensitively). Returns the name used.
    pub fn save(&mut self, name: &str, settings: &DevelopSettings) -> String {
        self.save_in(name, "Custom", settings, &DevelopSection::LOOK)
    }

    pub fn save_in(
        &mut self,
        name: &str,
        category: &str,
        settings: &DevelopSettings,
        sections: &[DevelopSection],
    ) -> String {
        let name = match name.trim() {
            "" => self.next_default_name(),
            trimmed => trimmed.to_string(),
        };
        let mut look = DevelopSettings::default();
        let effective = settings.effective();
        for section in sections {
            section.copy(&effective, &mut look);
        }
        let mut preset = DevelopPreset {
            name: name.clone(),
            settings: look,
            category: category.to_string(),
            favorite: false,
            notes: String::new(),
        };
        match self
            .user
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(&name))
        {
            Some(existing) => {
                preset.favorite = existing.favorite;
                preset.notes = std::mem::take(&mut existing.notes);
                *existing = preset;
            }
            None => self.user.push(preset),
        }
        name
    }

    /// Adds `preset`, replacing a saved one of the same name.
    pub fn upsert(&mut self, preset: DevelopPreset) {
        match self
            .user
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(&preset.name))
        {
            Some(existing) => {
                let favorite = existing.favorite;
                *existing = DevelopPreset { favorite, ..preset };
            }
            None => self.user.push(preset),
        }
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.user.len() {
            self.user.remove(index);
        }
    }

    /// Renames a saved preset; fails if the name is blank or another preset
    /// already has it.
    pub fn rename(&mut self, index: usize, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty()
            || self
                .user
                .iter()
                .enumerate()
                .any(|(i, p)| i != index && p.name.eq_ignore_ascii_case(name))
        {
            return false;
        }
        match self.user.get_mut(index) {
            Some(preset) => {
                preset.name = name.to_string();
                true
            }
            None => false,
        }
    }

    /// Copies a saved preset right after itself as "<name> copy".
    pub fn duplicate(&mut self, index: usize) {
        let Some(original) = self.user.get(index).cloned() else {
            return;
        };
        let name = (1..)
            .map(|n| {
                if n == 1 {
                    format!("{} copy", original.name)
                } else {
                    format!("{} copy {n}", original.name)
                }
            })
            .find(|name| !self.user.iter().any(|p| p.name.eq_ignore_ascii_case(name)))
            .expect("an unused name always exists");
        self.user.insert(
            index + 1,
            DevelopPreset {
                name,
                favorite: false,
                ..original
            },
        );
    }

    pub fn toggle_favorite(&mut self, index: usize) {
        if let Some(preset) = self.user.get_mut(index) {
            preset.favorite = !preset.favorite;
        }
    }

    pub fn toggle_builtin_favorite(&mut self, name: &str) {
        match self.builtin_favorites.iter().position(|n| n == name) {
            Some(ix) => {
                self.builtin_favorites.remove(ix);
            }
            None => self.builtin_favorites.push(name.to_string()),
        }
    }

    pub fn is_builtin_favorite(&self, name: &str) -> bool {
        self.builtin_favorites.iter().any(|n| n == name)
    }

    /// Moves a saved preset one place up (`-1`) or down (`1`).
    pub fn move_by(&mut self, index: usize, delta: i32) {
        let target = index as i64 + delta as i64;
        if index < self.user.len() && target >= 0 && (target as usize) < self.user.len() {
            self.user.swap(index, target as usize);
        }
    }

    /// Replaces a saved preset's look with `settings`' (the "update" action).
    pub fn overwrite(
        &mut self,
        index: usize,
        settings: &DevelopSettings,
        sections: &[DevelopSection],
    ) {
        if let Some(preset) = self.user.get_mut(index) {
            let mut look = DevelopSettings::default();
            let effective = settings.effective();
            for section in sections {
                section.copy(&effective, &mut look);
            }
            preset.settings = look;
        }
    }

    /// Cycles a saved preset to the next category.
    pub fn next_category(&mut self, index: usize) {
        if let Some(preset) = self.user.get_mut(index) {
            let current = PRESET_CATEGORIES
                .iter()
                .position(|c| c.eq_ignore_ascii_case(preset.category_label()))
                .unwrap_or(PRESET_CATEGORIES.len() - 1);
            preset.category =
                PRESET_CATEGORIES[(current + 1) % PRESET_CATEGORIES.len()].to_string();
        }
    }

    /// Adds presets read from a file, renaming any whose name is taken.
    /// Returns how many were added.
    pub fn import(&mut self, incoming: Vec<DevelopPreset>) -> usize {
        let mut added = 0;
        for mut preset in incoming {
            let base = preset.name.trim().to_string();
            let base = if base.is_empty() {
                "Imported".to_string()
            } else {
                base
            };
            let mut name = base.clone();
            let mut n = 2;
            while self.user.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) {
                name = format!("{base} {n}");
                n += 1;
            }
            preset.name = name;
            self.user.push(preset);
            added += 1;
        }
        added
    }

    fn next_default_name(&self) -> String {
        (1..)
            .map(|n| format!("Preset {n}"))
            .find(|name| !self.user.iter().any(|p| p.name.eq_ignore_ascii_case(name)))
            .expect("an unused name always exists")
    }
}

impl DevelopSettings {
    /// These settings with `look` applied: only the `sections` of it, at
    /// `amount` (0.0 = unchanged, 1.0 = the full look). Sliders blend
    /// smoothly; curves, mixer and grading switch over at half strength.
    /// The photo's crop, rotation, flips, retouching and masks are kept.
    pub fn with_look_blended(
        &self,
        look: &DevelopSettings,
        sections: &[DevelopSection],
        amount: f32,
    ) -> DevelopSettings {
        let amount = amount.clamp(0.0, 1.0);
        let mut full = *self;
        for section in sections {
            section.copy(look, &mut full);
        }
        if amount >= 1.0 {
            return full;
        }
        let mut out = *self;
        let mix = |a: f32, b: f32| a + (b - a) * amount;
        out.exposure = mix(self.exposure, full.exposure);
        out.contrast = mix(self.contrast, full.contrast);
        out.highlights = mix(self.highlights, full.highlights);
        out.shadows = mix(self.shadows, full.shadows);
        out.whites = mix(self.whites, full.whites);
        out.blacks = mix(self.blacks, full.blacks);
        out.temperature = mix(self.temperature, full.temperature);
        out.tint = mix(self.tint, full.tint);
        out.vibrance = mix(self.vibrance, full.vibrance);
        out.saturation = mix(self.saturation, full.saturation);
        out.clarity = mix(self.clarity, full.clarity);
        out.sharpening = mix(self.sharpening, full.sharpening);
        out.dehaze = mix(self.dehaze, full.dehaze);
        out.vignette = mix(self.vignette, full.vignette);
        out.vignette_midpoint = mix(self.vignette_midpoint, full.vignette_midpoint);
        out.vignette_feather = mix(self.vignette_feather, full.vignette_feather);
        out.grain = mix(self.grain, full.grain);
        out.grain_size = mix(self.grain_size, full.grain_size);
        if amount >= 0.5 {
            out.curves = full.curves;
            out.mixer = full.mixer;
            out.grading = full.grading;
        }
        out
    }
}

/// Tints one tonal zone of the colour grading.
fn tint(s: &mut DevelopSettings, zone: ToneZone, hue: f32, saturation: f32) {
    let grade = s.grading.zone_mut(zone);
    grade.hue = hue;
    grade.saturation = saturation;
}

/// Adds control points to a curve.
fn plot(curve: &mut ToneCurve, points: &[(f32, f32)]) {
    for &(x, y) in points {
        if curve.insert(x, y).is_none() {
            // Already a point near there: move it instead.
            if let Some(ix) = curve.points().iter().position(|p| (p[0] - x).abs() < 0.05) {
                curve.move_point(ix, x, y);
            }
        }
    }
}

impl DevelopSettings {
    /// These settings with `look` stacked on top: its sliders are added to
    /// the current ones (scaled by `amount`, 0.0 to 1.0) instead of
    /// replacing them, so several presets can be applied one after another.
    /// Only the `sections` of the look count. Curves are taken from the look
    /// when it has any; gradings are added per tonal range.
    pub fn with_look_added(
        &self,
        look: &DevelopSettings,
        sections: &[DevelopSection],
        amount: f32,
    ) -> DevelopSettings {
        let amount = amount.clamp(0.0, 1.0);
        let neutral = DevelopSettings::default();
        // What the look changes, relative to neutral, section by section.
        let mut delta = neutral;
        for section in sections {
            section.copy(look, &mut delta);
        }
        let mut out = *self;
        let add = |cur: f32, d: f32, base: f32, lo: f32, hi: f32| {
            (cur + (d - base) * amount).clamp(lo, hi)
        };
        macro_rules! slider {
            ($field:ident, $lo:expr, $hi:expr) => {
                out.$field = add(self.$field, delta.$field, neutral.$field, $lo, $hi);
            };
        }
        slider!(exposure, -100.0, 100.0);
        slider!(contrast, -100.0, 100.0);
        slider!(highlights, -100.0, 100.0);
        slider!(shadows, -100.0, 100.0);
        slider!(whites, -100.0, 100.0);
        slider!(blacks, -100.0, 100.0);
        slider!(temperature, -100.0, 100.0);
        slider!(tint, -100.0, 100.0);
        slider!(vibrance, -100.0, 100.0);
        slider!(saturation, -100.0, 100.0);
        slider!(clarity, -100.0, 100.0);
        slider!(sharpening, -100.0, 100.0);
        slider!(sharpen_radius, 0.0, 100.0);
        slider!(sharpen_masking, 0.0, 100.0);
        slider!(noise_luma, 0.0, 100.0);
        slider!(noise_color, 0.0, 100.0);
        slider!(dehaze, -100.0, 100.0);
        slider!(vignette, -100.0, 100.0);
        slider!(vignette_midpoint, 0.0, 100.0);
        slider!(vignette_feather, 0.0, 100.0);
        slider!(grain, 0.0, 100.0);
        slider!(grain_size, 0.0, 100.0);

        out.parametric.highlights = add(
            self.parametric.highlights,
            delta.parametric.highlights,
            0.0,
            -100.0,
            100.0,
        );
        out.parametric.lights = add(
            self.parametric.lights,
            delta.parametric.lights,
            0.0,
            -100.0,
            100.0,
        );
        out.parametric.darks = add(
            self.parametric.darks,
            delta.parametric.darks,
            0.0,
            -100.0,
            100.0,
        );
        out.parametric.shadows = add(
            self.parametric.shadows,
            delta.parametric.shadows,
            0.0,
            -100.0,
            100.0,
        );

        for i in 0..out.mixer.len() {
            let (cur, d) = (self.mixer[i], delta.mixer[i]);
            out.mixer[i].hue = add(cur.hue, d.hue, 0.0, -100.0, 100.0);
            out.mixer[i].saturation = add(cur.saturation, d.saturation, 0.0, -100.0, 100.0);
            out.mixer[i].luminance = add(cur.luminance, d.luminance, 0.0, -100.0, 100.0);
        }

        for zone in ToneZone::ALL {
            let look_grade = *delta.grading.zone(zone);
            if look_grade.saturation > 0.0 {
                let own = out.grading.zone_mut(zone);
                own.hue = look_grade.hue;
                own.saturation = (own.saturation + look_grade.saturation * amount).min(100.0);
            }
        }
        out.grading.balance = add(
            self.grading.balance,
            delta.grading.balance,
            0.0,
            -100.0,
            100.0,
        );

        if amount >= 0.5 {
            for channel in crate::CurveChannel::ALL {
                if !delta.curves.get(channel).is_identity() {
                    *out.curves.get_mut(channel) = *delta.curves.get(channel);
                }
            }
        }
        out
    }
}

/// The presets that ship with the app.
pub fn builtin_presets() -> Vec<DevelopPreset> {
    let look = |category: &str, name: &str, edit: fn(&mut DevelopSettings)| {
        let mut settings = DevelopSettings::default();
        edit(&mut settings);
        DevelopPreset {
            name: name.to_string(),
            settings,
            category: category.to_string(),
            favorite: false,
            notes: String::new(),
        }
    };
    vec![
        // Basic
        look("Basic", "Punchy", |s| {
            s.contrast = 25.0;
            s.vibrance = 25.0;
            s.clarity = 15.0;
        }),
        look("Basic", "Soft", |s| {
            s.contrast = -20.0;
            s.highlights = -15.0;
            s.clarity = -15.0;
        }),
        look("Basic", "Bright", |s| {
            s.exposure = 15.0;
            s.shadows = 20.0;
        }),
        look("Basic", "Matte", |s| {
            s.contrast = -15.0;
            s.blacks = 25.0;
        }),
        look("Basic", "Warm", |s| {
            s.temperature = 20.0;
            s.tint = 5.0;
        }),
        look("Basic", "Cool", |s| s.temperature = -20.0),
        look("Basic", "Crisp", |s| {
            s.clarity = 25.0;
            s.sharpening = 35.0;
            s.contrast = 10.0;
        }),
        look("Basic", "Faded", |s| {
            s.blacks = 35.0;
            s.contrast = -15.0;
            s.saturation = -15.0;
        }),
        look("Basic", "Rich Shadows", |s| {
            s.shadows = 30.0;
            s.blacks = -15.0;
            s.vibrance = 15.0;
        }),
        // Black & white
        look("B&W", "Black & White", |s| {
            s.saturation = -100.0;
            s.contrast = 20.0;
        }),
        look("B&W", "High Contrast B&W", |s| {
            s.saturation = -100.0;
            s.contrast = 55.0;
            s.blacks = -20.0;
            s.whites = 15.0;
            s.clarity = 20.0;
        }),
        look("B&W", "Soft B&W", |s| {
            s.saturation = -100.0;
            s.contrast = -10.0;
            s.highlights = -15.0;
            s.shadows = 20.0;
        }),
        look("B&W", "Film Noir", |s| {
            s.saturation = -100.0;
            s.contrast = 45.0;
            s.exposure = -10.0;
            s.vignette = -35.0;
            s.grain = 30.0;
        }),
        look("B&W", "Silver", |s| {
            s.saturation = -100.0;
            s.contrast = 15.0;
            s.whites = 20.0;
            s.clarity = 15.0;
            s.grain = 12.0;
        }),
        // Portrait
        look("Portrait", "Soft Skin", |s| {
            s.clarity = -20.0;
            s.contrast = -5.0;
            s.vibrance = 10.0;
            s.temperature = 6.0;
        }),
        look("Portrait", "Glow", |s| {
            s.exposure = 10.0;
            s.highlights = -20.0;
            s.shadows = 15.0;
            s.clarity = -12.0;
            s.temperature = 8.0;
        }),
        look("Portrait", "Clean & Bright", |s| {
            s.exposure = 12.0;
            s.whites = 15.0;
            s.vibrance = 10.0;
        }),
        look("Portrait", "Moody", |s| {
            s.exposure = -12.0;
            s.contrast = 20.0;
            s.saturation = -15.0;
            s.vignette = -30.0;
        }),
        // Landscape
        look("Landscape", "Vivid", |s| {
            s.contrast = 20.0;
            s.vibrance = 35.0;
            s.saturation = 12.0;
            s.clarity = 20.0;
            s.dehaze = 10.0;
        }),
        look("Landscape", "Golden Hour", |s| {
            s.temperature = 28.0;
            s.tint = 6.0;
            s.highlights = -20.0;
            s.shadows = 20.0;
            s.vibrance = 20.0;
        }),
        look("Landscape", "Deep Blue", |s| {
            s.temperature = -15.0;
            s.contrast = 15.0;
            s.saturation = 10.0;
            s.dehaze = 15.0;
        }),
        look("Landscape", "Hazy Morning", |s| {
            s.contrast = -15.0;
            s.blacks = 20.0;
            s.temperature = -8.0;
            s.dehaze = -15.0;
        }),
        // Film
        look("Film", "Faded Film", |s| {
            s.blacks = 40.0;
            s.contrast = -20.0;
            s.saturation = -12.0;
            s.grain = 25.0;
        }),
        look("Film", "Vintage", |s| {
            s.temperature = 15.0;
            s.tint = 8.0;
            s.contrast = -10.0;
            s.saturation = -18.0;
            s.vignette = -20.0;
            s.grain = 20.0;
        }),
        look("Film", "Kodak Warm", |s| {
            s.temperature = 18.0;
            s.contrast = 12.0;
            s.saturation = 8.0;
            s.grain = 15.0;
        }),
        look("Film", "Cross Process", |s| {
            s.contrast = 30.0;
            s.tint = -15.0;
            s.saturation = 25.0;
            s.temperature = -10.0;
        }),
        // Cinematic
        look("Cinematic", "Cinematic", |s| {
            s.contrast = 20.0;
            s.saturation = -10.0;
            s.highlights = -20.0;
            s.shadows = 10.0;
            s.vignette = -25.0;
        }),
        look("Cinematic", "Teal Shadows", |s| {
            s.temperature = -8.0;
            s.tint = -6.0;
            s.contrast = 25.0;
            s.vibrance = 15.0;
            s.vignette = -20.0;
        }),
        look("Cinematic", "Dramatic", |s| {
            s.contrast = 40.0;
            s.clarity = 30.0;
            s.exposure = -8.0;
            s.vignette = -35.0;
            s.dehaze = 12.0;
        }),
        // Colour grade: split-toning the shadows, midtones and highlights.
        look("Color Grade", "Teal & Orange", |s| {
            s.contrast = 15.0;
            tint(s, ToneZone::Shadows, 195.0, 35.0);
            tint(s, ToneZone::Highlights, 30.0, 35.0);
        }),
        look("Color Grade", "Golden Teal", |s| {
            s.contrast = 10.0;
            tint(s, ToneZone::Shadows, 185.0, 25.0);
            tint(s, ToneZone::Highlights, 45.0, 30.0);
            s.grading.balance = 10.0;
        }),
        look("Color Grade", "Cold Steel", |s| {
            s.contrast = 15.0;
            s.saturation = -15.0;
            tint(s, ToneZone::Shadows, 215.0, 30.0);
            tint(s, ToneZone::Midtones, 210.0, 10.0);
            tint(s, ToneZone::Highlights, 200.0, 12.0);
        }),
        look("Color Grade", "Sunset Glow", |s| {
            s.temperature = 10.0;
            tint(s, ToneZone::Shadows, 340.0, 20.0);
            tint(s, ToneZone::Midtones, 30.0, 15.0);
            tint(s, ToneZone::Highlights, 20.0, 40.0);
        }),
        look("Color Grade", "Magenta Dusk", |s| {
            tint(s, ToneZone::Shadows, 290.0, 30.0);
            tint(s, ToneZone::Highlights, 330.0, 25.0);
            s.contrast = 10.0;
        }),
        look("Color Grade", "Emerald Shadows", |s| {
            tint(s, ToneZone::Shadows, 150.0, 30.0);
            tint(s, ToneZone::Highlights, 55.0, 18.0);
            s.contrast = 8.0;
        }),
        look("Color Grade", "Rose Gold", |s| {
            s.blacks = 12.0;
            s.vibrance = 10.0;
            tint(s, ToneZone::Shadows, 5.0, 10.0);
            tint(s, ToneZone::Midtones, 350.0, 10.0);
            tint(s, ToneZone::Highlights, 15.0, 28.0);
        }),
        look("Color Grade", "Midnight Blue", |s| {
            s.exposure = -8.0;
            s.contrast = 12.0;
            tint(s, ToneZone::Shadows, 230.0, 40.0);
            tint(s, ToneZone::Midtones, 220.0, 12.0);
        }),
        look("Color Grade", "Autumn", |s| {
            s.vibrance = 15.0;
            tint(s, ToneZone::Shadows, 15.0, 25.0);
            tint(s, ToneZone::Highlights, 38.0, 35.0);
            s.mixer[ColorBand::Green.index()].saturation = -15.0;
            s.mixer[ColorBand::Orange.index()].saturation = 18.0;
        }),
        look("Color Grade", "Arctic", |s| {
            s.temperature = -15.0;
            s.exposure = 8.0;
            tint(s, ToneZone::Shadows, 205.0, 20.0);
            tint(s, ToneZone::Highlights, 190.0, 10.0);
        }),
        look("Color Grade", "Neon Night", |s| {
            s.contrast = 30.0;
            s.saturation = 20.0;
            s.vibrance = 20.0;
            tint(s, ToneZone::Shadows, 270.0, 35.0);
            tint(s, ToneZone::Highlights, 190.0, 30.0);
        }),
        look("Color Grade", "Vaporwave", |s| {
            s.saturation = 10.0;
            s.contrast = 12.0;
            tint(s, ToneZone::Shadows, 270.0, 40.0);
            tint(s, ToneZone::Midtones, 300.0, 15.0);
            tint(s, ToneZone::Highlights, 330.0, 40.0);
        }),
        look("Color Grade", "Cyberpunk", |s| {
            s.contrast = 28.0;
            s.vibrance = 15.0;
            tint(s, ToneZone::Shadows, 275.0, 35.0);
            tint(s, ToneZone::Highlights, 190.0, 35.0);
            s.mixer[ColorBand::Magenta.index()].saturation = 25.0;
            s.mixer[ColorBand::Aqua.index()].saturation = 20.0;
        }),
        look("Color Grade", "Day for Night", |s| {
            s.exposure = -25.0;
            s.temperature = -25.0;
            s.contrast = 15.0;
            s.vignette = -30.0;
            tint(s, ToneZone::Shadows, 220.0, 25.0);
        }),
        look("Color Grade", "Blue Hour", |s| {
            s.temperature = -18.0;
            s.exposure = -6.0;
            tint(s, ToneZone::Shadows, 225.0, 30.0);
            tint(s, ToneZone::Highlights, 25.0, 15.0);
        }),
        // Curves: per-channel RGB curves.
        look("Curves", "Strong S-Curve", |s| {
            plot(&mut s.curves.master, &[(0.25, 0.17), (0.75, 0.83)]);
        }),
        look("Curves", "Soft S-Curve", |s| {
            plot(&mut s.curves.master, &[(0.25, 0.23), (0.75, 0.77)]);
        }),
        look("Curves", "Faded Blacks", |s| {
            s.curves.master.move_point(0, 0.0, 0.08);
        }),
        look("Curves", "Matte Curve", |s| {
            s.curves.master.move_point(0, 0.0, 0.07);
            s.curves.master.move_point(1, 1.0, 0.94);
            plot(&mut s.curves.master, &[(0.5, 0.5)]);
        }),
        look("Curves", "High Key", |s| {
            plot(&mut s.curves.master, &[(0.5, 0.63), (0.8, 0.92)]);
        }),
        look("Curves", "Low Key", |s| {
            plot(&mut s.curves.master, &[(0.2, 0.1), (0.5, 0.38)]);
        }),
        look("Curves", "Cross Process Curves", |s| {
            plot(&mut s.curves.red, &[(0.25, 0.18), (0.75, 0.85)]);
            plot(&mut s.curves.green, &[(0.5, 0.54)]);
            plot(&mut s.curves.blue, &[(0.25, 0.33), (0.75, 0.67)]);
        }),
        look("Curves", "Teal Shadows Curve", |s| {
            plot(&mut s.curves.red, &[(0.2, 0.14)]);
            plot(&mut s.curves.blue, &[(0.2, 0.27)]);
            plot(&mut s.curves.master, &[(0.25, 0.22), (0.75, 0.78)]);
        }),
        look("Curves", "Warm Highlights Curve", |s| {
            plot(&mut s.curves.red, &[(0.8, 0.86)]);
            plot(&mut s.curves.blue, &[(0.8, 0.74)]);
        }),
        look("Curves", "Matrix Green", |s| {
            plot(&mut s.curves.green, &[(0.5, 0.58)]);
            plot(&mut s.curves.red, &[(0.5, 0.46)]);
            plot(&mut s.curves.blue, &[(0.5, 0.45)]);
            s.contrast = 10.0;
        }),
        look("Curves", "Magenta Fade", |s| {
            plot(&mut s.curves.green, &[(0.5, 0.45)]);
            s.curves.master.move_point(0, 0.0, 0.06);
        }),
        look("Curves", "Blue Shadow Fade", |s| {
            s.curves.blue.move_point(0, 0.0, 0.1);
            plot(&mut s.curves.master, &[(0.25, 0.22), (0.75, 0.79)]);
        }),
        look("Curves", "Vintage RGB Fade", |s| {
            s.curves.red.move_point(0, 0.0, 0.06);
            s.curves.green.move_point(0, 0.0, 0.04);
            s.curves.blue.move_point(0, 0.0, 0.09);
            s.curves.blue.move_point(1, 1.0, 0.93);
            s.saturation = -8.0;
        }),
        look("Curves", "Crushed Blacks", |s| {
            s.curves.master.move_point(0, 0.0, 0.0);
            plot(&mut s.curves.master, &[(0.15, 0.06), (0.8, 0.84)]);
        }),
        // Creative looks.
        look("Creative", "Cyanotype", |s| {
            s.saturation = -100.0;
            s.contrast = 15.0;
            tint(s, ToneZone::Shadows, 210.0, 60.0);
            tint(s, ToneZone::Midtones, 205.0, 45.0);
            tint(s, ToneZone::Highlights, 195.0, 25.0);
        }),
        look("Creative", "Sepia", |s| {
            s.saturation = -100.0;
            s.contrast = 8.0;
            tint(s, ToneZone::Shadows, 28.0, 30.0);
            tint(s, ToneZone::Midtones, 35.0, 35.0);
            tint(s, ToneZone::Highlights, 42.0, 22.0);
        }),
        look("Creative", "Selenium Tone", |s| {
            s.saturation = -100.0;
            s.contrast = 18.0;
            tint(s, ToneZone::Shadows, 270.0, 14.0);
            tint(s, ToneZone::Highlights, 40.0, 6.0);
        }),
        look("Creative", "Purple & Gold Duotone", |s| {
            s.saturation = -100.0;
            s.contrast = 20.0;
            tint(s, ToneZone::Shadows, 275.0, 55.0);
            tint(s, ToneZone::Midtones, 320.0, 20.0);
            tint(s, ToneZone::Highlights, 45.0, 50.0);
        }),
        look("Creative", "Infrared", |s| {
            s.saturation = -100.0;
            s.contrast = 20.0;
            s.mixer[ColorBand::Green.index()].luminance = 60.0;
            s.mixer[ColorBand::Yellow.index()].luminance = 30.0;
            s.mixer[ColorBand::Blue.index()].luminance = -55.0;
            s.mixer[ColorBand::Aqua.index()].luminance = -40.0;
            s.grain = 15.0;
        }),
        look("Creative", "Lomo", |s| {
            s.saturation = 20.0;
            s.contrast = 25.0;
            s.vignette = -45.0;
            plot(&mut s.curves.red, &[(0.25, 0.2), (0.75, 0.82)]);
            plot(&mut s.curves.blue, &[(0.25, 0.3), (0.75, 0.7)]);
        }),
        look("Creative", "Polaroid", |s| {
            s.temperature = 12.0;
            s.blacks = 28.0;
            s.contrast = -8.0;
            s.vignette = -15.0;
            tint(s, ToneZone::Highlights, 45.0, 10.0);
        }),
        look("Creative", "Kodachrome", |s| {
            s.contrast = 20.0;
            s.saturation = 8.0;
            s.mixer[ColorBand::Red.index()].saturation = 15.0;
            s.mixer[ColorBand::Yellow.index()].saturation = 10.0;
            s.mixer[ColorBand::Blue.index()].luminance = -10.0;
            s.grain = 12.0;
        }),
        look("Creative", "Technicolor", |s| {
            s.contrast = 22.0;
            s.saturation = 12.0;
            s.mixer[ColorBand::Red.index()].saturation = 25.0;
            s.mixer[ColorBand::Green.index()].saturation = 10.0;
            s.mixer[ColorBand::Aqua.index()].saturation = 18.0;
        }),
        look("Creative", "Dreamy", |s| {
            s.exposure = 10.0;
            s.clarity = -30.0;
            s.highlights = -10.0;
            s.blacks = 12.0;
            tint(s, ToneZone::Highlights, 20.0, 10.0);
        }),
        look("Creative", "HDR", |s| {
            s.clarity = 50.0;
            s.shadows = 40.0;
            s.highlights = -40.0;
            s.vibrance = 20.0;
            s.dehaze = 15.0;
        }),
        // More for landscapes and portraits.
        look("Landscape", "Sunny Day", |s| {
            s.temperature = 10.0;
            s.vibrance = 20.0;
            s.highlights = -10.0;
        }),
        look("Landscape", "Overcast Fix", |s| {
            s.contrast = 15.0;
            s.vibrance = 20.0;
            s.temperature = 8.0;
            s.dehaze = 10.0;
        }),
        look("Landscape", "Foliage Pop", |s| {
            s.mixer[ColorBand::Green.index()].saturation = 25.0;
            s.mixer[ColorBand::Green.index()].luminance = -10.0;
            s.mixer[ColorBand::Yellow.index()].saturation = 12.0;
            s.clarity = 10.0;
        }),
        look("Landscape", "Sky Drama", |s| {
            s.contrast = 10.0;
            s.mixer[ColorBand::Blue.index()].saturation = 20.0;
            s.mixer[ColorBand::Blue.index()].luminance = -25.0;
            s.mixer[ColorBand::Aqua.index()].luminance = -15.0;
            s.clarity = 15.0;
        }),
        look("Landscape", "Autumn Gold", |s| {
            s.vibrance = 15.0;
            s.mixer[ColorBand::Orange.index()].saturation = 25.0;
            s.mixer[ColorBand::Yellow.index()].saturation = 25.0;
            s.temperature = 8.0;
        }),
        look("Portrait", "Warm Skin Glow", |s| {
            s.temperature = 8.0;
            s.mixer[ColorBand::Orange.index()].luminance = 10.0;
            s.mixer[ColorBand::Orange.index()].saturation = -5.0;
            s.mixer[ColorBand::Red.index()].saturation = -6.0;
            s.clarity = -10.0;
        }),
        look("Portrait", "Pastel", |s| {
            s.exposure = 12.0;
            s.saturation = -15.0;
            s.contrast = -12.0;
            s.blacks = 15.0;
            tint(s, ToneZone::Highlights, 20.0, 8.0);
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_strips_geometry_and_replaces_by_name() {
        let mut presets = Presets::default();
        let mut settings = DevelopSettings {
            exposure: 10.0,
            rotation_degrees: 90.0,
            ..Default::default()
        };
        assert_eq!(presets.save("Mine", &settings), "Mine");
        assert_eq!(presets.user[0].settings.rotation_degrees, 0.0);

        settings.exposure = 20.0;
        presets.save("mine", &settings);
        assert_eq!(presets.user.len(), 1);
        assert_eq!(presets.user[0].settings.exposure, 20.0);
    }

    #[test]
    fn blank_names_get_numbered() {
        let mut presets = Presets::default();
        let settings = DevelopSettings::default();
        assert_eq!(presets.save("  ", &settings), "Preset 1");
        assert_eq!(presets.save("", &settings), "Preset 2");
    }

    #[test]
    fn blending_scales_sliders_and_honours_sections() {
        let photo = DevelopSettings {
            exposure: 10.0,
            rotation_degrees: 90.0,
            ..Default::default()
        };
        let look = DevelopSettings {
            exposure: 50.0,
            saturation: -100.0,
            ..Default::default()
        };
        let half = photo.with_look_blended(&look, &DevelopSection::LOOK, 0.5);
        assert_eq!(half.exposure, 30.0);
        assert_eq!(half.saturation, -50.0);
        assert_eq!(half.rotation_degrees, 90.0);

        let tone_only = photo.with_look_blended(&look, &[DevelopSection::Basic], 1.0);
        assert_eq!(tone_only.exposure, 50.0);
        assert_eq!(tone_only.saturation, 0.0);
    }

    #[test]
    fn presets_can_be_stacked_one_on_another() {
        let warm = DevelopSettings {
            exposure: 10.0,
            temperature: 20.0,
            ..Default::default()
        };
        let photo = DevelopSettings {
            exposure: 5.0,
            rotation_degrees: 90.0,
            ..Default::default()
        };
        let once = photo.with_look_added(&warm, &DevelopSection::LOOK, 1.0);
        assert_eq!(once.exposure, 15.0);
        assert_eq!(once.rotation_degrees, 90.0);
        // Applying it again adds again.
        let twice = once.with_look_added(&warm, &DevelopSection::LOOK, 1.0);
        assert_eq!(twice.exposure, 25.0);
        assert_eq!(twice.temperature, 40.0);
        // Half strength adds half; a section left out adds nothing.
        let half = photo.with_look_added(&warm, &DevelopSection::LOOK, 0.5);
        assert_eq!(half.exposure, 10.0);
        let tone_only = photo.with_look_added(&warm, &[DevelopSection::Basic], 1.0);
        assert_eq!(tone_only.temperature, 0.0);
        // Sliders stay in range.
        let big = DevelopSettings {
            exposure: 90.0,
            ..Default::default()
        };
        assert_eq!(
            big.with_look_added(&warm, &DevelopSection::LOOK, 1.0)
                .exposure,
            100.0
        );
    }

    #[test]
    fn two_different_built_ins_stack_rather_than_replace() {
        let all = builtin_presets();
        let find = |n: &str| all.iter().find(|p| p.name == n).unwrap().settings;
        let (punchy, warm) = (find("Punchy"), find("Warm"));
        let photo = DevelopSettings::default()
            .with_look_added(&punchy, &DevelopSection::LOOK, 1.0)
            .with_look_added(&warm, &DevelopSection::LOOK, 1.0);
        // Punchy's contrast is still there after Warm.
        assert_eq!(photo.contrast, punchy.contrast);
        assert_eq!(photo.temperature, warm.temperature);
    }

    #[test]
    fn manage_saved_presets() {
        let mut presets = Presets::default();
        let s = DevelopSettings::default();
        presets.save("A", &s);
        presets.save("B", &s);
        assert!(!presets.rename(1, "a"));
        assert!(presets.rename(1, "C"));
        presets.duplicate(0);
        assert_eq!(presets.user[1].name, "A copy");
        presets.move_by(1, 1);
        assert_eq!(presets.user[2].name, "A copy");
        presets.toggle_favorite(0);
        assert!(presets.user[0].favorite);
        assert_eq!(presets.import(vec![presets.user[0].clone()]), 1);
        assert_eq!(presets.user.last().unwrap().name, "A 2");
    }

    #[test]
    fn built_ins_have_real_categories_and_do_something() {
        let neutral = DevelopSettings::default();
        for preset in builtin_presets() {
            assert!(
                PRESET_CATEGORIES.contains(&preset.category.as_str()),
                "{} has category {}",
                preset.name,
                preset.category
            );
            assert!(
                preset.settings != neutral,
                "{} changes nothing",
                preset.name
            );
        }
        assert!(builtin_presets().len() >= 70);
    }

    #[test]
    fn built_in_names_are_unique() {
        let all = builtin_presets();
        for (i, a) in all.iter().enumerate() {
            assert!(all[i + 1..].iter().all(|b| b.name != a.name), "{}", a.name);
        }
    }
}
