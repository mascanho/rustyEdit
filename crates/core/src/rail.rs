//! The tools that appear as icons in the thin rails beside each side dock.
//!
//! Adding a tool is two steps: add a variant here (with its label) and list
//! it in [`RailTool::for_side`]; then give it an icon in the app's
//! `rail_icon` and behaviour in `Workspace::activate_rail_tool`. The rail UI itself is generic
//! and renders whatever this registry returns.

use crate::settings::DockSide;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RailTool {
    // Left rail: finding and organising photos.
    Folders,
    Search,
    Collections,
    Filters,
    Presets,
    Duplicates,
    History,
    // Right rail: working on the selected photo.
    Info,
    Adjust,
    Keywords,
}

impl RailTool {
    /// The tools shown on `side`'s rail, top to bottom.
    pub fn for_side(side: DockSide) -> &'static [RailTool] {
        match side {
            DockSide::Left => &[
                RailTool::Folders,
                RailTool::Search,
                RailTool::Collections,
                RailTool::Filters,
                RailTool::Presets,
                RailTool::Duplicates,
                RailTool::History,
            ],
            DockSide::Right => &[RailTool::Info, RailTool::Adjust, RailTool::Keywords],
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            RailTool::Folders => "Folders",
            RailTool::Search => "Search",
            RailTool::Collections => "Collections",
            RailTool::Filters => "Filters",
            RailTool::Presets => "Presets",
            RailTool::Duplicates => "Duplicates",
            RailTool::Info => "Info",
            RailTool::Adjust => "Adjust",
            RailTool::Keywords => "Keywords",
            RailTool::History => "History",
        }
    }
}
