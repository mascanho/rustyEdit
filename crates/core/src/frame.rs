//! Frames: borders drawn around the finished photo, from a plain white
//! mat to instant-film cards, film rebates and wooden mouldings.
//!
//! A frame is made *after* the photo is developed and cropped, so it grows
//! the picture (a border is added around it) rather than covering any of
//! it. Every style is drawn procedurally from the same few settings, which
//! are all fractions of the photo's short edge, so a frame looks the same
//! on a preview as on the full-size export.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FrameStyle {
    #[default]
    None,
    // Classic mats and borders
    Border,
    Matte,
    FineArt,
    Gallery,
    Passepartout,
    DoubleLine,
    // Prints
    Polaroid,
    Instax,
    Vintage,
    Deckle,
    Stamp,
    // Film
    Film35,
    Film120,
    Slide,
    Cinema,
    Grunge,
    // Mouldings
    Wood,
    Gold,
    Metal,
    // New school
    Rounded,
    Shadow,
    Oval,
    Neon,
    Gradient,
    BlurFill,
    Offset,
}

/// How the styles are grouped in the picker.
pub const FRAME_GROUPS: [(&str, &[FrameStyle]); 5] = [
    (
        "Mats & borders",
        &[
            FrameStyle::Border,
            FrameStyle::Matte,
            FrameStyle::FineArt,
            FrameStyle::Gallery,
            FrameStyle::Passepartout,
            FrameStyle::DoubleLine,
        ],
    ),
    (
        "Prints",
        &[
            FrameStyle::Polaroid,
            FrameStyle::Instax,
            FrameStyle::Vintage,
            FrameStyle::Deckle,
            FrameStyle::Stamp,
        ],
    ),
    (
        "Film",
        &[
            FrameStyle::Film35,
            FrameStyle::Film120,
            FrameStyle::Slide,
            FrameStyle::Cinema,
            FrameStyle::Grunge,
        ],
    ),
    (
        "Mouldings",
        &[FrameStyle::Wood, FrameStyle::Gold, FrameStyle::Metal],
    ),
    (
        "New school",
        &[
            FrameStyle::Rounded,
            FrameStyle::Shadow,
            FrameStyle::Oval,
            FrameStyle::Neon,
            FrameStyle::Gradient,
            FrameStyle::BlurFill,
            FrameStyle::Offset,
        ],
    ),
];

