use gpui::{
    div, prelude::*, px, AnyElement, App, ClickEvent, ElementId, ScrollHandle, SharedString, Window,
};

use crate::{button::glyph_button, scrollbar::vertical_scrollbar, theme::Theme};

pub const COLLAPSED_PANEL_WIDTH: f32 = 28.0;

/// Which edge of the window a collapsible panel is docked to. Decides the
/// border side and which way the collapse/expand chevrons point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelSide {
    Left,
    Right,
}

/// A titled group of controls inside the Develop side panel (e.g. "Basic",
/// "Color", "Detail"), mirroring Lightroom's stacked, titled panel sections.
pub fn section(
    theme: &Theme,
    title: impl Into<SharedString>,
    children: Vec<AnyElement>,
) -> impl IntoElement {
    let theme = *theme;

    div()
        .flex()
        .flex_col()
        .w_full()
        .border_b_1()
        .border_color(theme.border_subtle)
        .py_3()
        .gap_2()
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text_muted)
                .child(title.into()),
        )
        .child(div().flex().flex_col().gap_2().children(children))
}

/// A section whose title row expands/collapses its contents, like the
/// stacked panels in Lightroom's side panels. The caller owns the
/// `expanded` state and flips it in `on_toggle`.
pub fn disclosure_section(
    id: impl Into<ElementId>,
    theme: &Theme,
    title: impl Into<SharedString>,
    expanded: bool,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    children: Vec<AnyElement>,
) -> impl IntoElement {
    let theme = *theme;

    div()
        .flex()
        .flex_col()
        .w_full()
        .border_b_1()
        .border_color(theme.border_subtle)
        .child(
            div()
                .id(id)
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .py_2()
                .cursor_pointer()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text_muted)
                .hover(|el| el.text_color(theme.text))
                .child(
                    div()
                        .w(px(10.0))
                        .child(if expanded { "\u{25BE}" } else { "\u{25B8}" }),
                )
                .child(title.into())
                .on_click(on_toggle),
        )
        .when(expanded, |el| {
            el.child(div().flex().flex_col().gap_2().pb_3().children(children))
        })
}

/// A vertically scrollable container for stacking develop panel sections,
/// docked to `side` at a fixed `width`, with a minimal scrollbar driven by
/// `scroll`.
pub fn side_panel(
    theme: &Theme,
    side: PanelSide,
    width: f32,
    scroll: &ScrollHandle,
    children: Vec<AnyElement>,
) -> impl IntoElement {
    let theme = *theme;

    div()
        .relative()
        .flex()
        .flex_col()
        .w(px(width))
        .h_full()
        .flex_shrink_0()
        .bg(theme.surface)
        .border_color(theme.border_subtle)
        .when(side == PanelSide::Left, |el| el.border_r_1())
        .when(side == PanelSide::Right, |el| el.border_l_1())
        .child(
            div()
                .id("develop-side-panel")
                .flex()
                .flex_col()
                .size_full()
                .min_w(px(0.0))
                .overflow_y_scroll()
                .track_scroll(scroll)
                .px_3()
                .children(children),
        )
        .child(vertical_scrollbar(
            "develop-side-panel-scrollbar",
            scroll.clone(),
            &theme,
        ))
}

/// A docked side panel (Library's left/right panels) that can be collapsed
/// down to a thin strip holding only its expand chevron, like Lightroom's
/// panel groups.
pub fn collapsible_panel(
    id: impl Into<ElementId>,
    theme: &Theme,
    title: impl Into<SharedString>,
    side: PanelSide,
    width: f32,
    collapsed: bool,
    scroll: &ScrollHandle,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    children: Vec<AnyElement>,
) -> impl IntoElement {
    let theme = *theme;
    let id = id.into();

    // Chevron points the way the panel will move when clicked.
    let glyph = match (side, collapsed) {
        (PanelSide::Left, false) | (PanelSide::Right, true) => "\u{2039}",
        (PanelSide::Left, true) | (PanelSide::Right, false) => "\u{203A}",
    };
    let toggle = glyph_button(
        ElementId::NamedChild(Box::new(id.clone()), "toggle".into()),
        glyph,
        if collapsed {
            "Expand panel"
        } else {
            "Collapse panel"
        },
        theme.text_muted,
        true,
        &theme,
        on_toggle,
    );

    let container = div()
        .flex()
        .flex_col()
        .h_full()
        .flex_shrink_0()
        .bg(theme.surface)
        .border_color(theme.border_subtle)
        .when(side == PanelSide::Left, |el| el.border_r_1())
        .when(side == PanelSide::Right, |el| el.border_l_1());

    if collapsed {
        return container
            .w(px(COLLAPSED_PANEL_WIDTH))
            .items_center()
            .pt_1()
            .child(toggle)
            .into_any_element();
    }

    let header = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .flex_shrink_0()
        .h(px(28.0))
        .px_2()
        .border_b_1()
        .border_color(theme.border_subtle)
        .when(side == PanelSide::Right, |el| el.flex_row_reverse())
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text_muted)
                .child(title.into()),
        )
        .child(toggle);

    let scrollbar_id = SharedString::from(format!("{id:?}-scrollbar"));

    container
        .w(px(width))
        .child(header)
        .child(
            div()
                .relative()
                .flex_1()
                .w_full()
                .min_w(px(0.0))
                .min_h(px(0.0))
                .child(
                    div()
                        .id(id)
                        .flex()
                        .flex_col()
                        .size_full()
                        .min_w(px(0.0))
                        .overflow_y_scroll()
                        .track_scroll(scroll)
                        .px_3()
                        .children(children),
                )
                .child(vertical_scrollbar(scrollbar_id, scroll.clone(), &theme)),
        )
        .into_any_element()
}
