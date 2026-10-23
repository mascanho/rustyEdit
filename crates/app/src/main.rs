//! RustyEdit: a Lightroom-inspired cataloging and editing app built with
//! GPUI. The binary is intentionally thin — it only wires up keybindings,
//! the menu, and the window, then hands off to `Workspace`.

mod actions;
mod albums;
mod catalogs;
mod compare;
mod context_menu;
mod cull;
mod cull_bar;
mod develop;
mod display_profile;
mod duplicates;
mod edit_clipboard;
mod export;
mod export_presets;
mod file_ops;
mod file_tree;
mod filmstrip;
mod filters_panel;
mod footer_zoom;
mod grid;
mod histogram;
mod histogram_panel;
mod history_panel;
mod import;
mod before_after;
mod edited_thumbs;
mod info_overlay;
mod keywords;
mod library;
mod maintenance;
mod metadata_panel;
mod panel_footers;
mod rail;
mod resize;
mod selection;
mod settings_view;
mod size_menu;
mod stacks;
mod sync_settings;
mod toasts;
mod undo;
mod virtual_copies;
mod wheel_zoom;
mod workspace;

use gpui::{
    point, prelude::*, px, size, App, Application, Bounds, KeyBinding, Menu, MenuItem,
    TitlebarOptions, WindowBounds, WindowOptions,
};

use actions::{
    AutoStack, BackupCatalog, ClearCache, CloseSettings, CycleBackground, CycleFilter, CycleLayout,
    DeleteFromCatalog, DeleteFromDisk, FindDuplicates, FlagPick, FlagReject, FlagUnflag,
    FocusSearch, GoDevelop, GoExport, GoLibrary, ImportFolder, LabelBlue, LabelGreen, LabelRed,
    LabelYellow, NextImage, OpenSettings, OptimizeCatalog, PrevImage, Quit, Rating0, Rating1,
    Rating2, Rating3, Rating4, Rating5, Redo, RenamePhoto, RestoreCatalog, RestoreRemoved,
    RevealInFinder, RotateLeft, RotateRight, SelectAll, StackSelected, SyncFolders, ToggleBeforeAfter, ToggleInfo,
    ToggleStack, ToggleZoom, Undo, Unstack, ViewCompare, ViewSurvey,
};
use actions::{
    AutoTone, CreateVirtualCopy, MatchLook, PickBestOfBursts, PresetFromPhoto, RejectSoftPhotos,
    SyncSettings, ToggleCullView, ToolWhiteBalance,
    CycleGridStyle, GridLarger, GridSmaller, ToolLinearGradient, ToolRadialGradient,
    ToolSpotRemoval, ExtendDown, ExtendFirst, ExtendLast, ExtendLeft, ExtendRight, ExtendUp,
    SelectDown, SelectFirst, SelectLast, SelectPageDown, SelectPageUp, SelectUp,
};
use workspace::Workspace;

