//! Fujifilm "recipes": the in-camera JPEG settings (film simulation, grain,
//! tone, colour, white balance…) that photographers share. A recipe is
//! converted into a Develop preset that approximates the look on any photo.
//!
//! Fujifilm's film simulations are proprietary colour science, so the
//! conversion is an approximation built from the controls Develop has: tone
//! curve, colour mixer, colour grading, and the basic sliders.

use serde::{Deserialize, Serialize};

use crate::{BandAdjust, ColorBand, DevelopPreset, DevelopSettings, ToneCurve, ToneZone};

macro_rules! choice_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $label:expr),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn label(&self) -> &'static str {
                match self { $($name::$variant => $label),+ }
            }
        }
    };
}

choice_enum!(
    /// The camera's film simulation.
    FilmSimulation {
        Provia => "Provia",
        Velvia => "Velvia",
        Astia => "Astia",
        ClassicChrome => "Classic Chrome",
        RealaAce => "Reala Ace",
        ProNegHi => "Pro Neg. Hi",
        ProNegStd => "Pro Neg. Std",
        ClassicNeg => "Classic Neg.",
        NostalgicNeg => "Nostalgic Neg.",
        EternaCinema => "Eterna",
        EternaBleachBypass => "Eterna Bleach Bypass",
        Acros => "Acros",
        Monochrome => "Monochrome",
        Sepia => "Sepia",
    }
);

impl FilmSimulation {
    /// Whether the simulation is black and white (and so takes a filter).
    pub fn is_monochrome(&self) -> bool {
        matches!(
            self,
            FilmSimulation::Acros | FilmSimulation::Monochrome | FilmSimulation::Sepia
        )
    }
}

choice_enum!(
    /// The colour filter used with the monochrome simulations.
    MonoFilter {
        Off => "None",
        Yellow => "Yellow",
        Red => "Red",
        Green => "Green",
    }
);

choice_enum!(
    DynamicRange {
        Auto => "Auto",
        Dr100 => "DR100",
        Dr200 => "DR200",
        Dr400 => "DR400",
    }
);

choice_enum!(
    GrainEffect {
        Off => "Off",
        WeakSmall => "Weak, Small",
        WeakLarge => "Weak, Large",
        StrongSmall => "Strong, Small",
        StrongLarge => "Strong, Large",
    }
);

choice_enum!(
    /// Strength of Color Chrome Effect / Color Chrome FX Blue.
    Strength {
        Off => "Off",
        Weak => "Weak",
        Strong => "Strong",
    }
);

choice_enum!(
    WhiteBalance {
        Auto => "Auto",
        AutoWhitePriority => "Auto (White Priority)",
        AutoAmbience => "Auto (Ambience Priority)",
        Daylight => "Daylight",
        Shade => "Shade",
        Fluorescent => "Fluorescent",
        Incandescent => "Incandescent",
        Kelvin => "Kelvin",
    }
);

/// A complete recipe, as listed on recipe sites.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FujiRecipe {
    pub name: String,
    pub film_simulation: FilmSimulation,
    /// Only used with the monochrome simulations.
    pub mono_filter: MonoFilter,
    pub dynamic_range: DynamicRange,
    pub grain: GrainEffect,
    pub color_chrome: Strength,
    pub color_chrome_blue: Strength,
    pub white_balance: WhiteBalance,
    /// Used when the white balance is Kelvin.
    pub kelvin: u32,
    /// -9..=9
    pub wb_red: i32,
    /// -9..=9
    pub wb_blue: i32,
    /// -2.0..=4.0 in half steps.
    pub highlight: f32,
    /// -2.0..=4.0 in half steps.
    pub shadow: f32,
    /// -4..=4
    pub color: i32,
    /// -4..=4
    pub sharpness: i32,
    /// -4..=4
    pub high_iso_nr: i32,
    /// -5..=5
    pub clarity: i32,
    /// Free text, e.g. "Auto, up to ISO 6400".
    pub iso: String,
    /// Free text, e.g. "0 to +2/3 (typically)".
    pub exposure_compensation: String,
}

