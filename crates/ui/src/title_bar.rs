use gpui::{div, prelude::*, px, AnyElement, MouseButton, SharedString};

use crate::theme::Theme;

/// Height of the custom title bar, in pixels.
pub const TITLE_BAR_HEIGHT: f32 = 38.0;

/// Left padding reserved for macOS's native traffic-light buttons, which
/// are drawn by the OS on top of our (transparent) system title bar even
/// though the rest of the bar's content is ours to draw.
const MACOS_TRAFFIC_LIGHT_INSET: f32 = 78.0;

/// RustyEdit's custom title bar: a draggable strip that shows the app title
/// plus caller-supplied buttons on the left and right, matching the "title
/// bar can host its own buttons" requirement. The background itself starts
/// a native window move on mouse-down so the window is still draggable by
/// its title bar like a normal app, while children keep their own clicks.
pub fn title_bar(
    theme: &Theme,
    title: impl Into<SharedString>,
    left_buttons: Vec<AnyElement>,
    right_buttons: Vec<AnyElement>,
) -> impl IntoElement {
    let theme = *theme;

    div()
        .id("title-bar")
        .flex()
        .flex_row()
        .items_center()
        .h(px(TITLE_BAR_HEIGHT))
        .w_full()
        .flex_shrink_0()
        .bg(theme.background)
        .border_b_1()
        .border_color(theme.border_subtle)
        .pl(px(if cfg!(target_os = "macos") {
            MACOS_TRAFFIC_LIGHT_INSET
        } else {
            12.0
        }))
        .pr_2()
        .gap_3()
        .on_mouse_down(MouseButton::Left, |_, window, _| {
            window.start_window_move();
        })
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .children(left_buttons),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .justify_center()
                .text_sm()
                .text_color(theme.text_muted)
                .child(title.into()),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .children(right_buttons),
        )
}
