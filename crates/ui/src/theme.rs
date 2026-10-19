use gpui::{hsla, Hsla};
use rustyedit_core::ColorLabel;

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

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}
