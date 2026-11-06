//! The design tokens every panel draws from: one type scale, one spacing
//! scale and the small text primitives built on them (caption, hint,
//! field, row, divider). Panels should reach for these instead of writing
//! their own sizes, so a heading or hint looks the same everywhere.
//!
//! Sizes are scaled pixels (see [`crate::sp`]), so they follow the
//! interface-scale setting.

use gpui::{div, prelude::*, AnyElement, FontWeight, Pixels, SharedString};

use crate::{sp, Theme};

/// The type scale, in pixels at 100%.
pub mod type_size {
    /// Badges, frame numbers, the smallest legible text.
    pub const MICRO: f32 = 9.0;
    /// Captions and tab headings (UPPER CASE).
    pub const CAPTION: f32 = 10.0;
    /// Hints, secondary values and metadata rows.
    pub const SMALL: f32 = 10.5;
    /// Control labels and body text.
    pub const BODY: f32 = 11.5;
    /// Section titles.
    pub const LABEL: f32 = 12.0;
    /// Headlines inside a panel.
    pub const TITLE: f32 = 13.5;
}

/// The spacing scale, in pixels at 100%: the gap between a caption and
/// its control, between controls, and between groups.
pub mod space {
    pub const TIGHT: f32 = 4.0;
    pub const NORMAL: f32 = 8.0;
    pub const LOOSE: f32 = 12.0;
}

/// A type-scale size as pixels.
pub fn text(size: f32) -> Pixels {
    sp(size)
}

/// A small UPPER-CASE heading over a control or group.
pub fn caption(theme: &Theme, text: impl Into<SharedString>) -> AnyElement {
    div()
        .text_size(sp(type_size::CAPTION))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.text_faint)
        .child(text.into().to_uppercase())
        .into_any_element()
}

/// A line of explanation under a control.
pub fn hint(theme: &Theme, text: impl Into<SharedString>) -> AnyElement {
    div()
        .text_size(sp(type_size::SMALL))
        .line_height(gpui::relative(1.35))
        .text_color(theme.text_faint)
        .child(text.into())
        .into_any_element()
}

/// A control under its caption.
pub fn field(theme: &Theme, label: impl Into<SharedString>, control: AnyElement) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(sp(space::TIGHT))
        .child(caption(theme, label))
        .child(control)
        .into_any_element()
}

/// A label at the left and a control or value at the right.
pub fn row(theme: &Theme, label: impl Into<SharedString>, control: AnyElement) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(sp(space::NORMAL))
        .child(
            div()
                .text_size(sp(type_size::BODY))
                .text_color(theme.text_muted)
                .child(label.into()),
        )
        .child(control)
        .into_any_element()
}

/// A hairline between groups.
pub fn divider(theme: &Theme) -> AnyElement {
    div()
        .h(sp(1.0))
        .w_full()
        .bg(theme.border_subtle)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_type_scale_only_goes_up() {
        let scale = [
            type_size::MICRO,
            type_size::CAPTION,
            type_size::SMALL,
            type_size::BODY,
            type_size::LABEL,
            type_size::TITLE,
        ];
        assert!(scale.windows(2).all(|w| w[0] < w[1]), "{scale:?}");
    }
}
