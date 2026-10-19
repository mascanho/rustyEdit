use gpui::{div, prelude::*, px, App, ClickEvent, ElementId, SharedString, Window};

use crate::theme::Theme;

/// A small, chrome-style text/icon button used in the title bar and status
/// bar (import, grid/loupe toggle, mode switch, etc). Deliberately plain —
/// no icon font is bundled, so labels are short text or a unicode glyph.
pub fn chrome_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    theme: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    chrome_icon_button(id, label, None, selected, theme, on_click)
}

/// A chrome button for a glyph with no text label, so it carries a tooltip.
pub fn chrome_icon_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    tooltip: Option<SharedString>,
    selected: bool,
    theme: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let theme = *theme;
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .h(px(24.0))
        .px_2()
        .rounded_md()
        .text_sm()
        .cursor_pointer()
        .when(selected, |el| {
            el.bg(theme.accent_muted).text_color(theme.text)
        })
        .when(!selected, |el| {
            el.text_color(theme.text_muted)
                .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
        })
        .child(label.into())
        .when_some(tooltip, |el, text| {
            el.tooltip(crate::tooltip::tip(text, &theme))
        })
        .on_click(on_click)
}

/// A tiny glyph-only button (flag/star/filter-type controls) sized to sit
/// comfortably in the thin bottom status bar.
pub fn glyph_button(
    id: impl Into<ElementId>,
    glyph: impl Into<SharedString>,
    tooltip: impl Into<SharedString>,
    color: gpui::Hsla,
    active: bool,
    theme: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let theme = *theme;
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(20.0))
        .rounded_sm()
        .cursor_pointer()
        .text_color(if active { color } else { theme.text_faint })
        .hover(|el| el.bg(theme.surface_elevated))
        .child(glyph.into())
        .tooltip(crate::tooltip::tip(tooltip, &theme))
        .on_click(on_click)
}
