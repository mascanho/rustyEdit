use gpui::{div, prelude::*, px, AnyElement, MouseButton, SharedString, WindowControlArea};

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
        // Windows has no imperative "start a move" call; instead the OS asks
        // the window what's under the cursor, and gpui answers HTCAPTION for
        // a `Drag` control area. That also gives the native double-click to
        // maximise and Aero Snap for free.
        //
        // gpui first replays that native mouse-down through the element tree
        // and swallows it if anything handles it, and a focus-tracking parent
        // (the workspace root) always does. Occluding the bar hides it from
        // everything behind, so the press falls through to the OS drag.
        .when(cfg!(target_os = "windows"), |bar| {
            bar.occlude().window_control_area(WindowControlArea::Drag)
        })
        .when(!cfg!(target_os = "windows"), |bar| {
            bar.on_mouse_down(MouseButton::Left, |_, window, _| {
                window.start_window_move();
            })
        })
        .child(
            // The button groups occlude the drag area behind them, otherwise
            // Windows would treat clicks on them as title-bar drags.
            div()
                .id("title-bar-left")
                .occlude()
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
                .id("title-bar-right")
                .occlude()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .children(right_buttons),
        )
}
