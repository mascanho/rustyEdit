use gpui::{hsla, Hsla};
use rustyedit_core::settings::FilmstripStyle;

use crate::theme::Theme;

/// Everything that varies between filmstrip styles: palette plus the
/// structural pieces (sprocket holes, edge print, frame lines, print
/// borders). The filmstrip and its Settings previews both render purely
/// from this, so a new style is just a new constructor here.
#[derive(Debug, Clone, Copy)]
pub struct FilmLook {
    /// Background of the strip, as a vertical gradient.
    pub base_top: Hsla,
    pub base_bottom: Hsla,

    /// Header bar and arrow buttons.
    pub chrome_bg: Hsla,
    pub chrome_hover: Hsla,
    pub chrome_text: Hsla,
    pub divider: Hsla,
    pub title: &'static str,
    /// Use a monospaced font for the header and edge print.
    pub monospace: bool,

    pub sprockets: Option<Sprockets>,
    pub edge_print: Option<EdgePrint>,
    /// A thin line between frames, as on motion picture film.
    pub frame_line: Option<Hsla>,
    /// A border the picture sits in, like an instant print.
    pub mat: Option<Hsla>,

    pub picture_inset: f32,
    pub picture_radius: f32,
    pub picture_border: Hsla,
    /// Render thumbnails in grayscale.
    pub monochrome: bool,

    /// Picture border on the selected frame.
    pub selected: Hsla,
    pub frame_selected_bg: Hsla,
    pub frame_hover_bg: Hsla,

    pub meta_text: Hsla,
    pub star_filled: Hsla,
    pub star_empty: Hsla,
}

/// Perforations along the top and bottom of each frame.
#[derive(Debug, Clone, Copy)]
pub struct Sprockets {
    pub color: Hsla,
    pub per_frame: usize,
    pub width: f32,
    pub height: f32,
    pub radius: f32,
}

/// Text printed along the film's edge above each frame.
#[derive(Debug, Clone, Copy)]
pub struct EdgePrint {
    pub color: Hsla,
    pub selected_color: Hsla,
    /// Left and right text for a (1-based) frame number.
    pub label: fn(usize) -> (String, String),
}

fn hsl(h: f32, s: f32, l: f32) -> Hsla {
    hsla(h / 360.0, s, l, 1.0)
}

impl FilmLook {
    pub fn for_style(style: FilmstripStyle, theme: &Theme) -> Self {
        match style {
            FilmstripStyle::ColorNegative => Self::color_negative(theme),
            FilmstripStyle::BlackAndWhite => Self::black_and_white(theme),
            FilmstripStyle::Slide => Self::slide(theme),
            FilmstripStyle::Cinema => Self::cinema(theme),
            FilmstripStyle::Instant => Self::instant(theme),
            FilmstripStyle::Minimal => Self::minimal(theme),
        }
    }

    /// Developed C-41 colour negative: translucent orange-brown base,
    /// rounded perforations and amber edge print.
    fn color_negative(theme: &Theme) -> Self {
        Self {
            base_top: theme.film_base_dark,
            base_bottom: theme.film_base,
            chrome_bg: theme.film_base_dark,
            chrome_hover: theme.film_base,
            chrome_text: theme.film_edge_print,
            divider: theme.film_frame_border,
            title: "FILMSTRIP \u{00B7} RUSTY 400",
            monospace: true,
            sprockets: Some(Sprockets {
                color: theme.film_sprocket.opacity(0.85),
                per_frame: 6,
                width: 9.0,
                height: 7.0,
                radius: 2.0,
            }),
            edge_print: Some(EdgePrint {
                color: theme.film_edge_print,
                selected_color: theme.film_sprocket,
                label: |n| {
                    let right = if n % 4 == 1 {
                        "RUSTY 400".to_string()
                    } else {
                        format!("{n}A")
                    };
                    (format!("\u{25B8} {n}"), right)
                },
            }),
            frame_line: None,
            mat: None,
            picture_inset: 6.0,
            picture_radius: 2.0,
            picture_border: theme.film_frame_border,
            monochrome: false,
            selected: theme.accent,
            frame_selected_bg: theme.film_base.opacity(0.6),
            frame_hover_bg: theme.film_base,
            meta_text: theme.film_edge_print,
            star_filled: theme.star,
            star_empty: theme.film_edge_print.opacity(0.3),
        }
    }

