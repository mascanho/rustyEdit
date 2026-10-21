//! Develop presets: named looks (tone, colour and detail settings) that
//! can be applied to any photo. A few ship with the app; the rest are saved
//! by the user and stored app-wide.

use serde::{Deserialize, Serialize};

use crate::DevelopSettings;

/// A named look. Only its tone, colour and detail values are used when
/// applying it; a photo's rotation and flips are its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DevelopPreset {
    pub name: String,
    pub settings: DevelopSettings,
}

/// The user's saved presets, in the order they were created.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Presets {
    pub user: Vec<DevelopPreset>,
}

impl Presets {
    /// Saves `settings` as a preset called `name`, replacing any preset of
    /// the same name (case-insensitively). Returns the name used.
    pub fn save(&mut self, name: &str, settings: &DevelopSettings) -> String {
        let name = match name.trim() {
            "" => self.next_default_name(),
            trimmed => trimmed.to_string(),
        };
        let preset = DevelopPreset {
            name: name.clone(),
            settings: DevelopSettings::default().with_look_of(settings),
        };
        match self
            .user
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(&name))
        {
            Some(existing) => *existing = preset,
            None => self.user.push(preset),
        }
        name
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.user.len() {
            self.user.remove(index);
        }
    }

    fn next_default_name(&self) -> String {
        (1..)
            .map(|n| format!("Preset {n}"))
            .find(|name| !self.user.iter().any(|p| p.name.eq_ignore_ascii_case(name)))
            .expect("an unused name always exists")
    }
}

/// The presets that ship with the app.
pub fn builtin_presets() -> Vec<DevelopPreset> {
    let look = |name: &str, edit: fn(&mut DevelopSettings)| {
        let mut settings = DevelopSettings::default();
        edit(&mut settings);
        DevelopPreset {
            name: name.to_string(),
            settings,
        }
    };
    vec![
        look("Punchy", |s| {
            s.contrast = 25.0;
            s.vibrance = 25.0;
            s.clarity = 15.0;
        }),
        look("Soft", |s| {
            s.contrast = -20.0;
            s.highlights = -15.0;
            s.clarity = -15.0;
        }),
        look("Bright", |s| {
            s.exposure = 15.0;
            s.shadows = 20.0;
        }),
        look("Matte", |s| {
            s.contrast = -15.0;
            s.blacks = 25.0;
        }),
        look("Warm", |s| {
            s.temperature = 20.0;
            s.tint = 5.0;
        }),
        look("Cool", |s| s.temperature = -20.0),
        look("Black & White", |s| {
            s.saturation = -100.0;
            s.contrast = 20.0;
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
}
