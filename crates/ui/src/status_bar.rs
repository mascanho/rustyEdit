use gpui::{div, prelude::*, px, AnyElement};

use crate::theme::Theme;

/// Height of the bottom status bar, in pixels — thin and informational,
/// matching the proportions of Zed's own status bar rather than a chunky
/// toolbar.
pub const STATUS_BAR_HEIGHT: f32 = 28.0;

/// A thin bottom bar with a left-aligned and right-aligned section,
/// separated by a flexible gap. Used for catalog/selection info on the left
/// and quick actions (zoom, filmstrip toggle, filter) on the right.
pub fn status_bar(
    theme: &Theme,
    left: Vec<AnyElement>,
    right: Vec<AnyElement>,
) -> impl IntoElement {
    let theme = *theme;

    div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(STATUS_BAR_HEIGHT))
        .w_full()
        .flex_shrink_0()
        .bg(theme.surface)
        .border_t_1()
        .border_color(theme.border_subtle)
        .px_3()
        .gap_3()
        .text_xs()
        .text_color(theme.text_muted)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .children(left),
        )
        .child(div().flex_1())
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .children(right),
        )
}

/// A small vertical divider pipe used to separate groups of controls within
/// the status bar, matching Zed's visual language.
pub fn status_divider(theme: &Theme) -> impl IntoElement {
    div().w(px(1.0)).h(px(14.0)).bg(theme.border)
}