    /// Black & white negative: neutral grey base, bright white edge print,
    /// and grayscale previews.
    fn black_and_white(_theme: &Theme) -> Self {
        Self {
            base_top: hsl(0.0, 0.0, 0.08),
            base_bottom: hsl(0.0, 0.0, 0.16),
            chrome_bg: hsl(0.0, 0.0, 0.06),
            chrome_hover: hsl(0.0, 0.0, 0.14),
            chrome_text: hsl(0.0, 0.0, 0.88),
            divider: hsl(0.0, 0.0, 0.03),
            title: "FILMSTRIP \u{00B7} RUSTY PAN 400",
            monospace: true,
            sprockets: Some(Sprockets {
                color: hsl(0.0, 0.0, 0.82).opacity(0.9),
                per_frame: 6,
                width: 9.0,
                height: 7.0,
                radius: 2.0,
            }),
            edge_print: Some(EdgePrint {
                color: hsl(0.0, 0.0, 0.9),
                selected_color: hsl(0.0, 0.0, 1.0),
                label: |n| {
                    let right = if n % 4 == 1 {
                        "RUSTY PAN 400".to_string()
                    } else {
                        format!("{n}A")
                    };
                    (format!("\u{25B8} {n}"), right)
                },
            }),
            frame_line: None,
            mat: None,
            picture_inset: 6.0,
            picture_radius: 2.0,
            picture_border: hsl(0.0, 0.0, 0.03),
            monochrome: true,
            selected: hsl(0.0, 0.0, 0.96),
            frame_selected_bg: hsl(0.0, 0.0, 0.22),
            frame_hover_bg: hsl(0.0, 0.0, 0.19),
            meta_text: hsl(0.0, 0.0, 0.85),
            star_filled: hsl(0.0, 0.0, 0.96),
            star_empty: hsl(0.0, 0.0, 0.6).opacity(0.35),
        }
    }

    /// E-6 slide film: near-black base with cool green edge print.
    fn slide(theme: &Theme) -> Self {
        let edge = hsl(150.0, 0.55, 0.55);
        Self {
            base_top: hsl(220.0, 0.1, 0.03),
            base_bottom: hsl(220.0, 0.12, 0.08),
            chrome_bg: hsl(220.0, 0.1, 0.03),
            chrome_hover: hsl(220.0, 0.12, 0.11),
            chrome_text: edge,
            divider: hsl(0.0, 0.0, 0.0),
            title: "FILMSTRIP \u{00B7} RUSTYCHROME 100",
            monospace: true,
            sprockets: Some(Sprockets {
                color: hsl(210.0, 0.08, 0.6).opacity(0.75),
                per_frame: 6,
                width: 9.0,
                height: 7.0,
                radius: 2.0,
            }),
            edge_print: Some(EdgePrint {
                color: edge,
                selected_color: hsl(150.0, 0.7, 0.75),
                label: |n| {
                    let right = if n % 4 == 1 {
                        "RCH 100".to_string()
                    } else {
                        format!("{n:02}")
                    };
                    (format!("\u{25B2} {n}"), right)
                },
            }),
            frame_line: None,
            mat: None,
            picture_inset: 6.0,
            picture_radius: 1.0,
            picture_border: hsl(0.0, 0.0, 0.0),
            monochrome: false,
            selected: theme.accent,
            frame_selected_bg: hsl(220.0, 0.15, 0.13),
            frame_hover_bg: hsl(220.0, 0.12, 0.11),
            meta_text: edge,
            star_filled: theme.star,
            star_empty: edge.opacity(0.3),
        }
    }

