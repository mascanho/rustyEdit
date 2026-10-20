//! The icon rails beside the side docks: which tools each shows, which one
//! is active, and what activating it does. The rail widget lives in
//! `rustyedit_ui`; the tool registry in `rustyedit_core::RailTool`.

use gpui::{prelude::*, AnyElement, Context};
use rustyedit_core::{settings::DockSide, RailTool};
use rustyedit_ui::{PanelSide, RailButton};

use crate::workspace::Workspace;

impl Workspace {
    pub(crate) fn active_rail_tool(&self, side: DockSide) -> Option<RailTool> {
        match side {
            DockSide::Left => self.left_rail_tool,
            DockSide::Right => self.right_rail_tool,
        }
    }

    /// Builds `side`'s rail from the tool registry.
    fn render_rail(&self, side: DockSide, cx: &mut Context<Self>) -> AnyElement {
        let active = self.active_rail_tool(side);
        let (id, panel_side) = match side {
            DockSide::Left => ("rail-left", PanelSide::Left),
            DockSide::Right => ("rail-right", PanelSide::Right),
        };
        let buttons = RailTool::for_side(side)
            .iter()
            .map(|&tool| RailButton {
                id: format!("{id}-{}", tool.label()).into(),
                glyph: tool.glyph().into(),
                tooltip: tool.label().into(),
                active: active == Some(tool),
                on_click: Box::new(cx.listener(move |this, _, _window, cx| {
                    this.toggle_rail_tool(side, tool, cx);
                })),
            })
            .collect();
        rustyedit_ui::icon_rail(id, &self.theme, panel_side, buttons).into_any_element()
    }

    /// Orders a dock and its rail so the rail sits on the edge facing the
    /// photo: right of a left dock, left of a right dock. A layout with no
    /// dock on that side gets no rail either.
    pub(crate) fn dock_with_rail(
        &self,
        side: DockSide,
        dock: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(dock) = dock else {
            return Vec::new();
        };
        // A collapsed dock collapses entirely, rail included; only the
        // dock's own thin expand strip remains.
        let collapsed = match side {
            DockSide::Left => self.left_panel_collapsed,
            DockSide::Right => self.right_panel_collapsed,
        };
        if collapsed {
            return vec![dock];
        }
        let rail = self.render_rail(side, cx);
        let grip = self.resize_grip(crate::resize::ResizeTarget::Dock(side), cx);
        match side {
            DockSide::Left => vec![dock, grip, rail],
            DockSide::Right => vec![rail, grip, dock],
        }
    }

    fn toggle_rail_tool(&mut self, side: DockSide, tool: RailTool, cx: &mut Context<Self>) {
        let slot = match side {
            DockSide::Left => &mut self.left_rail_tool,
            DockSide::Right => &mut self.right_rail_tool,
        };
        *slot = if *slot == Some(tool) {
            None
        } else {
            Some(tool)
        };
        if *slot == Some(tool) {
            self.activate_rail_tool(tool, cx);
        }
        cx.notify();
    }

    /// A tool that takes over its dock shows its own content instead of the
    /// layout's panels; tools without one return `None` and leave the dock
    /// as it was. Add new dock-hosted tools here.
    pub(crate) fn rail_tool_content(
        &self,
        tool: RailTool,
        cx: &mut Context<Self>,
    ) -> Option<Vec<AnyElement>> {
        match tool {
            RailTool::History => Some(self.history_sections()),
            RailTool::Collections => Some(self.albums_sections(cx)),
            RailTool::Search => Some(self.search_sections(cx)),
            RailTool::Filters => Some(self.filters_sections(cx)),
            _ => None,
        }
    }

    /// The extension point: give each tool its behaviour here as it's built.
    fn activate_rail_tool(&mut self, tool: RailTool, _cx: &mut Context<Self>) {
        if matches!(
            tool,
            RailTool::History | RailTool::Collections | RailTool::Search | RailTool::Filters
        ) {
            return; // Shown in the dock itself; nothing to announce.
        }
        self.status_message = Some(format!("{} \u{2014} coming soon", tool.label()).into());
    }
}
