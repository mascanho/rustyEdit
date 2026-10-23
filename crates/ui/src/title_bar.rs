use crate::sp;
use gpui::{
    div, prelude::*, px, AnyElement, App, ClickEvent, MouseButton, SharedString, Window,
    WindowControlArea,
};

use crate::theme::Theme;

gpui::actions!(title_bar, [
    /// Sent by the close caption button on Linux, where nothing native
    /// handles it; the app answers it (and may ask before closing).
    CloseWindow
]);

/// Height of the custom title bar, in pixels.
pub const TITLE_BAR_HEIGHT: f32 = 38.0;

/// Left padding reserved for macOS's native traffic-light buttons, which
/// are drawn by the OS on top of our (transparent) system title bar even
/// though the rest of the bar's content is ours to draw.
const MACOS_TRAFFIC_LIGHT_INSET: f32 = 78.0;

/// Width of each minimise / maximise / close button on Windows and Linux,
/// where the system title bar (and so its buttons) is hidden.
const CAPTION_BUTTON_WIDTH: f32 = 46.0;

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
        .when(cfg!(target_os = "macos"), |bar| bar.pr_2())
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
        .when(!cfg!(target_os = "macos"), |bar| bar.child(caption_buttons(&theme)))
}

/// Minimise, maximise / restore and close, for Windows and Linux (macOS
/// keeps its traffic lights). On Windows each is marked as that window
/// control, so the OS handles it, including the snap-layouts flyout on
/// maximise; on Linux the click does it.
fn caption_buttons(theme: &Theme) -> impl IntoElement {
    let theme = *theme;
    let button = |id: &'static str,
                  area: WindowControlArea,
                  danger: bool,
                  glyph: AnyElement,
                  action: fn(&mut Window, &mut App)| {
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .w(px(CAPTION_BUTTON_WIDTH))
            .h_full()
            .text_color(theme.text_muted)
            .window_control_area(area)
            .hover(move |el| {
                if danger {
                    el.bg(gpui::rgb(0xc42b1c)).text_color(gpui::white())
                } else {
                    el.bg(theme.surface_elevated).text_color(theme.text)
                }
            })
            .child(glyph)
            // On Windows the OS acts on the control area at mouse-up and gpui
            // still delivers the click, so handling it too would act twice
            // (maximise then immediately restore).
            .when(!cfg!(target_os = "windows"), |el| {
                el.on_click(move |_: &ClickEvent, window, cx| action(window, cx))
            })
    };
    let glyph = |text: &'static str| div().text_size(sp(12.0)).child(text).into_any_element();
    div()
        .id("title-bar-caption")
        // Like the button groups: these aren't part of the drag area.
        .occlude()
        .flex()
        .flex_row()
        .h_full()
        .ml_2()
        .child(button(
            "caption-minimize",
            WindowControlArea::Min,
            false,
            glyph("\u{2014}"),
            |window, _| window.minimize_window(),
        ))
        .child(button(
            "caption-maximize",
            WindowControlArea::Max,
            false,
            div()
                .size(sp(10.0))
                .border_1()
                .border_color(theme.text_muted)
                .into_any_element(),
            |window, _| window.zoom_window(),
        ))
        .child(button(
            "caption-close",
            WindowControlArea::Close,
            true,
            glyph("\u{2715}"),
            // The app decides (it asks first), so hand it an action.
            |window, cx| window.dispatch_action(Box::new(CloseWindow), cx),
        ))
}