    /// 35mm motion picture print: dark base, four square-ish perforations
    /// per frame, frame lines between frames and key-code edge print.
    fn cinema(theme: &Theme) -> Self {
        let edge = hsl(42.0, 0.9, 0.55);
        Self {
            base_top: hsl(30.0, 0.25, 0.05),
            base_bottom: hsl(30.0, 0.2, 0.1),
            chrome_bg: hsl(30.0, 0.25, 0.04),
            chrome_hover: hsl(30.0, 0.2, 0.12),
            chrome_text: edge,
            divider: hsl(0.0, 0.0, 0.0),
            title: "REEL 1 \u{00B7} RUSTY 5219",
            monospace: true,
            sprockets: Some(Sprockets {
                color: hsl(40.0, 0.15, 0.78).opacity(0.85),
                per_frame: 4,
                width: 8.0,
                height: 9.0,
                radius: 1.0,
            }),
            edge_print: Some(EdgePrint {
                color: edge,
                selected_color: hsl(42.0, 1.0, 0.75),
                label: |n| {
                    let right = if n % 4 == 1 {
                        "RE 5219".to_string()
                    } else {
                        format!("+{:02}", n % 16)
                    };
                    (format!("KX {:06}", n * 16), right)
                },
            }),
            frame_line: Some(hsl(0.0, 0.0, 0.0)),
            mat: None,
            picture_inset: 4.0,
            picture_radius: 3.0,
            picture_border: hsl(0.0, 0.0, 0.0),
            monochrome: false,
            selected: edge,
            frame_selected_bg: hsl(30.0, 0.25, 0.14),
            frame_hover_bg: hsl(30.0, 0.2, 0.12),
            meta_text: edge,
            star_filled: theme.star,
            star_empty: edge.opacity(0.3),
        }
    }

    /// Instant prints scattered along a dark desk: thick white borders,
    /// no perforations, rating written on the print's chin.
    fn instant(theme: &Theme) -> Self {
        Self {
            base_top: hsl(28.0, 0.2, 0.13),
            base_bottom: hsl(28.0, 0.18, 0.19),
            chrome_bg: hsl(28.0, 0.2, 0.1),
            chrome_hover: hsl(28.0, 0.2, 0.17),
            chrome_text: hsl(40.0, 0.3, 0.85),
            divider: hsl(28.0, 0.25, 0.07),
            title: "Prints",
            monospace: false,
            sprockets: None,
            edge_print: None,
            frame_line: None,
            mat: Some(hsl(40.0, 0.3, 0.94)),
            picture_inset: 0.0,
            picture_radius: 0.0,
            picture_border: hsl(40.0, 0.2, 0.85),
            monochrome: false,
            selected: theme.accent,
            frame_selected_bg: hsl(28.0, 0.2, 0.25),
            frame_hover_bg: hsl(28.0, 0.2, 0.22),
            meta_text: hsl(0.0, 0.0, 0.25),
            star_filled: hsl(35.0, 0.85, 0.48),
            star_empty: hsl(0.0, 0.0, 0.3).opacity(0.3),
        }
    }

    /// No film at all: rounded thumbnails on the app's own surfaces.
    fn minimal(theme: &Theme) -> Self {
        Self {
            base_top: theme.surface,
            base_bottom: theme.surface,
            chrome_bg: theme.background,
            chrome_hover: theme.surface_elevated,
            chrome_text: theme.text_muted,
            divider: theme.border_subtle,
            title: "Filmstrip",
            monospace: false,
            sprockets: None,
            edge_print: None,
            frame_line: None,
            mat: None,
            picture_inset: 5.0,
            picture_radius: 6.0,
            picture_border: theme.surface,
            monochrome: false,
            selected: theme.accent,
            frame_selected_bg: theme.surface_elevated,
            frame_hover_bg: theme.surface_elevated.opacity(0.6),
            meta_text: theme.text_muted,
            star_filled: theme.star,
            star_empty: theme.text_faint.opacity(0.5),
        }
    }
}
