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
    disclosure_section_with(
        id,
        theme,
        title,
        expanded,
        on_toggle,
        SectionExtras::default(),
        children,
    )
}

/// Optional parts of a section's title bar and body.
#[derive(Default)]
pub struct SectionExtras {
    /// Controls at the right end of the title bar (a reset button, an
    /// on/off switch). Clicks on them don't fold the section.
    pub trailing: Vec<AnyElement>,
    /// Fade the body, e.g. while the section is switched off.
    pub dimmed: bool,
}

/// [`disclosure_section`] with extra title-bar controls and a dimmed state.
pub fn disclosure_section_with(
    id: impl Into<ElementId>,
    theme: &Theme,
    title: impl Into<SharedString>,
    expanded: bool,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    extras: SectionExtras,
    children: Vec<AnyElement>,
) -> impl IntoElement {
    let theme = *theme;
    let SectionExtras { trailing, dimmed } = extras;
    let has_trailing = !trailing.is_empty();

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
                .when(dimmed, |el| el.text_color(theme.text_faint))
                .when(has_trailing, |el| {
                    el.child(div().flex_1()).child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1p5()
                            // Keep these clicks from folding the section.
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation()
                            })
                            .children(trailing),
                    )
                })
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
                    .when(dimmed, |el| el.opacity(0.45))
                    .children(children),
            )
        })
}

/// A vertically scrollable container for stacking develop panel sections,
/// docked to `side` at a fixed `width`, with a minimal scrollbar driven by
/// `scroll`. `pinned` stays above the scrolling sections (the histogram)
/// and `footer` (see [`panel_footer`]) below them.
pub fn side_panel(
    theme: &Theme,
    side: PanelSide,
    width: f32,
    scroll: &ScrollHandle,
    pinned: Option<AnyElement>,
    footer: Option<AnyElement>,
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
        .children(pinned.map(|pinned| {
            div()
                .flex_shrink_0()
                .px_3()
                .pt_2()
                .pb_2()
                .border_b_1()
                .border_color(theme.border_subtle)
                .child(pinned)
        }))
        .child(
            div()
                .relative()
                .flex_1()
                .min_h(px(0.0))
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
                )),
        )
        .children(footer)
}

/// A docked side panel (Library's left/right panels) that can be collapsed
/// down to a thin strip holding only its expand chevron, like Lightroom's
/// panel groups.
#[allow(clippy::too_many_arguments)]
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
    collapsible_panel_pinned(
        id, theme, title, side, width, collapsed, scroll, on_toggle, None, None, children,
    )
}

/// [`collapsible_panel`] with `pinned` held under the title and `footer`
/// (see [`panel_footer`]) along the bottom, both outside the scrolling
/// `children` so they never scroll away.
#[allow(clippy::too_many_arguments)]
pub fn collapsible_panel_pinned(
    id: impl Into<ElementId>,
    theme: &Theme,
    title: impl Into<SharedString>,
    side: PanelSide,
    width: f32,
    collapsed: bool,
    scroll: &ScrollHandle,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    pinned: Option<AnyElement>,
    footer: Option<AnyElement>,
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
        .children(pinned.map(|pinned| {
            div()
                .flex_shrink_0()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(theme.border_subtle)
                .child(pinned)
        }))
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
        .children(footer)
        .into_any_element()
}

/// The fixed strip along the bottom of a side panel: rows of
/// [`footer_button`]s, side by side.
pub fn panel_footer(theme: &Theme, rows: Vec<Vec<AnyElement>>) -> AnyElement {
    let theme = *theme;
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap_1p5()
        .px_3()
        .py_2()
        .bg(theme.background)
        .border_t_1()
        .border_color(theme.border_subtle)
        .children(
            rows.into_iter()
                .map(|row| div().flex().flex_row().gap_1p5().children(row)),
        )
        .into_any_element()
}

/// An equal-width button for [`panel_footer`]. Disabled buttons are faded
/// and ignore clicks.
pub fn footer_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    tooltip: impl Into<SharedString>,
    enabled: bool,
    theme: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let theme = *theme;
    div()
        .id(id)
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .h(px(26.0))
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .bg(theme.surface_elevated.opacity(0.5))
        .text_size(px(11.5))
        .text_color(theme.text)
        .when(enabled, |el| {
            el.cursor_pointer()
                .hover(|el| el.bg(theme.surface_elevated).border_color(theme.accent))
                .on_click(on_click)
        })
        .when(!enabled, |el| el.opacity(0.4))
        .child(label.into())
        .tooltip(tip(tooltip, &theme))
        .into_any_element()
}
