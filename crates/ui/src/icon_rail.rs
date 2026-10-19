use gpui::{div, prelude::*, px, App, ClickEvent, ElementId, SharedString, Window};

use crate::{panel::PanelSide, theme::Theme};

/// Width of the rail, including its separator border.
pub const RAIL_WIDTH: f32 = 36.0;
const BUTTON_SIZE: f32 = 28.0;

/// One icon in a rail. Plain data plus a callback, so the rail knows
/// nothing about which tools exist.
pub struct RailButton {
    pub id: SharedString,
    pub glyph: SharedString,
    pub tooltip: SharedString,
    pub active: bool,
    pub on_click: Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
}

/// A thin vertical bar of icon buttons that sits on a dock's inner edge
/// (the side facing the photo). `side` is the side of the dock it belongs
/// to: a left dock's rail draws its separator on the right, and vice versa.
pub fn icon_rail(
    id: impl Into<ElementId>,
    theme: &Theme,
    side: PanelSide,
    buttons: Vec<RailButton>,
) -> impl IntoElement {
    let theme = *theme;

    div()
        .id(id)
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .pt_2()
        .w(px(RAIL_WIDTH))
        .h_full()
        .flex_shrink_0()
        .bg(theme.surface)
        .border_color(theme.border_subtle)
        .when(side == PanelSide::Left, |el| el.border_r_1())
        .when(side == PanelSide::Right, |el| el.border_l_1())
        .children(buttons.into_iter().map(|button| {
            div()
                .id(ElementId::from(button.id))
                .flex()
                .items_center()
                .justify_center()
                .size(px(BUTTON_SIZE))
                .rounded_md()
                .text_size(px(15.0))
                .cursor_pointer()
                .when(button.active, |el| {
                    el.bg(theme.accent_muted).text_color(theme.text)
                })
                .when(!button.active, |el| {
                    el.text_color(theme.text_muted)
                        .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                })
                .child(button.glyph)
                .tooltip(crate::tooltip::tip(button.tooltip, &theme))
                .on_click(button.on_click)
        }))
}