const KEY_CONTEXT: &str = "Workspace";

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-o", ImportFolder, Some(KEY_CONTEXT)),
            KeyBinding::new("right", NextImage, Some(KEY_CONTEXT)),
            KeyBinding::new("left", PrevImage, Some(KEY_CONTEXT)),
            KeyBinding::new("up", SelectUp, Some(KEY_CONTEXT)),
            KeyBinding::new("down", SelectDown, Some(KEY_CONTEXT)),
            KeyBinding::new("home", SelectFirst, Some(KEY_CONTEXT)),
            KeyBinding::new("end", SelectLast, Some(KEY_CONTEXT)),
            KeyBinding::new("pageup", SelectPageUp, Some(KEY_CONTEXT)),
            KeyBinding::new("pagedown", SelectPageDown, Some(KEY_CONTEXT)),
            KeyBinding::new("shift-left", ExtendLeft, Some(KEY_CONTEXT)),
            KeyBinding::new("shift-right", ExtendRight, Some(KEY_CONTEXT)),
            KeyBinding::new("shift-up", ExtendUp, Some(KEY_CONTEXT)),
            KeyBinding::new("shift-down", ExtendDown, Some(KEY_CONTEXT)),
            KeyBinding::new("shift-home", ExtendFirst, Some(KEY_CONTEXT)),
            KeyBinding::new("shift-end", ExtendLast, Some(KEY_CONTEXT)),
            KeyBinding::new("p", FlagPick, Some(KEY_CONTEXT)),
            KeyBinding::new("x", FlagReject, Some(KEY_CONTEXT)),
            KeyBinding::new("u", FlagUnflag, Some(KEY_CONTEXT)),
            KeyBinding::new("0", Rating0, Some(KEY_CONTEXT)),
            KeyBinding::new("1", Rating1, Some(KEY_CONTEXT)),
            KeyBinding::new("2", Rating2, Some(KEY_CONTEXT)),
            KeyBinding::new("3", Rating3, Some(KEY_CONTEXT)),
            KeyBinding::new("4", Rating4, Some(KEY_CONTEXT)),
            KeyBinding::new("5", Rating5, Some(KEY_CONTEXT)),
            KeyBinding::new("6", LabelRed, Some(KEY_CONTEXT)),
            KeyBinding::new("7", LabelYellow, Some(KEY_CONTEXT)),
            KeyBinding::new("8", LabelGreen, Some(KEY_CONTEXT)),
            KeyBinding::new("9", LabelBlue, Some(KEY_CONTEXT)),
            KeyBinding::new("g", GoLibrary, Some(KEY_CONTEXT)),
            KeyBinding::new("\\", ToggleBeforeAfter, Some(KEY_CONTEXT)),
            KeyBinding::new("l", GoLibrary, Some(KEY_CONTEXT)),
            KeyBinding::new("d", GoDevelop, Some(KEY_CONTEXT)),
            KeyBinding::new("e", GoExport, Some(KEY_CONTEXT)),
            KeyBinding::new("escape", GoLibrary, Some(KEY_CONTEXT)),
            KeyBinding::new("f", CycleFilter, Some(KEY_CONTEXT)),
            KeyBinding::new("z", ToggleZoom, Some(KEY_CONTEXT)),
            KeyBinding::new("i", ToggleInfo, Some(KEY_CONTEXT)),
            KeyBinding::new("b", CycleBackground, Some(KEY_CONTEXT)),
            KeyBinding::new("j", CycleGridStyle, Some(KEY_CONTEXT)),
            KeyBinding::new("q", ToolSpotRemoval, Some(KEY_CONTEXT)),
            KeyBinding::new("m", ToolLinearGradient, Some(KEY_CONTEXT)),
            KeyBinding::new("shift-m", ToolRadialGradient, Some(KEY_CONTEXT)),
            KeyBinding::new("w", ToolWhiteBalance, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-u", AutoTone, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-shift-s", SyncSettings, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-'", CreateVirtualCopy, Some(KEY_CONTEXT)),
            KeyBinding::new("shift-f", ToggleCullView, Some(KEY_CONTEXT)),
            // "+" is shift-= on most layouts; bind every way it can arrive.
            KeyBinding::new("ctrl-=", GridLarger, Some(KEY_CONTEXT)),
            KeyBinding::new("ctrl-+", GridLarger, Some(KEY_CONTEXT)),
            KeyBinding::new("ctrl-shift-=", GridLarger, Some(KEY_CONTEXT)),
            KeyBinding::new("ctrl--", GridSmaller, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-f", FocusSearch, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-z", Undo, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-shift-z", Redo, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-a", SelectAll, Some(KEY_CONTEXT)),
            KeyBinding::new("c", ViewCompare, Some(KEY_CONTEXT)),
            KeyBinding::new("n", ViewSurvey, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-,", OpenSettings, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-shift-l", CycleLayout, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-g", StackSelected, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-shift-g", AutoStack, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-shift-u", Unstack, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-shift-k", ToggleStack, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-shift-d", FindDuplicates, Some(KEY_CONTEXT)),
            KeyBinding::new("backspace", DeleteFromCatalog, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-backspace", DeleteFromDisk, Some(KEY_CONTEXT)),
            KeyBinding::new("f2", RenamePhoto, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-[", RotateLeft, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-]", RotateRight, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-shift-o", RevealInFinder, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-shift-r", SyncFolders, Some(KEY_CONTEXT)),
            KeyBinding::new("escape", CloseSettings, Some("SettingsModal")),
        ]);

        cx.on_action(|_: &Quit, cx| cx.quit());

        cx.set_menus(vec![
            Menu {
                name: "RustyEdit".into(),
                items: vec![
                    MenuItem::action("Settings…", OpenSettings),
                    MenuItem::action("Next Layout", CycleLayout),
                    MenuItem::separator(),
                    MenuItem::action("Quit RustyEdit", Quit),
                ],
            },
            Menu {
                name: "Photo".into(),
                items: vec![
                    MenuItem::action("Reveal in Finder", RevealInFinder),
                    MenuItem::action("Rename Photo…", RenamePhoto),
                    MenuItem::action("Rotate Left", RotateLeft),
                    MenuItem::action("Rotate Right", RotateRight),
                    MenuItem::separator(),
                    MenuItem::action("Create Virtual Copy", CreateVirtualCopy),
                    MenuItem::action("Sync Settings…", SyncSettings),
                    MenuItem::action("Auto Tone", AutoTone),
                    MenuItem::action("Match Look to a Photo…", MatchLook),
                    MenuItem::action("New Preset from a Photo…", PresetFromPhoto),
                    MenuItem::separator(),
                    MenuItem::action("Remove from Catalog", DeleteFromCatalog),
                    MenuItem::action("Move to Trash…", DeleteFromDisk),
                    MenuItem::separator(),
                    MenuItem::action("Stack Selected Photos", StackSelected),
                    MenuItem::action("Auto-stack by Capture Time", AutoStack),
                    MenuItem::action("Unstack", Unstack),
                    MenuItem::action("Collapse/Expand Stack", ToggleStack),
                    MenuItem::action("Find Duplicates…", FindDuplicates),
                    MenuItem::separator(),
                    MenuItem::action("Full-Screen Cull", ToggleCullView),
                    MenuItem::action("Pick Best of Each Burst", PickBestOfBursts),
                    MenuItem::action("Reject Soft Photos", RejectSoftPhotos),
                ],
            },
            Menu {
                name: "Catalog".into(),
                items: vec![
                    MenuItem::action("Synchronize Folders", SyncFolders),
                    MenuItem::action("Restore Removed Photos", RestoreRemoved),
                    MenuItem::separator(),
                    MenuItem::action("Back Up Catalog…", BackupCatalog),
                    MenuItem::action("Restore Catalog from Backup…", RestoreCatalog),
                    MenuItem::action("Optimize Catalog", OptimizeCatalog),
                    MenuItem::action("Clear Thumbnail & Preview Cache", ClearCache),
                ],
            },
        ]);

        let bounds = Bounds::centered(None, size(px(1440.0), px(900.0)), cx);

        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: None,
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(12.0), px(12.0))),
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let workspace = cx.new(Workspace::new);
                window.focus(&workspace.read(cx).focus_handle.clone());
                workspace
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
