use crate::sp;
use gpui::{div, prelude::*};
use rustyedit_core::Flag;

use crate::theme::Theme;

/// A small read-only glyph badge for a cull flag. Renders nothing for
/// `Flag::Unflagged` so unflagged grid cells stay visually clean.
pub fn flag_badge(flag: Flag, theme: &Theme) -> impl IntoElement {
    let theme = *theme;
    let (glyph, color) = match flag {
        Flag::Unflagged => ("", theme.text_faint),
        Flag::Picked => ("\u{2691}", theme.pick),
        Flag::Rejected => ("\u{2715}", theme.reject),
    };

    div()
        .flex()
        .items_center()
        .justify_center()
        .size(sp(16.0))
        .text_size(sp(12.0))
        .text_color(color)
        .child(glyph)
}
