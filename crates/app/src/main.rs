//! RustyEdit: a Lightroom-inspired cataloging and editing app built with
//! GPUI. The binary is intentionally thin — it only wires up keybindings,
//! the menu, and the window, then hands off to `Workspace`.

mod actions;
mod albums;
mod catalogs;
mod compare;
mod cull_bar;
mod develop;
mod export;
mod file_tree;
mod filmstrip;
mod filters_panel;
mod histogram;
mod history_panel;
mod import;
mod info_overlay;
mod library;
mod rail;
mod resize;
mod selection;
mod settings_view;
mod size_menu;
mod undo;
mod wheel_zoom;
mod workspace;

use gpui::{
    point, prelude::*, px, size, App, Application, Bounds, KeyBinding, Menu, MenuItem,
    TitlebarOptions, WindowBounds, WindowOptions,
};

use actions::{
    CloseSettings, CycleBackground, CycleFilter, CycleLayout, FlagPick, FlagReject, FlagUnflag,
    FocusSearch, GoDevelop, GoExport, GoLibrary, ImportFolder, LabelBlue, LabelGreen, LabelRed,
    LabelYellow, NextImage, OpenSettings, PrevImage, Quit, Rating0, Rating1, Rating2, Rating3,
    Rating4, Rating5, Redo, SelectAll, ToggleInfo, ToggleZoom, Undo, ViewCompare, ViewSurvey,
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
            KeyBinding::new("d", GoDevelop, Some(KEY_CONTEXT)),
            KeyBinding::new("e", GoExport, Some(KEY_CONTEXT)),
            KeyBinding::new("escape", GoLibrary, Some(KEY_CONTEXT)),
            KeyBinding::new("f", CycleFilter, Some(KEY_CONTEXT)),
            KeyBinding::new("z", ToggleZoom, Some(KEY_CONTEXT)),
            KeyBinding::new("i", ToggleInfo, Some(KEY_CONTEXT)),
            KeyBinding::new("b", CycleBackground, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-f", FocusSearch, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-z", Undo, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-shift-z", Redo, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-a", SelectAll, Some(KEY_CONTEXT)),
            KeyBinding::new("c", ViewCompare, Some(KEY_CONTEXT)),
            KeyBinding::new("n", ViewSurvey, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-,", OpenSettings, Some(KEY_CONTEXT)),
            KeyBinding::new("cmd-shift-l", CycleLayout, Some(KEY_CONTEXT)),
            KeyBinding::new("escape", CloseSettings, Some("SettingsModal")),
        ]);

        cx.on_action(|_: &Quit, cx| cx.quit());

        cx.set_menus(vec![Menu {
            name: "RustyEdit".into(),
            items: vec![
                MenuItem::action("Settings…", OpenSettings),
                MenuItem::action("Next Layout", CycleLayout),
                MenuItem::separator(),
                MenuItem::action("Quit RustyEdit", Quit),
            ],
        }]);

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