impl Default for FujiRecipe {
    fn default() -> Self {
        Self {
            name: "My recipe".into(),
            film_simulation: FilmSimulation::Provia,
            mono_filter: MonoFilter::Off,
            dynamic_range: DynamicRange::Dr100,
            grain: GrainEffect::Off,
            color_chrome: Strength::Off,
            color_chrome_blue: Strength::Off,
            white_balance: WhiteBalance::Auto,
            kelvin: 5500,
            wb_red: 0,
            wb_blue: 0,
            highlight: 0.0,
            shadow: 0.0,
            color: 0,
            sharpness: 0,
            high_iso_nr: 0,
            clarity: 0,
            iso: "Auto, up to ISO 6400".into(),
            exposure_compensation: "0".into(),
        }
    }
}

impl FujiRecipe {
    /// A ready-made starting point: a Reala Ace recipe.
    pub fn example() -> Self {
        Self {
            name: "Reala Ace".into(),
            film_simulation: FilmSimulation::RealaAce,
            dynamic_range: DynamicRange::Dr200,
            grain: GrainEffect::WeakSmall,
            color_chrome: Strength::Strong,
            color_chrome_blue: Strength::Off,
            white_balance: WhiteBalance::Auto,
            wb_red: 1,
            wb_blue: -2,
            highlight: -1.0,
            shadow: -1.0,
            color: 4,
            sharpness: -1,
            high_iso_nr: -4,
            clarity: -2,
            iso: "Auto, up to ISO 6400".into(),
            exposure_compensation: "0 to +2/3 (typically)".into(),
            ..Self::default()
        }
    }

    /// The recipe written out the way recipe sites list it.
    pub fn summary(&self) -> String {
        let signed = |v: i32| format!("{v:+}");
        let tone = |v: f32| {
            if v == v.trunc() {
                format!("{:+}", v as i32)
            } else {
                format!("{v:+.1}")
            }
        };
        let mut sim = self.film_simulation.label().to_string();
        if self.film_simulation.is_monochrome() && self.mono_filter != MonoFilter::Off {
            sim.push_str(&format!(" ({} filter)", self.mono_filter.label()));
        }
        let wb = if self.white_balance == WhiteBalance::Kelvin {
            format!("{}K", self.kelvin)
        } else {
            self.white_balance.label().to_string()
        };
        let mut lines = vec![
            format!("Film Simulation: {sim}"),
            format!("Dynamic Range: {}", self.dynamic_range.label()),
            format!("Grain Effect: {}", self.grain.label()),
            format!("Color Chrome Effect: {}", self.color_chrome.label()),
            format!("Color Chrome FX Blue: {}", self.color_chrome_blue.label()),
            format!(
                "White Balance: {wb}, {} Red & {} Blue",
                signed(self.wb_red),
                signed(self.wb_blue)
            ),
            format!("Highlight: {}", tone(self.highlight)),
            format!("Shadow: {}", tone(self.shadow)),
            format!("Color: {}", signed(self.color)),
            format!("Sharpness: {}", signed(self.sharpness)),
            format!("High ISO NR: {}", signed(self.high_iso_nr)),
            format!("Clarity: {}", signed(self.clarity)),
        ];
        if !self.iso.trim().is_empty() {
            lines.push(format!("ISO: {}", self.iso.trim()));
        }
        if !self.exposure_compensation.trim().is_empty() {
            lines.push(format!(
                "Exposure Compensation: {}",
                self.exposure_compensation.trim()
            ));
        }
        lines.join("\n")
    }

    /// The Develop settings that approximate this recipe.
    pub fn to_settings(&self) -> DevelopSettings {
        let mut s = DevelopSettings::default();
        self.apply_film_simulation(&mut s);

        // Dynamic range: DR200/400 hold back highlights and open shadows.
        let (dr_highlights, dr_shadows) = match self.dynamic_range {
            DynamicRange::Auto | DynamicRange::Dr100 => (0.0, 0.0),
            DynamicRange::Dr200 => (-18.0, 8.0),
            DynamicRange::Dr400 => (-36.0, 16.0),
        };
        s.highlights += dr_highlights;
        s.shadows += dr_shadows;

        // Highlight / Shadow tone: positive is harder (more contrast), so
        // highlights go up and shadows go down; negative is softer.
        s.highlights += self.highlight * 10.0;
        s.shadows -= self.shadow * 10.0;
        s.contrast += (self.highlight + self.shadow) * 2.5;

        // Colour, sharpness, clarity.
        s.saturation += self.color as f32 * 5.0;
        s.vibrance += self.color as f32 * 2.5;
        s.sharpening += 20.0 + self.sharpness as f32 * 6.0;
        s.clarity += self.clarity as f32 * 5.0;

        self.apply_white_balance(&mut s);
        self.apply_grain(&mut s);
        self.apply_color_chrome(&mut s);

        clamp_settings(&mut s);
        s
    }

