use gpui::{hsla, Hsla};
use rustyedit_core::{settings::{AppTheme, AppearanceSettings}, ColorLabel};

/// A single, fixed dark theme in the spirit of Lightroom Classic's and
/// Zed's chrome: near-black surfaces, a muted accent, and restrained text
/// contrast so photos (not UI) stay the visual focus.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub background: Hsla,
    pub surface: Hsla,
    pub surface_elevated: Hsla,
    pub border: Hsla,
    pub border_subtle: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_faint: Hsla,
    pub accent: Hsla,
    pub accent_muted: Hsla,
    pub pick: Hsla,
    pub reject: Hsla,
    pub star: Hsla,
    pub danger: Hsla,

    // Filmstrip, styled after a strip of developed colour negative film:
    // translucent orange-brown base, punched sprocket holes and the amber
    // edge print (frame numbers, stock name) along the rebate.
    pub film_base: Hsla,
    pub film_base_dark: Hsla,
    pub film_sprocket: Hsla,
    pub film_edge_print: Hsla,
    pub film_frame_border: Hsla,

    pub label_red: Hsla,
    pub label_yellow: Hsla,
    pub label_green: Hsla,
    pub label_blue: Hsla,
    pub label_purple: Hsla,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            background: hsla(220.0 / 360.0, 0.08, 0.09, 1.0),
            surface: hsla(220.0 / 360.0, 0.08, 0.12, 1.0),
            surface_elevated: hsla(220.0 / 360.0, 0.08, 0.16, 1.0),
            border: hsla(220.0 / 360.0, 0.08, 0.22, 1.0),
            border_subtle: hsla(220.0 / 360.0, 0.08, 0.17, 1.0),
            text: hsla(0.0, 0.0, 0.92, 1.0),
            text_muted: hsla(220.0 / 360.0, 0.05, 0.65, 1.0),
            text_faint: hsla(220.0 / 360.0, 0.05, 0.45, 1.0),
            accent: hsla(205.0 / 360.0, 0.85, 0.60, 1.0),
            accent_muted: hsla(205.0 / 360.0, 0.4, 0.30, 1.0),
            pick: hsla(140.0 / 360.0, 0.55, 0.50, 1.0),
            reject: hsla(5.0 / 360.0, 0.65, 0.58, 1.0),
            star: hsla(45.0 / 360.0, 0.90, 0.60, 1.0),
            danger: hsla(5.0 / 360.0, 0.70, 0.55, 1.0),
            film_base: hsla(24.0 / 360.0, 0.55, 0.16, 1.0),
            film_base_dark: hsla(20.0 / 360.0, 0.50, 0.09, 1.0),
            film_sprocket: hsla(35.0 / 360.0, 0.30, 0.82, 1.0),
            film_edge_print: hsla(30.0 / 360.0, 0.95, 0.58, 1.0),
            film_frame_border: hsla(20.0 / 360.0, 0.60, 0.05, 1.0),
            label_red: hsla(0.0, 0.70, 0.55, 1.0),
            label_yellow: hsla(48.0 / 360.0, 0.85, 0.55, 1.0),
            label_green: hsla(130.0 / 360.0, 0.50, 0.48, 1.0),
            label_blue: hsla(210.0 / 360.0, 0.75, 0.58, 1.0),
            label_purple: hsla(280.0 / 360.0, 0.50, 0.60, 1.0),
        }
    }

    /// Swatch colour for a colour label; `None` for unlabelled.
    pub fn color_label(&self, label: ColorLabel) -> Option<Hsla> {
        match label {
            ColorLabel::None => None,
            ColorLabel::Red => Some(self.label_red),
            ColorLabel::Yellow => Some(self.label_yellow),
            ColorLabel::Green => Some(self.label_green),
            ColorLabel::Blue => Some(self.label_blue),
            ColorLabel::Purple => Some(self.label_purple),
        }
    }
}

impl Theme {
    /// The palette for the user's appearance settings. `contrast` (0-100,
    /// 50 = standard) scales how far apart the surface layers, borders and
    /// text sit; the theme picks the base lightness and hue.
    pub fn from_appearance(a: &AppearanceSettings) -> Self {
        let mut t = Self::dark();
        let f = a.contrast.min(100) as f32 / 50.0; // 0.0 ..= 2.0, 1.0 standard

        // (hue, saturation, background lightness, light theme?)
        let (hue, sat, bg, light) = match a.theme {
            AppTheme::Dark => (220.0, 0.08, 0.09, false),
            AppTheme::Midnight => (228.0, 0.22, 0.06, false),
            AppTheme::Graphite => (0.0, 0.0, 0.12, false),
            AppTheme::Light => (220.0, 0.10, 0.95, true),
        };
        let dir = if light { -1.0 } else { 1.0 };
        let layer = |step: f32| (bg + dir * step * f).clamp(0.0, 1.0);
        let h = hue / 360.0;

        t.background = hsla(h, sat, bg, 1.0);
        t.surface = hsla(h, sat, layer(0.045), 1.0);
        t.surface_elevated = hsla(h, sat, layer(0.095), 1.0);
        t.border_subtle = hsla(h, sat, layer(0.115), 1.0);
        t.border = hsla(h, sat, layer(0.185), 1.0);

        // Text runs from near-white (or near-black) at full contrast to
        // closer to the background at low contrast.
        let text_l = if light { 0.10 } else { 0.94 };
        let mix = |toward_bg: f32| {
            let l = text_l + (bg - text_l) * toward_bg / f.max(0.5);
            l.clamp(0.0, 1.0)
        };
        t.text = hsla(0.0, 0.0, mix(0.04), 1.0);
        t.text_muted = hsla(h, sat * 0.6, mix(0.30), 1.0);
        t.text_faint = hsla(h, sat * 0.6, mix(0.52), 1.0);

        let accent_h = a.accent.hue() / 360.0;
        let accent_l = if light { 0.45 } else { 0.62 };
        t.accent = hsla(accent_h, 0.85, accent_l, 1.0);
        t.accent_muted = hsla(
            accent_h,
            if light { 0.55 } else { 0.45 },
            if light { 0.80 } else { 0.30 },
            1.0,
        );
        t
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}
