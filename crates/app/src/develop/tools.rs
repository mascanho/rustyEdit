//! Develop's tools column: Navigator, Presets, Snapshots and History. It
//! sits wherever the layout puts the Navigator: its own dock opposite the
//! editing panel, stacked above the sliders, or nowhere at all. The
//! Navigator itself is pinned at its top, outside the scrolling sections. As its own
//! dock it carries that side's icon rail, whose tools can take it over.

use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context, SharedString};
use rustyedit_core::{settings::DockSide, RailTool};
use rustyedit_ui::{PanelSide, Theme};

use crate::{library::panel_group_heading, resize::ResizeTarget, workspace::Workspace};

/// Dock title while it shows the Develop tools and Library's panel,
/// joined like Library's stacked dock titles.
const TOOLS_TITLE: &str = "Tools \u{00B7} Library";

/// A click handler stored for later, e.g. a row's delete button.
pub(super) type ClickHandler = Box<dyn Fn(&ClickEvent, &mut gpui::Window, &mut gpui::App)>;

impl Workspace {
    /// The side the tools column belongs to, or `None` when the layout has
    /// no Navigator (Inspector Only, Focus).
    pub(super) fn develop_tools_side(&self) -> Option<DockSide> {
        self.settings.layout.preset.arrangement().navigator_side()
    }

    /// Width of the column the tools are in: their own dock's, or the
    /// editing panel's when the layout stacks them there.
    pub(super) fn develop_tools_width(&self) -> f32 {
        let layout = &self.settings.layout;
        if self.develop_tools_side() == Some(self.develop_panel_side()) {
            layout.inspector_width as f32
        } else {
            layout.navigator_width as f32
        }
    }

    /// The Develop tools, then Library's Navigator panel (folders, catalog,
    /// filter) under its own heading, as Library stacks panels that share
    /// a dock.
    pub(super) fn develop_tools_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        // The Navigator isn't here: it's pinned above these, always shown.
        let mut sections = vec![
            self.disclosure(
                "develop-snapshots",
                "Snapshots",
                self.snapshots_section(cx),
                cx,
            ),
            self.disclosure(
                "develop-history",
                "History",
                self.develop_history_section(cx),
                cx,
            ),
        ];
        sections.push(panel_group_heading(&self.theme, "Library"));
        sections.extend(self.navigator_sections(cx));
        sections
    }

    /// What the tools column shows: the Develop tools, or the rail tool
    /// picked beside it (Folders, Search, …) in their place. Develop's
    /// History is the clickable one, so the rail's History opens that.
    fn develop_tools_content(
        &self,
        side: DockSide,
        cx: &mut Context<Self>,
    ) -> (SharedString, Vec<AnyElement>) {
        match self.active_rail_tool(side) {
            Some(RailTool::History) => ("History".into(), self.develop_history_section(cx)),
            Some(tool) => match self.rail_tool_content(tool, cx) {
                Some(content) => (tool.label().into(), content),
                None => (TOOLS_TITLE.into(), self.develop_tools_sections(cx)),
            },
            None => (TOOLS_TITLE.into(), self.develop_tools_sections(cx)),
        }
    }

    /// The tools column as its own collapsible dock on `side`, with its icon
    /// rail and a resize grip on the edge facing the photo.
    pub(super) fn render_develop_tools_dock(
        &self,
        side: DockSide,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let collapsed = self.develop_ui.tools_collapsed;
        let (title, content) = if collapsed {
            (TOOLS_TITLE.into(), Vec::new())
        } else {
            self.develop_tools_content(side, cx)
        };
        let navigator =
            (!collapsed).then(|| self.render_navigator(super::navigator::MAX_HEIGHT, cx));
        // The Presets tab pins its controls under the Navigator.
        let navigator = if !collapsed && self.active_rail_tool(side) == Some(RailTool::Presets) {
            Some(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(navigator)
                    .child(self.presets_pinned(cx))
                    .into_any_element(),
            )
        } else {
            navigator
        };
        let dock = rustyedit_ui::collapsible_panel_pinned(
            "develop-tools",
            &self.theme,
            title,
            match side {
                DockSide::Left => PanelSide::Left,
                DockSide::Right => PanelSide::Right,
            },
            self.settings.layout.navigator_width as f32,
            collapsed,
            &self.develop_ui.tools_scroll,
            cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.develop_ui.tools_collapsed = !this.develop_ui.tools_collapsed;
                cx.notify();
            }),
            navigator,
            Some(self.develop_tools_footer(cx)),
            content,
        )
        .into_any_element();

        // A collapsed column collapses entirely, rail included, like
        // Library's docks.
        if collapsed {
            return vec![dock];
        }
        let grip = self.resize_grip(ResizeTarget::Dock(side), cx);
        let rail = self.render_rail(side, cx);
        match side {
            DockSide::Left => vec![dock, grip, rail],
            DockSide::Right => vec![rail, grip, dock],
        }
    }
}

/// One clickable row in a tools list (a preset, a snapshot): name on the
/// left, a muted note and an optional delete button on the right.
pub(super) fn list_row(
    id: impl Into<gpui::ElementId>,
    theme: &Theme,
    name: String,
    note: Option<String>,
    active: bool,
    on_click: impl Fn(&ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    on_delete: Option<ClickHandler>,
) -> AnyElement {
    let theme = *theme;
    div()
        .id(id)
        .group("develop-list-row")
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .h(px(24.0))
        .px_2()
        .cursor_pointer()
        .text_size(px(11.5))
        .when(active, |el| {
            el.bg(theme.accent_muted).text_color(theme.text)
        })
        .when(!active, |el| {
            el.text_color(theme.text_muted)
                .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
        })
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .whitespace_nowrap()
                .child(name),
        )
        .children(note.map(|note| {
            div()
                .flex_shrink_0()
                .text_size(px(9.5))
                .text_color(theme.text_faint)
                .child(note)
        }))
        .children(on_delete.map(|on_delete| {
            div()
                .id("delete")
                .flex_shrink_0()
                .px_1()
                .text_size(px(10.0))
                .text_color(theme.text_faint)
                .invisible()
                .group_hover("develop-list-row", |s| s.visible())
                .hover(|el| el.text_color(theme.danger))
                .child("\u{2715}")
                .on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    on_delete(event, window, cx);
                })
        }))
        .on_click(on_click)
        .into_any_element()
}

/// Section header row with a title-less "+" action on the right, used by
/// Presets and Snapshots to add the current state.
pub(super) fn add_row(
    id: &'static str,
    theme: &Theme,
    label: &'static str,
    on_click: impl Fn(&ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let theme = *theme;
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .h(px(24.0))
        .px_2()
        .cursor_pointer()
        .text_size(px(11.0))
        .text_color(theme.accent)
        .hover(|el| el.bg(theme.surface_elevated))
        .child("+")
        .child(label)
        .on_click(on_click)
        .into_any_element()
}

/// The inline name field shown while naming a new preset or snapshot.
pub(super) fn name_field(theme: &Theme, draft: &str, placeholder: &str) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(26.0))
        .px_2()
        .bg(theme.background)
        .border_1()
        .border_color(theme.accent)
        .text_size(px(11.5))
        .child(
            div()
                .text_color(if draft.is_empty() {
                    theme.text_faint
                } else {
                    theme.text
                })
                .child(if draft.is_empty() {
                    placeholder.to_string()
                } else {
                    draft.to_string()
                }),
        )
        .child(div().w(px(1.0)).h(px(14.0)).ml(px(1.0)).bg(theme.accent))
        .into_any_element()
}
