use gpui::{
    div, prelude::*, px, AnyElement, App, ClickEvent, ElementId, Hsla, SharedString, Window,
};

use crate::theme::Theme;

/// Height of the quick-cull bar under the photo.
pub const CULL_BAR_HEIGHT: f32 = 52.0;

/// The bar under the loupe/grid: `left` and `right` hug the edges while
/// `center` stays truly centred regardless of how wide the sides get.
pub fn cull_bar(
    theme: &Theme,
    left: Vec<AnyElement>,
    center: Vec<AnyElement>,
    right: Vec<AnyElement>,
) -> impl IntoElement {
    let theme = *theme;
    let side = |justify_end: bool, children: Vec<AnyElement>| {
        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_w(px(0.0))
            .overflow_hidden()
            .items_center()
            .gap_3()
            .when(justify_end, |el| el.justify_end())
            .children(children)
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_4()
        .w_full()
        .h(px(CULL_BAR_HEIGHT))
        .flex_shrink_0()
        .px_4()
        .bg(theme.surface)
        .border_t_1()
        .border_color(theme.border_subtle)
        .text_xs()
        .text_color(theme.text_muted)
        .child(side(false, left))
        .child(
            div()
                .flex()
                .flex_row()
                .flex_shrink_0()
                .items_center()
                .gap_3()
                .children(center),
        )
        .child(side(true, right))
}

/// A rounded "pill" holding related controls, so the bar reads as a few
/// distinct clusters instead of one long row.
pub fn cull_group(theme: &Theme, children: Vec<AnyElement>) -> impl IntoElement {
    let theme = *theme;
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .h(px(36.0))
        .px_1()
        .rounded_xl()
        .bg(theme.background)
        .border_1()
        .border_color(theme.border_subtle)
        .children(children)
}

/// A glyph + label action (Reject / Unflag / Pick). When `active` it fills
/// with a tint of `color`; otherwise it only lights up on hover.
pub fn cull_button(
    id: impl Into<ElementId>,
    glyph: impl Into<SharedString>,
    label: impl Into<SharedString>,
    tooltip: impl Into<SharedString>,
    color: Hsla,
    active: bool,
    theme: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let theme = *theme;
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .h(px(28.0))
        .px_3()
        .rounded_lg()
        .text_sm()
        .cursor_pointer()
        .when(active, |el| {
            el.bg(color.opacity(0.22))
                .text_color(color)
                .border_1()
                .border_color(color.opacity(0.55))
        })
        .when(!active, |el| {
            el.text_color(theme.text_muted)
                .border_1()
                .border_color(gpui::transparent_black())
                .hover(|el| el.bg(theme.surface_elevated).text_color(color))
        })
        .child(div().text_size(px(14.0)).child(glyph.into()))
        .child(label.into())
        .tooltip(crate::tooltip::tip(tooltip, &theme))
        .on_click(on_click)
}

/// A round colour-label swatch; the active one gets a ring.
pub fn label_swatch(
    id: impl Into<ElementId>,
    color: Hsla,
    tooltip: impl Into<SharedString>,
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
        .size(px(24.0))
        .rounded_full()
        .cursor_pointer()
        .border_2()
        .border_color(if active {
            theme.text
        } else {
            gpui::transparent_black()
        })
        .hover(|el| el.bg(theme.surface_elevated))
        .child(div().size(px(12.0)).rounded_full().bg(color))
        .tooltip(crate::tooltip::tip(tooltip, &theme))
        .on_click(on_click)
}

/// A square glyph button, used for previous / next.
pub fn cull_nav_button(
    id: impl Into<ElementId>,
    glyph: impl Into<SharedString>,
    tooltip: impl Into<SharedString>,
    enabled: bool,
    theme: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let theme = *theme;
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(28.0))
        .rounded_lg()
        .text_size(px(16.0))
        .tooltip(crate::tooltip::tip(tooltip, &theme))
        .when(enabled, |el| {
            el.cursor_pointer()
                .text_color(theme.text_muted)
                .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                .on_click(on_click)
        })
        .when(!enabled, |el| el.text_color(theme.text_faint.opacity(0.5)))
        .child(glyph.into())
}