    /// The recipe as a preset, filed under Film.
    pub fn to_preset(&self) -> DevelopPreset {
        let mut notes = self.summary();
        notes.push_str(
            "\n\nApproximated from the recipe; the film simulation itself is \
             Fujifilm's own colour science. High ISO NR isn't applied.",
        );
        DevelopPreset {
            name: if self.name.trim().is_empty() {
                format!("{} recipe", self.film_simulation.label())
            } else {
                self.name.trim().to_string()
            },
            settings: self.to_settings(),
            category: "Film".into(),
            favorite: false,
            notes,
        }
    }

    fn apply_white_balance(&self, s: &mut DevelopSettings) {
        let kelvin_offset = match self.white_balance {
            WhiteBalance::Auto | WhiteBalance::AutoWhitePriority | WhiteBalance::Daylight => 0.0,
            WhiteBalance::AutoAmbience => 8.0,
            WhiteBalance::Shade => 40.0,
            WhiteBalance::Fluorescent => -10.0,
            WhiteBalance::Incandescent => -55.0,
            WhiteBalance::Kelvin => (self.kelvin as f32 - 5500.0) / 40.0,
        };
        let (r, b) = (self.wb_red as f32, self.wb_blue as f32);
        // More red / less blue warms; both up leans magenta.
        s.temperature += kelvin_offset + (r - b) * 1.5;
        s.tint += (r + b) * 1.0;
    }

    fn apply_grain(&self, s: &mut DevelopSettings) {
        let (amount, size) = match self.grain {
            GrainEffect::Off => (0.0, 25.0),
            GrainEffect::WeakSmall => (25.0, 25.0),
            GrainEffect::WeakLarge => (25.0, 60.0),
            GrainEffect::StrongSmall => (55.0, 25.0),
            GrainEffect::StrongLarge => (55.0, 60.0),
        };
        s.grain = amount;
        s.grain_size = size;
    }

    /// Color Chrome Effect deepens and enriches saturated colours; the FX
    /// Blue version does the same for blues.
    fn apply_color_chrome(&self, s: &mut DevelopSettings) {
        let level = |st: Strength| match st {
            Strength::Off => 0.0,
            Strength::Weak => 1.0,
            Strength::Strong => 2.0,
        };
        let chrome = level(self.color_chrome);
        if chrome > 0.0 {
            for band in ColorBand::ALL {
                // Skin tones stay gentler.
                let k = if band == ColorBand::Orange { 0.5 } else { 1.0 };
                let m = &mut s.mixer[band.index()];
                m.saturation += 3.0 * chrome * k;
                m.luminance -= 5.0 * chrome * k;
            }
        }
        let blue = level(self.color_chrome_blue);
        if blue > 0.0 {
            let m = &mut s.mixer[ColorBand::Blue.index()];
            m.saturation += 6.0 * blue;
            m.luminance -= 8.0 * blue;
            let a = &mut s.mixer[ColorBand::Aqua.index()];
            a.luminance -= 3.0 * blue;
        }
    }

