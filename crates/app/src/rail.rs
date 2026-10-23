//! The icon rails beside the side docks: which tools each shows, which one
//! is active, and what activating it does. The rail widget lives in
//! `rustyedit_ui`; the tool registry in `rustyedit_core::RailTool`.

use gpui::{prelude::*, AnyElement, Context};
use rustyedit_core::{settings::DockSide, RailTool};
use rustyedit_ui::{Icon, PanelSide, RailButton};

use crate::workspace::Workspace;

impl Workspace {
    /// The Presets tab is open in a Library dock.
    pub(crate) fn library_presets_open(&self) -> bool {
        self.mode == crate::workspace::Mode::Library
            && (self.left_rail_tool == Some(RailTool::Presets)
                || self.right_rail_tool == Some(RailTool::Presets))
    }

    /// Opens the Presets panel in the Library's left dock (or closes it if
    /// it's open), for applying presets while looking at the loupe.
    pub(crate) fn toggle_library_presets(&mut self, cx: &mut Context<Self>) {
        if self.library_presets_open() {
            self.toggle_rail_tool(DockSide::Left, RailTool::Presets, cx);
            return;
        }
        self.left_panel_collapsed = false;
        self.left_rail_tool = None;
        self.toggle_rail_tool(DockSide::Left, RailTool::Presets, cx);
    }

    pub(crate) fn active_rail_tool(&self, side: DockSide) -> Option<RailTool> {
        match side {
            DockSide::Left => self.left_rail_tool,
            DockSide::Right => self.right_rail_tool,
        }
    }

    /// Builds `side`'s rail from the tool registry.
    pub(crate) fn render_rail(&self, side: DockSide, cx: &mut Context<Self>) -> AnyElement {
        let active = self.active_rail_tool(side);
        let (id, panel_side) = match side {
            DockSide::Left => ("rail-left", PanelSide::Left),
            DockSide::Right => ("rail-right", PanelSide::Right),
        };
        let buttons = RailTool::for_side(side)
            .iter()
            .map(|&tool| RailButton {
                id: format!("{id}-{}", tool.label()).into(),
                icon: rail_icon(tool),
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

    pub(crate) fn toggle_rail_tool(
        &mut self,
        side: DockSide,
        tool: RailTool,
        cx: &mut Context<Self>,
    ) {
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
            RailTool::Folders => Some(self.navigator_sections(cx)),
            RailTool::History => Some(self.history_sections()),
            RailTool::Snapshots => Some(self.snapshots_section(cx)),
            RailTool::Collections => Some(self.albums_sections(cx)),
            RailTool::Search => Some(self.search_sections(cx)),
            RailTool::Filters => Some(self.filters_sections(cx)),
            RailTool::Presets => Some(self.presets_sections(cx)),
            RailTool::Keywords => Some(self.keywords_sections(cx)),
            RailTool::Duplicates => Some(self.duplicates_sections(cx)),
            _ => None,
        }
    }

    /// Opens the right dock on `tool` (Keywords, Duplicates, …).
    pub(crate) fn open_right_tool(&mut self, tool: RailTool, cx: &mut Context<Self>) {
        self.right_panel_collapsed = false;
        self.right_rail_tool = Some(tool);
        self.activate_rail_tool(tool, cx);
        cx.notify();
    }

    /// The extension point: give each tool its behaviour here as it's built.
    fn activate_rail_tool(&mut self, tool: RailTool, cx: &mut Context<Self>) {
        // In the Library, the Presets tab renders the selected photo's edits
        // so the Navigator shows what a preset does.
        if tool == RailTool::Presets && self.library_presets_open() {
            if self.source_image_for != self.catalog.selected_asset().map(|a| a.id) {
                self.load_source_image(cx);
            }
            return;
        }
        if tool == RailTool::Duplicates {
            self.ensure_duplicates(cx);
            return;
        }
        if matches!(
            tool,
            RailTool::Folders
                | RailTool::History
                | RailTool::Snapshots
                | RailTool::Collections
                | RailTool::Search
                | RailTool::Filters
                | RailTool::Presets
                | RailTool::Keywords
        ) {
            return; // Shown in the dock itself; nothing to announce.
        }
        self.status_message = Some(format!("{} \u{2014} coming soon", tool.label()).into());
    }
}

/// The icon each rail tool is drawn with.
fn rail_icon(tool: RailTool) -> Icon {
    match tool {
        RailTool::Folders => Icon::Folder,
        RailTool::Search => Icon::Search,
        RailTool::Collections => Icon::Collections,
        RailTool::Filters => Icon::Filter,
        RailTool::Presets => Icon::Presets,
        RailTool::Duplicates => Icon::Duplicates,
        RailTool::History => Icon::History,
        RailTool::Snapshots => Icon::Camera,
        RailTool::Info => Icon::Info,
        RailTool::Adjust => Icon::Adjust,
        RailTool::Keywords => Icon::Tag,
    }
}