impl FrameStyle {
    pub const ALL: [FrameStyle; 27] = [
        FrameStyle::None,
        FrameStyle::Border,
        FrameStyle::Matte,
        FrameStyle::FineArt,
        FrameStyle::Gallery,
        FrameStyle::Passepartout,
        FrameStyle::DoubleLine,
        FrameStyle::Polaroid,
        FrameStyle::Instax,
        FrameStyle::Vintage,
        FrameStyle::Deckle,
        FrameStyle::Stamp,
        FrameStyle::Film35,
        FrameStyle::Film120,
        FrameStyle::Slide,
        FrameStyle::Cinema,
        FrameStyle::Grunge,
        FrameStyle::Wood,
        FrameStyle::Gold,
        FrameStyle::Metal,
        FrameStyle::Rounded,
        FrameStyle::Shadow,
        FrameStyle::Oval,
        FrameStyle::Neon,
        FrameStyle::Gradient,
        FrameStyle::BlurFill,
        FrameStyle::Offset,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            FrameStyle::None => "None",
            FrameStyle::Border => "Border",
            FrameStyle::Matte => "Matte",
            FrameStyle::FineArt => "Fine art",
            FrameStyle::Gallery => "Gallery",
            FrameStyle::Passepartout => "Passe-partout",
            FrameStyle::DoubleLine => "Double line",
            FrameStyle::Polaroid => "Polaroid",
            FrameStyle::Instax => "Instax",
            FrameStyle::Vintage => "Vintage print",
            FrameStyle::Deckle => "Deckle edge",
            FrameStyle::Stamp => "Postage stamp",
            FrameStyle::Film35 => "35mm film",
            FrameStyle::Film120 => "120 film",
            FrameStyle::Slide => "Slide mount",
            FrameStyle::Cinema => "Cinema",
            FrameStyle::Grunge => "Grunge rebate",
            FrameStyle::Wood => "Wood",
            FrameStyle::Gold => "Gilded",
            FrameStyle::Metal => "Brushed metal",
            FrameStyle::Rounded => "Rounded",
            FrameStyle::Shadow => "Floating",
            FrameStyle::Oval => "Cameo",
            FrameStyle::Neon => "Neon",
            FrameStyle::Gradient => "Gradient",
            FrameStyle::BlurFill => "Blur fill",
            FrameStyle::Offset => "Offset",
        }
    }

    /// What the settings read as for this style: which of the controls do
    /// anything. `(colour, corner, line, shadow)`.
    pub fn uses(&self) -> (bool, bool, bool, bool) {
        match self {
            FrameStyle::None => (false, false, false, false),
            FrameStyle::Border => (true, false, true, false),
            FrameStyle::Matte | FrameStyle::FineArt | FrameStyle::Gallery => (true, false, true, true),
            FrameStyle::Passepartout | FrameStyle::DoubleLine => (true, false, true, false),
            FrameStyle::Polaroid | FrameStyle::Instax => (true, true, false, true),
            FrameStyle::Vintage | FrameStyle::Deckle => (true, false, false, true),
            FrameStyle::Stamp => (true, false, true, false),
            FrameStyle::Film35 | FrameStyle::Film120 => (true, true, true, false),
            FrameStyle::Slide => (true, true, false, true),
            FrameStyle::Cinema => (true, false, false, false),
            FrameStyle::Grunge => (true, false, false, false),
            FrameStyle::Wood | FrameStyle::Gold | FrameStyle::Metal => (true, false, false, true),
            FrameStyle::Rounded => (true, true, true, false),
            FrameStyle::Shadow => (true, true, false, true),
            FrameStyle::Oval => (true, false, true, true),
            FrameStyle::Neon => (true, true, true, true),
            FrameStyle::Gradient => (true, true, false, true),
            FrameStyle::BlurFill => (false, true, false, true),
            FrameStyle::Offset => (true, true, true, true),
        }
    }

    /// A style's own look: the settings it starts from when picked.
    pub fn defaults(&self) -> FrameSettings {
        let base = FrameSettings {
            style: *self,
            ..FrameSettings::NEUTRAL
        };
        // (size, hue, saturation, lightness, corner, line, shadow)
        let (size, hue, sat, light, corner, line, shadow) = match self {
            FrameStyle::None => return FrameSettings::default(),
            FrameStyle::Border => (20.0, 0.0, 0.0, 98.0, 0.0, 0.0, 0.0),
            FrameStyle::Matte => (22.0, 40.0, 12.0, 95.0, 0.0, 40.0, 40.0),
            FrameStyle::FineArt => (30.0, 0.0, 0.0, 99.0, 0.0, 50.0, 0.0),
            FrameStyle::Gallery => (28.0, 40.0, 8.0, 96.0, 0.0, 30.0, 60.0),
            FrameStyle::Passepartout => (26.0, 200.0, 8.0, 22.0, 0.0, 60.0, 0.0),
            FrameStyle::DoubleLine => (22.0, 0.0, 0.0, 97.0, 0.0, 50.0, 0.0),
            FrameStyle::Polaroid => (20.0, 45.0, 28.0, 94.0, 15.0, 0.0, 40.0),
            FrameStyle::Instax => (20.0, 0.0, 0.0, 98.0, 30.0, 0.0, 30.0),
            FrameStyle::Vintage => (22.0, 42.0, 40.0, 88.0, 0.0, 0.0, 50.0),
            FrameStyle::Deckle => (22.0, 40.0, 14.0, 96.0, 0.0, 0.0, 50.0),
            FrameStyle::Stamp => (24.0, 0.0, 0.0, 98.0, 0.0, 40.0, 0.0),
            FrameStyle::Film35 => (20.0, 30.0, 35.0, 5.0, 12.0, 60.0, 0.0),
            FrameStyle::Film120 => (20.0, 30.0, 25.0, 6.0, 8.0, 60.0, 0.0),
            FrameStyle::Slide => (20.0, 0.0, 0.0, 92.0, 25.0, 0.0, 50.0),
            FrameStyle::Cinema => (24.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0),
            FrameStyle::Grunge => (22.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0),
            FrameStyle::Wood => (24.0, 28.0, 55.0, 32.0, 0.0, 0.0, 60.0),
            FrameStyle::Gold => (24.0, 44.0, 85.0, 52.0, 0.0, 0.0, 60.0),
            FrameStyle::Metal => (18.0, 215.0, 6.0, 68.0, 0.0, 0.0, 60.0),
            FrameStyle::Rounded => (18.0, 0.0, 0.0, 96.0, 40.0, 0.0, 0.0),
            FrameStyle::Shadow => (30.0, 220.0, 8.0, 92.0, 20.0, 0.0, 60.0),
            FrameStyle::Oval => (30.0, 40.0, 10.0, 95.0, 0.0, 50.0, 50.0),
            FrameStyle::Neon => (30.0, 320.0, 100.0, 60.0, 15.0, 50.0, 70.0),
            FrameStyle::Gradient => (20.0, 330.0, 85.0, 60.0, 20.0, 0.0, 40.0),
            FrameStyle::BlurFill => (34.0, 0.0, 0.0, 0.0, 25.0, 0.0, 60.0),
            FrameStyle::Offset => (22.0, 52.0, 95.0, 62.0, 0.0, 70.0, 100.0),
        };
        FrameSettings {
            size,
            hue,
            saturation: sat,
            lightness: light,
            corner,
            line,
            shadow,
            ..base
        }
    }
}