    fn apply_film_simulation(&self, s: &mut DevelopSettings) {
        let band = |s: &mut DevelopSettings, b: ColorBand, hue: f32, sat: f32, lum: f32| {
            let m = &mut s.mixer[b.index()];
            *m = BandAdjust {
                hue: m.hue + hue,
                saturation: m.saturation + sat,
                luminance: m.luminance + lum,
            };
        };
        let grade = |s: &mut DevelopSettings, zone: ToneZone, hue: f32, sat: f32| {
            let g = s.grading.zone_mut(zone);
            g.hue = hue;
            g.saturation = sat;
        };
        // An S-shaped curve: `amount` > 0 deepens shadows and brightens
        // highlights around the midpoint.
        let s_curve = |s: &mut DevelopSettings, amount: f32| {
            let mut curve = ToneCurve::default();
            curve.insert(0.25, (0.25 - amount).clamp(0.0, 1.0));
            curve.insert(0.75, (0.75 + amount).clamp(0.0, 1.0));
            s.curves.master = curve;
        };

        match self.film_simulation {
            FilmSimulation::Provia => {
                s.contrast = 4.0;
                s.saturation = 3.0;
            }
            FilmSimulation::Velvia => {
                s.contrast = 20.0;
                s.saturation = 22.0;
                s.vibrance = 10.0;
                s_curve(s, 0.03);
                band(s, ColorBand::Blue, 0.0, 10.0, -8.0);
                band(s, ColorBand::Green, 0.0, 8.0, -4.0);
            }
            FilmSimulation::Astia => {
                s.contrast = -4.0;
                s.saturation = 4.0;
                s.highlights = -6.0;
                band(s, ColorBand::Orange, 0.0, 2.0, 5.0);
            }
            FilmSimulation::ClassicChrome => {
                s.contrast = 16.0;
                s.saturation = -18.0;
                s.blacks = 4.0;
                s_curve(s, 0.025);
                grade(s, ToneZone::Shadows, 215.0, 10.0);
                grade(s, ToneZone::Highlights, 45.0, 6.0);
                band(s, ColorBand::Blue, 0.0, -14.0, -10.0);
                band(s, ColorBand::Green, 0.0, -14.0, -4.0);
                band(s, ColorBand::Red, 0.0, 6.0, -4.0);
            }
            FilmSimulation::RealaAce => {
                // Faithful colour with a firm, clean contrast.
                s.contrast = 12.0;
                s.saturation = -2.0;
                s.vibrance = 6.0;
                s.blacks = -4.0;
                s_curve(s, 0.02);
            }
            FilmSimulation::ProNegHi => {
                s.contrast = 8.0;
                s.saturation = -6.0;
                band(s, ColorBand::Orange, 0.0, 0.0, 4.0);
            }
            FilmSimulation::ProNegStd => {
                s.contrast = -8.0;
                s.saturation = -10.0;
                s.highlights = -8.0;
                s.shadows = 6.0;
            }
            FilmSimulation::ClassicNeg => {
                s.contrast = 26.0;
                s.saturation = 4.0;
                s.blacks = 8.0;
                s_curve(s, 0.04);
                grade(s, ToneZone::Shadows, 190.0, 18.0);
                grade(s, ToneZone::Highlights, 35.0, 14.0);
                band(s, ColorBand::Green, -8.0, -6.0, -6.0);
                band(s, ColorBand::Red, 0.0, 6.0, -6.0);
            }
            FilmSimulation::NostalgicNeg => {
                s.contrast = -4.0;
                s.saturation = -6.0;
                s.blacks = 12.0;
                s.temperature = 8.0;
                grade(s, ToneZone::Highlights, 38.0, 16.0);
                grade(s, ToneZone::Shadows, 20.0, 8.0);
                band(s, ColorBand::Blue, 0.0, -10.0, 0.0);
            }
            FilmSimulation::EternaCinema => {
                s.contrast = -10.0;
                s.saturation = -24.0;
                s.shadows = 10.0;
                s.blacks = 6.0;
                grade(s, ToneZone::Shadows, 210.0, 7.0);
            }
            FilmSimulation::EternaBleachBypass => {
                s.contrast = 36.0;
                s.saturation = -48.0;
                s.clarity = 10.0;
                s_curve(s, 0.04);
            }
            FilmSimulation::Acros | FilmSimulation::Monochrome | FilmSimulation::Sepia => {
                s.saturation = -100.0;
                s.contrast = if self.film_simulation == FilmSimulation::Acros {
                    20.0
                } else {
                    10.0
                };
                if self.film_simulation == FilmSimulation::Acros {
                    s_curve(s, 0.03);
                }
                match self.mono_filter {
                    MonoFilter::Off => {}
                    MonoFilter::Yellow => {
                        band(s, ColorBand::Yellow, 0.0, 0.0, 18.0);
                        band(s, ColorBand::Orange, 0.0, 0.0, 10.0);
                        band(s, ColorBand::Blue, 0.0, 0.0, -22.0);
                        band(s, ColorBand::Aqua, 0.0, 0.0, -10.0);
                    }
                    MonoFilter::Red => {
                        band(s, ColorBand::Red, 0.0, 0.0, 35.0);
                        band(s, ColorBand::Orange, 0.0, 0.0, 24.0);
                        band(s, ColorBand::Green, 0.0, 0.0, -18.0);
                        band(s, ColorBand::Aqua, 0.0, 0.0, -30.0);
                        band(s, ColorBand::Blue, 0.0, 0.0, -42.0);
                    }
                    MonoFilter::Green => {
                        band(s, ColorBand::Green, 0.0, 0.0, 32.0);
                        band(s, ColorBand::Yellow, 0.0, 0.0, 12.0);
                        band(s, ColorBand::Red, 0.0, 0.0, -16.0);
                        band(s, ColorBand::Blue, 0.0, 0.0, -10.0);
                    }
                }
                if self.film_simulation == FilmSimulation::Sepia {
                    // Toned after the mono conversion, so tint the zones.
                    grade(s, ToneZone::Shadows, 28.0, 30.0);
                    grade(s, ToneZone::Midtones, 35.0, 35.0);
                    grade(s, ToneZone::Highlights, 42.0, 22.0);
                }
            }
        }
    }
}

