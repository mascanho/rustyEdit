use gpui::{
    div, prelude::*, px, AnyElement, App, ClickEvent, ElementId, ScrollHandle, SharedString, Window,
};

use crate::{
    icons::{icon, Icon},
    scrollbar::vertical_scrollbar,
    theme::Theme,
    tooltip::tip,
};

pub const COLLAPSED_PANEL_WIDTH: f32 = 28.0;
/// Height of a docked panel's title row. The icon rail beside it starts
/// with a button of the same height, so the two line up.
pub const PANEL_HEADER_HEIGHT: f32 = 28.0;

/// Which edge of the window a collapsible panel is docked to. Decides the
/// border side and which way the collapse/expand chevrons point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelSide {
    Left,
    Right,
}

/// Height of a section's title bar.
const SECTION_HEADER_HEIGHT: f32 = 30.0;
/// Size of the disclosure chevron.
const CHEVRON_SIZE: f32 = 12.0;

/// A section's title bar: a full-width band so each section reads as its
/// own block. `chevron` is `Some(expanded)` for collapsible sections.
fn section_header(theme: &Theme, title: SharedString, chevron: Option<bool>) -> gpui::Div {
    let open = chevron.unwrap_or(true);
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .h(px(SECTION_HEADER_HEIGHT))
        .px_2()
        .rounded_md()
        .text_size(px(12.0))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .when(open, |el| {
            el.bg(theme.surface_elevated.opacity(0.55))
                .text_color(theme.text)
        })
        .when(!open, |el| el.text_color(theme.text_muted))
        .when_some(chevron, |el, expanded| {
            el.child(icon(
                if expanded {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                },
                CHEVRON_SIZE,
                if expanded {
                    theme.accent
                } else {
                    theme.text_muted
                },
            ))
        })
        .child(title)
}

/// A titled group of controls in a side panel (e.g. Library's "Info"),
/// styled like a [`disclosure_section`] that can't be collapsed.
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
        .pt_2()
        .child(section_header(&theme, title.into(), None))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .px_1()
                .pt_2()
                .pb_3()
                .children(children),
        )
}

/// A section whose title bar expands/collapses its contents, like the
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
        .pt_2()
        .child(
            section_header(&theme, title.into(), Some(expanded))
                .id(id)
                .cursor_pointer()
                .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                .on_click(on_toggle),
        )
        .when(expanded, |el| {
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .px_1()
                    .pt_2()
                    .pb_3()
                    .children(children),
            )
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
    let chevron = match (side, collapsed) {
        (PanelSide::Left, false) | (PanelSide::Right, true) => Icon::ChevronLeft,
        (PanelSide::Left, true) | (PanelSide::Right, false) => Icon::ChevronRight,
    };
    let toggle = div()
        .id(ElementId::NamedChild(Box::new(id.clone()), "toggle".into()))
        .flex()
        .items_center()
        .justify_center()
        .size(px(PANEL_HEADER_HEIGHT - 4.0))
        .rounded_md()
        .cursor_pointer()
        .hover(|el| el.bg(theme.surface_elevated))
        .child(icon(chevron, 14.0, theme.text_muted))
        .tooltip(tip(
            if collapsed {
                "Expand panel"
            } else {
                "Collapse panel"
            },
            &theme,
        ))
        .on_click(on_toggle);

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
            .pt(px(2.0))
            .child(toggle)
            .into_any_element();
    }

    let header = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .flex_shrink_0()
        .h(px(PANEL_HEADER_HEIGHT))
        .px_1()
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