/// A photo's frame. Every value is 0..=100 (the hue 0..=360).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FrameSettings {
    pub style: FrameStyle,
    /// How wide the border is.
    pub size: f32,
    /// The border's colour, where the style uses one.
    pub hue: f32,
    pub saturation: f32,
    pub lightness: f32,
    /// Roundness of corners.
    pub corner: f32,
    /// The thin keyline, or the weight of a rule, a ring or an outline.
    pub line: f32,
    /// Depth: drop shadow, bevel, or glow.
    pub shadow: f32,
}

impl FrameSettings {
    const NEUTRAL: FrameSettings = FrameSettings {
        style: FrameStyle::None,
        size: 20.0,
        hue: 0.0,
        saturation: 0.0,
        lightness: 98.0,
        corner: 0.0,
        line: 0.0,
        shadow: 0.0,
    };

    /// Whether a frame is drawn at all.
    pub fn is_active(&self) -> bool {
        self.style != FrameStyle::None && self.size > 0.0
    }

    /// The border unit in pixels for a photo of `width` × `height`: styles
    /// make their margins out of multiples of it.
    pub fn unit(&self, width: u32, height: u32) -> f32 {
        let short = width.min(height) as f32;
        (short * (0.004 + self.size.clamp(0.0, 100.0) / 100.0 * 0.2)).max(1.0)
    }

    /// The border's `[left, top, right, bottom]` width in pixels around a
    /// `width` × `height` photo.
    pub fn margins(&self, width: u32, height: u32) -> [u32; 4] {
        if !self.is_active() {
            return [0; 4];
        }
        let m = self.unit(width, height);
        let [l, t, r, b] = self.style.margin_factors();
        let px = |f: f32| (m * f).round().max(0.0) as u32;
        [px(l), px(t), px(r), px(b)]
    }
}

impl Default for FrameSettings {
    fn default() -> Self {
        Self::NEUTRAL
    }
}

impl FrameStyle {
    /// Margins as multiples of the border unit: left, top, right, bottom.
    fn margin_factors(&self) -> [f32; 4] {
        match self {
            FrameStyle::None => [0.0; 4],
            FrameStyle::Border | FrameStyle::Rounded | FrameStyle::Gradient => [1.0; 4],
            FrameStyle::Matte => [2.0; 4],
            FrameStyle::FineArt => [2.5, 2.5, 2.5, 3.2],
            FrameStyle::Gallery => [3.0, 3.0, 3.0, 3.6],
            FrameStyle::Passepartout => [2.2; 4],
            FrameStyle::DoubleLine => [1.8; 4],
            FrameStyle::Polaroid => [1.0, 1.0, 1.0, 4.0],
            FrameStyle::Instax => [1.2, 1.2, 1.2, 4.2],
            FrameStyle::Vintage | FrameStyle::Deckle => [1.4; 4],
            FrameStyle::Stamp => [1.5; 4],
            FrameStyle::Film35 => [0.7, 1.9, 0.7, 1.9],
            FrameStyle::Film120 => [0.9, 1.3, 0.9, 1.3],
            FrameStyle::Slide => [1.3; 4],
            FrameStyle::Cinema => [0.15, 1.6, 0.15, 1.6],
            FrameStyle::Grunge => [1.2; 4],
            FrameStyle::Wood | FrameStyle::Gold => [1.2; 4],
            FrameStyle::Metal => [0.9; 4],
            FrameStyle::Shadow => [2.5; 4],
            FrameStyle::Oval => [1.6; 4],
            FrameStyle::Neon => [2.2; 4],
            FrameStyle::BlurFill => [3.0; 4],
            FrameStyle::Offset => [1.0, 1.0, 1.5, 1.5],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_frame_adds_nothing() {
        assert_eq!(FrameSettings::default().margins(300, 200), [0; 4]);
        assert!(!FrameSettings::default().is_active());
    }

    #[test]
    fn every_style_has_defaults_and_margins() {
        for style in FrameStyle::ALL {
            let frame = style.defaults();
            assert_eq!(frame.style, style);
            if style != FrameStyle::None {
                assert!(frame.is_active());
                let [l, t, r, b] = frame.margins(600, 400);
                assert!(l + t + r + b > 0, "{style:?}");
            }
        }
    }

    #[test]
    fn margins_scale_with_the_photo() {
        let frame = FrameStyle::Border.defaults();
        let small = frame.margins(300, 200)[0];
        let big = frame.margins(3000, 2000)[0];
        assert!(big >= small * 9, "{small} {big}");
    }

    #[test]
    fn every_style_is_in_a_group_once() {
        let mut seen: Vec<FrameStyle> = FRAME_GROUPS.iter().flat_map(|(_, s)| s.iter().copied()).collect();
        seen.sort_by_key(|s| FrameStyle::ALL.iter().position(|a| a == s));
        let all: Vec<_> = FrameStyle::ALL.iter().copied().filter(|s| *s != FrameStyle::None).collect();
        assert_eq!(seen, all);
    }

    #[test]
    fn old_saves_without_a_frame_still_load() {
        let frame: FrameSettings = serde_json::from_str(r#"{"style":"Polaroid"}"#).unwrap();
        assert_eq!(frame.style, FrameStyle::Polaroid);
    }
}