fn clamp_settings(s: &mut DevelopSettings) {
    let c = |v: &mut f32| *v = v.clamp(-100.0, 100.0);
    for v in [
        &mut s.exposure,
        &mut s.contrast,
        &mut s.highlights,
        &mut s.shadows,
        &mut s.whites,
        &mut s.blacks,
        &mut s.temperature,
        &mut s.tint,
        &mut s.vibrance,
        &mut s.saturation,
        &mut s.clarity,
        &mut s.sharpening,
    ] {
        c(v);
    }
    s.grain = s.grain.clamp(0.0, 100.0);
    s.grain_size = s.grain_size.clamp(0.0, 100.0);
    for m in s.mixer.iter_mut() {
        m.hue = m.hue.clamp(-100.0, 100.0);
        m.saturation = m.saturation.clamp(-100.0, 100.0);
        m.luminance = m.luminance.clamp(-100.0, 100.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_recipe_maps_to_sensible_settings() {
        let s = FujiRecipe::example().to_settings();
        // Highlight -1 and DR200 both pull highlights down; Shadow -1 lifts.
        assert!(s.highlights < -20.0);
        assert!(s.shadows > 10.0);
        // Color +4 raises saturation; Clarity -2 softens.
        assert!(s.saturation > 10.0);
        assert!(s.clarity < 0.0);
        assert_eq!(s.grain, 25.0);
        // R+1 / B-2 warms and leans slightly green.
        assert!(s.temperature > 0.0);
        assert!(s.tint < 0.0);
        // Color Chrome Strong deepens colours.
        assert!(s.mixer[ColorBand::Red.index()].luminance < 0.0);
    }

    #[test]
    fn acros_with_a_red_filter_darkens_blue_skies() {
        let recipe = FujiRecipe {
            film_simulation: FilmSimulation::Acros,
            mono_filter: MonoFilter::Red,
            ..FujiRecipe::default()
        };
        let s = recipe.to_settings();
        assert_eq!(s.saturation, -100.0);
        assert!(s.mixer[ColorBand::Blue.index()].luminance < -30.0);
    }

    #[test]
    fn preset_is_named_and_filed_under_film() {
        let preset = FujiRecipe::example().to_preset();
        assert_eq!(preset.name, "Reala Ace");
        assert_eq!(preset.category, "Film");
        assert!(preset.notes.contains("Film Simulation: Reala Ace"));
        assert!(preset
            .notes
            .contains("White Balance: Auto, +1 Red & -2 Blue"));
    }

    #[test]
    fn every_simulation_stays_in_range() {
        for sim in FilmSimulation::ALL {
            let s = FujiRecipe {
                film_simulation: *sim,
                dynamic_range: DynamicRange::Dr400,
                color: 4,
                highlight: 4.0,
                shadow: 4.0,
                ..FujiRecipe::default()
            }
            .to_settings();
            for v in [
                s.contrast,
                s.saturation,
                s.highlights,
                s.shadows,
                s.temperature,
            ] {
                assert!((-100.0..=100.0).contains(&v), "{sim:?}");
            }
        }
    }
}
