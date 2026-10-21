use gpui::{div, prelude::*, px, App, ClickEvent, ElementId, SharedString, Window};

use crate::{button::chrome_button, theme::Theme};

/// An on/off switch, as used for boolean settings.
pub fn toggle_switch(
    id: impl Into<ElementId>,
    on: bool,
    theme: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let theme = *theme;

    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .flex_shrink_0()
        .w(px(32.0))
        .h(px(18.0))
        .p(px(2.0))
        .rounded_full()
        .cursor_pointer()
        .bg(if on { theme.accent } else { theme.border })
        .when(on, |el| el.justify_end())
        .child(div().size(px(14.0)).rounded_full().bg(theme.text))
        .on_click(on_click)
}

/// A smaller [`toggle_switch`], for section title bars.
pub fn mini_toggle_switch(
    id: impl Into<ElementId>,
    on: bool,
    theme: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let theme = *theme;

    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .flex_shrink_0()
        .w(px(20.0))
        .h(px(12.0))
        .p(px(1.5))
        .rounded_full()
        .cursor_pointer()
        .bg(if on { theme.accent } else { theme.border })
        .when(on, |el| el.justify_end())
        .child(div().size(px(8.0)).rounded_full().bg(theme.text))
        .on_click(on_click)
}

/// A compact "− value +" integer editor moving in increments of `step`,
/// with an optional unit suffix (e.g. "px"). Buttons dim at the bounds.
#[allow(clippy::too_many_arguments)]
pub fn stepper(
    id: impl Into<SharedString>,
    value: i64,
    min: i64,
    max: i64,
    step: i64,
    unit: &'static str,
    theme: &Theme,
    on_change: impl Fn(i64, &mut Window, &mut App) + Clone + 'static,
) -> impl IntoElement {
    let theme = *theme;
    let id: SharedString = id.into();
    let step_button = |delta: i64, enabled: bool| {
        let on_change = on_change.clone();
        let label = if delta < 0 { "\u{2212}" } else { "+" };
        div()
            .when(!enabled, |el| el.opacity(0.35))
            .child(chrome_button(
                SharedString::from(format!("{id}-{}", if delta < 0 { "dec" } else { "inc" })),
                label,
                false,
                &theme,
                move |_, window, cx| {
                    if enabled {
                        on_change((value + delta * step).clamp(min, max), window, cx);
                    }
                },
            ))
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .flex_shrink_0()
        .border_1()
        .border_color(theme.border)
        .child(step_button(-1, value > min))
        .child(
            div()
                .min_w(px(28.0))
                .px_1()
                .flex()
                .justify_center()
                .text_sm()
                .text_color(theme.text)
                .child(if unit.is_empty() {
                    value.to_string()
                } else {
                    format!("{value} {unit}")
                }),
        )
        .child(step_button(1, value < max))
}
