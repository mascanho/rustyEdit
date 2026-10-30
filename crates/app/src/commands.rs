//! Every command the app has, in one table, and the two places that show
//! it all:
//!
//! - The command palette (⌘K / Ctrl+K): type to find any command and press
//!   Enter to run it, exactly as if its shortcut or menu item were used.
//! - The keyboard shortcut sheet (?): every shortcut, grouped.
//!
//! Shortcuts are read from the live keymap, so the sheet and the palette
//! always show what's really bound, written the platform's way (⌘⇧S on a
//! Mac, Ctrl+Shift+S elsewhere).

use gpui::{
    div, prelude::*, Action, AnyElement, App, ClickEvent, Context, MouseButton, SharedString,
    Window,
};
use rustyedit_ui::{sp, Theme};

use crate::{
    actions::*,
    workspace::{TextTarget, Workspace},
};

/// One command: what it's called, where it belongs, and how to run it.
pub(crate) struct Command {
    pub(crate) label: &'static str,
    pub(crate) group: &'static str,
    /// Other words it might be searched by.
    pub(crate) keywords: &'static str,
    pub(crate) action: fn() -> Box<dyn Action>,
}

macro_rules! cmd {
    ($group:literal, $label:literal, $action:ident) => {
        cmd!($group, $label, $action, "")
    };
    ($group:literal, $label:literal, $action:ident, $keywords:literal) => {
        Command {
            label: $label,
            group: $group,
            keywords: $keywords,
            action: || Box::new($action),
        }
    };
}

/// The groups, in the order the shortcut sheet shows them.
const GROUPS: [&str; 8] = [
    "General", "View", "Navigation", "Culling", "Develop", "Photo", "Catalog", "Selection",
];

/// Every command.
pub(crate) fn commands() -> Vec<Command> {
    vec![
        cmd!("General", "Command palette", ToggleCommandPalette, "search run find"),
        cmd!("General", "Keyboard shortcuts", ToggleShortcuts, "help keys cheat sheet"),
        cmd!("General", "Undo", Undo),
        cmd!("General", "Redo", Redo),
        cmd!("General", "Import a folder\u{2026}", ImportFolder, "add open"),
        cmd!("General", "Settings\u{2026}", OpenSettings, "preferences options"),
        cmd!("General", "Next layout", CycleLayout, "panels arrangement"),
        cmd!("General", "Quit", Quit, "exit close"),
        cmd!("View", "Library", GoLibrary, "grid browse"),
        cmd!("View", "Loupe / grid", ToggleLoupe, "single photo view large"),
        cmd!("View", "Develop", GoDevelop, "edit adjust"),
        cmd!("View", "Export", GoExport, "save output"),
        cmd!("View", "Full-screen cull", ToggleCullView, "review pick reject"),
        cmd!("View", "Compare", ViewCompare, "side by side"),
        cmd!("View", "Survey", ViewSurvey, "several"),
        cmd!("View", "Zoom to 100% / fit", ToggleZoom),
        cmd!("View", "Before / after", ToggleBeforeAfter, "original"),
        cmd!("View", "Info overlay", ToggleInfo, "exif details"),
        cmd!("View", "Next background", CycleBackground, "loupe backdrop"),
        cmd!("View", "Next grid style", CycleGridStyle, "cards"),
        cmd!("View", "Larger thumbnails", GridLarger, "bigger"),
        cmd!("View", "Smaller thumbnails", GridSmaller),
        cmd!("View", "Search photos", FocusSearch, "find filter"),
        cmd!("View", "Next filter", CycleFilter, "picked rejected"),
        cmd!("Navigation", "Next photo", NextImage),
        cmd!("Navigation", "Previous photo", PrevImage),
        cmd!("Navigation", "Photo above", SelectUp),
        cmd!("Navigation", "Photo below", SelectDown),
        cmd!("Navigation", "First photo", SelectFirst),
        cmd!("Navigation", "Last photo", SelectLast),
        cmd!("Navigation", "Page up", SelectPageUp),
        cmd!("Navigation", "Page down", SelectPageDown),
        cmd!("Selection", "Select all", SelectAll),
        cmd!("Selection", "Extend selection left", ExtendLeft),
        cmd!("Selection", "Extend selection right", ExtendRight),
        cmd!("Selection", "Extend selection up", ExtendUp),
        cmd!("Selection", "Extend selection down", ExtendDown),
        cmd!("Selection", "Extend selection to first", ExtendFirst),
        cmd!("Selection", "Extend selection to last", ExtendLast),
        cmd!("Culling", "Pick", FlagPick, "flag keep"),
        cmd!("Culling", "Reject", FlagReject, "flag"),
        cmd!("Culling", "Unflag", FlagUnflag, "flag clear"),
        cmd!("Culling", "No stars", Rating0, "rating clear"),
        cmd!("Culling", "1 star", Rating1, "rating"),
        cmd!("Culling", "2 stars", Rating2, "rating"),
        cmd!("Culling", "3 stars", Rating3, "rating"),
        cmd!("Culling", "4 stars", Rating4, "rating"),
        cmd!("Culling", "5 stars", Rating5, "rating"),
        cmd!("Culling", "Red label", LabelRed, "colour color"),
        cmd!("Culling", "Yellow label", LabelYellow, "colour color"),
        cmd!("Culling", "Green label", LabelGreen, "colour color"),
        cmd!("Culling", "Blue label", LabelBlue, "colour color"),
        cmd!("Culling", "Pick best of each burst", PickBestOfBursts, "sharpest smart"),
        cmd!("Culling", "Reject soft photos", RejectSoftPhotos, "blurry focus smart"),
        cmd!("Culling", "Reject bad exposures", RejectBadExposures, "dark bright smart"),
        cmd!("Develop", "Auto tone", AutoTone, "automatic exposure"),
        cmd!("Develop", "White balance picker", ToolWhiteBalance, "eyedropper neutral"),
        cmd!("Develop", "Crop", ToolCrop, "straighten"),
        cmd!("Develop", "Spot removal", ToolSpotRemoval, "heal clone"),
        cmd!("Develop", "Content-aware remove", ToolRemove, "erase object brush"),
        cmd!("Develop", "Linear gradient", ToolLinearGradient, "mask"),
        cmd!("Develop", "Radial gradient", ToolRadialGradient, "mask"),
        cmd!("Develop", "Sync settings\u{2026}", SyncSettings, "copy paste apply"),
        cmd!("Develop", "Match look to a photo\u{2026}", MatchLook, "reference style"),
        cmd!("Develop", "New preset from a photo\u{2026}", PresetFromPhoto, "reference style"),
        cmd!("Photo", "Create virtual copy", CreateVirtualCopy, "duplicate version"),
        cmd!("Photo", "Rename photo\u{2026}", RenamePhoto, "file name"),
        cmd!("Photo", "Rotate left", RotateLeft),
        cmd!("Photo", "Rotate right", RotateRight),
        cmd!("Photo", "Stack selected photos", StackSelected, "group"),
        cmd!("Photo", "Auto-stack by capture time", AutoStack, "group"),
        cmd!("Photo", "Unstack", Unstack),
        cmd!("Photo", "Collapse / expand stack", ToggleStack),
        cmd!("Photo", "Reveal in Finder", RevealInFinder, "explorer show folder"),
        cmd!("Photo", "Find duplicates\u{2026}", FindDuplicates, "similar"),
        cmd!("Photo", "Remove from catalog", DeleteFromCatalog, "delete"),
        cmd!("Photo", "Move to Trash\u{2026}", DeleteFromDisk, "delete bin"),
        cmd!("Catalog", "Synchronize folders", SyncFolders, "rescan refresh"),
        cmd!("Catalog", "Restore removed photos", RestoreRemoved),
        cmd!("Catalog", "Back up catalog\u{2026}", BackupCatalog),
        cmd!("Catalog", "Restore catalog from backup\u{2026}", RestoreCatalog),
        cmd!("Catalog", "Optimize catalog", OptimizeCatalog),
        cmd!("Catalog", "Clear thumbnail & preview cache", ClearCache),
    ]
}

/// A key written the platform's way.
fn key_label(key: &str) -> String {
    let mac = cfg!(target_os = "macos");
    match key {
        "left" => "\u{2190}".into(),
        "right" => "\u{2192}".into(),
        "up" => "\u{2191}".into(),
        "down" => "\u{2193}".into(),
        "backspace" => if mac { "\u{232B}" } else { "Backspace" }.into(),
        "escape" => "Esc".into(),
        "enter" => if mac { "\u{21A9}" } else { "Enter" }.into(),
        "space" => "Space".into(),
        "pageup" => "Page Up".into(),
        "pagedown" => "Page Down".into(),
        "home" => "Home".into(),
        "end" => "End".into(),
        "tab" => "Tab".into(),
        other if other.len() == 1 => other.to_uppercase(),
        other => {
            let mut chars = other.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        }
    }
}

/// The shortcut for `action`, written the platform's way, if it has one.
/// The first binding is the one shown (e.g. G rather than Esc for Library).
pub(crate) fn shortcut_label(cx: &App, action: &dyn Action) -> Option<String> {
    let keymap = cx.key_bindings();
    let keymap = keymap.borrow();
    let binding = keymap.bindings_for_action(action).next()?;
    let mac = cfg!(target_os = "macos");
    let strokes: Vec<String> = binding
        .keystrokes()
        .iter()
        .map(|stroke| {
            let m = stroke.modifiers();
            let key = key_label(stroke.key());
            if mac {
                let mut out = String::new();
                if m.control {
                    out.push('\u{2303}');
                }
                if m.alt {
                    out.push('\u{2325}');
                }
                if m.shift {
                    out.push('\u{21E7}');
                }
                if m.platform {
                    out.push('\u{2318}');
                }
                out + &key
            } else {
                let mut parts: Vec<String> = Vec::new();
                if m.control {
                    parts.push("Ctrl".into());
                }
                if m.alt {
                    parts.push("Alt".into());
                }
                if m.shift {
                    parts.push("Shift".into());
                }
                if m.platform {
                    parts.push("Win".into());
                }
                parts.push(key);
                parts.join("+")
            }
        })
        .collect();
    Some(strokes.join(" "))
}

/// How well `query` matches a command (higher is better), or `None` when
/// it doesn't. Every word of the query has to appear in the label, group or
/// keywords; label prefixes and word starts rank highest.
fn score(query: &str, command: &Command) -> Option<i32> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(0);
    }
    let label = command.label.to_lowercase();
    let haystack = format!("{label} {} {}", command.group.to_lowercase(), command.keywords);
    let mut total = 0;
    for word in query.split_whitespace() {
        if label.starts_with(word) {
            total += 30;
        } else if label.split_whitespace().any(|w| w.starts_with(word)) {
            total += 20;
        } else if haystack.split_whitespace().any(|w| w.starts_with(word)) {
            total += 10;
        } else if haystack.contains(word) {
            total += 4;
        } else {
            return None;
        }
    }
    Some(total)
}

/// The commands matching `query`, best first.
fn matching(query: &str) -> Vec<Command> {
    let mut found: Vec<(i32, usize, Command)> = commands()
        .into_iter()
        .enumerate()
        .filter_map(|(i, c)| Some((score(query, &c)?, i, c)))
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    found.into_iter().map(|(_, _, c)| c).collect()
}

/// Most matches listed at once.
const MAX_SHOWN: usize = 12;

impl Workspace {
    pub fn toggle_command_palette(
        &mut self,
        _: &ToggleCommandPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.palette.is_some() {
            self.close_command_palette(cx);
        } else if !self.settings.appearance.command_palette {
            self.status_message =
                Some("The command palette is off. Turn it on in Settings \u{25B8} Appearance.".into());
            cx.notify();
        } else {
            self.shortcut_sheet = false;
            self.palette = Some(0);
            self.begin_text_draft(TextTarget::CommandPalette, window, cx);
        }
    }

    pub(crate) fn close_command_palette(&mut self, cx: &mut Context<Self>) {
        self.palette = None;
        if self.text_draft.as_ref().is_some_and(|(t, _)| *t == TextTarget::CommandPalette) {
            self.text_draft = None;
        }
        cx.notify();
    }

    pub fn toggle_shortcuts(&mut self, _: &ToggleShortcuts, _w: &mut Window, cx: &mut Context<Self>) {
        if !self.shortcut_sheet && !self.settings.appearance.shortcut_sheet {
            self.status_message = Some(
                "The shortcut sheet is off. Turn it on in Settings \u{25B8} Appearance.".into(),
            );
        } else {
            self.shortcut_sheet = !self.shortcut_sheet;
        }
        cx.notify();
    }

    /// Keys while the palette is open: ↑↓ move, Enter runs, Esc closes.
    /// Returns whether the key was the palette's.
    pub(crate) fn palette_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(selected) = self.palette else {
            return false;
        };
        let query = self.draft_for(TextTarget::CommandPalette).unwrap_or_default().to_string();
        let count = matching(&query).len().min(MAX_SHOWN);
        match key {
            "down" => {
                self.palette = Some(if count == 0 { 0 } else { (selected + 1) % count });
            }
            "up" => {
                self.palette = Some(if count == 0 {
                    0
                } else {
                    (selected + count - 1) % count
                });
            }
            "enter" => {
                self.run_palette_command(selected, window, cx);
                return true;
            }
            "escape" => {
                self.close_command_palette(cx);
                return true;
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    /// Runs the `index`th match and closes the palette. The action goes
    /// through the normal dispatch, after this event is done.
    fn run_palette_command(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.draft_for(TextTarget::CommandPalette).unwrap_or_default().to_string();
        let found = matching(&query);
        let Some(command) = found.get(index) else {
            return;
        };
        let action = (command.action)();
        self.close_command_palette(cx);
        window.dispatch_action(action, cx);
    }

    /// Typing changed the query: the best match is selected again.
    pub(crate) fn palette_query_changed(&mut self) {
        if self.palette.is_some() {
            self.palette = Some(0);
        }
    }

    pub(crate) fn render_command_palette(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let selected = self.palette?;
        let theme = self.theme;
        let query = self.draft_for(TextTarget::CommandPalette).unwrap_or_default().to_string();
        let found = matching(&query);
        let total = found.len();
        let rows: Vec<AnyElement> = found
            .iter()
            .take(MAX_SHOWN)
            .enumerate()
            .map(|(i, command)| {
                let shortcut = shortcut_label(cx, (command.action)().as_ref());
                let active = i == selected;
                div()
                    .id(SharedString::from(format!("palette-row-{i}")))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .h(sp(32.0))
                    .px_3()
                    .rounded_md()
                    .cursor_pointer()
                    .when(active, |el| el.bg(theme.accent_muted))
                    .when(!active, |el| el.hover(|el| el.bg(theme.surface_elevated)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(sp(0.0))
                            .truncate()
                            .text_size(sp(13.0))
                            .text_color(theme.text)
                            .child(command.label),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(sp(11.0))
                            .text_color(theme.text_faint)
                            .child(command.group),
                    )
                    .when_some(shortcut, |el, keys| el.child(key_chip(keys, &theme)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.run_palette_command(i, window, cx)
                    }))
                    .into_any_element()
            })
            .collect();

        let input = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(sp(44.0))
            .px_4()
            .border_b_1()
            .border_color(theme.border_subtle)
            .child(rustyedit_ui::icon(rustyedit_ui::Icon::Search, 15.0, theme.text_muted))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .flex_1()
                    .min_w(sp(0.0))
                    .text_size(sp(14.0))
                    .when(query.is_empty(), |el| {
                        el.child(div().w(sp(1.5)).h(sp(16.0)).bg(theme.accent)).child(
                            div()
                                .ml_1()
                                .text_color(theme.text_faint)
                                .child("Type a command\u{2026}"),
                        )
                    })
                    .when(!query.is_empty(), |el| {
                        el.child(div().text_color(theme.text).child(query.clone()))
                            .child(div().w(sp(1.5)).h(sp(16.0)).ml(sp(1.0)).bg(theme.accent))
                    }),
            );

        let footer = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .h(sp(30.0))
            .px_4()
            .border_t_1()
            .border_color(theme.border_subtle)
            .text_size(sp(10.5))
            .text_color(theme.text_faint)
            .child("\u{2191}\u{2193} choose \u{00B7} \u{21A9} run \u{00B7} Esc close")
            .child(if total > MAX_SHOWN {
                format!("{MAX_SHOWN} of {total}")
            } else {
                format!("{total} command{}", if total == 1 { "" } else { "s" })
            });

        let card = div()
            .id("palette-card")
            .flex()
            .flex_col()
            .w(sp(560.0))
            .rounded_lg()
            .overflow_hidden()
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _w, cx| cx.stop_propagation())
            .child(input)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p_1()
                    .children(rows)
                    .when(total == 0, |el| {
                        el.child(
                            div()
                                .py_6()
                                .flex()
                                .justify_center()
                                .text_sm()
                                .text_color(theme.text_faint)
                                .child("No command matches"),
                        )
                    }),
            )
            .child(footer);

        Some(
            div()
                .id("palette-backdrop")
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .flex_col()
                .items_center()
                .pt(sp(90.0))
                .bg(gpui::black().opacity(0.35))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _w, cx| this.close_command_palette(cx)),
                )
                .child(card)
                .into_any_element(),
        )
    }

    pub(crate) fn render_shortcut_sheet(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.shortcut_sheet {
            return None;
        }
        let theme = self.theme;
        let all = commands();
        let groups: Vec<AnyElement> = GROUPS
            .iter()
            .filter_map(|&group| {
                let rows: Vec<AnyElement> = all
                    .iter()
                    .filter(|c| c.group == group)
                    .filter_map(|c| {
                        let keys = shortcut_label(cx, (c.action)().as_ref())?;
                        Some(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .h(sp(24.0))
                                .child(
                                    div()
                                        .min_w(sp(0.0))
                                        .truncate()
                                        .text_size(sp(12.0))
                                        .text_color(theme.text_muted)
                                        .child(c.label),
                                )
                                .child(key_chip(keys, &theme))
                                .into_any_element(),
                        )
                    })
                    .collect();
                (!rows.is_empty()).then(|| {
                    div()
                        .flex()
                        .flex_col()
                        .w(sp(260.0))
                        .gap_0p5()
                        .child(
                            div()
                                .pb_1()
                                .text_size(sp(10.5))
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(theme.accent)
                                .child(group.to_uppercase()),
                        )
                        .children(rows)
                        .into_any_element()
                })
            })
            .collect();

        let card = div()
            .id("shortcuts-card")
            .flex()
            .flex_col()
            .max_w(sp(900.0))
            .max_h(sp(640.0))
            .rounded_lg()
            .overflow_hidden()
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _w, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(sp(48.0))
                    .px_5()
                    .border_b_1()
                    .border_color(theme.border_subtle)
                    .child(
                        div()
                            .text_size(sp(15.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child("Keyboard shortcuts"),
                    )
                    .child(
                        div()
                            .text_size(sp(11.0))
                            .text_color(theme.text_faint)
                            .child(format!(
                                "Everything else: {} \u{00B7} ? or Esc to close",
                                shortcut_label(cx, &ToggleCommandPalette).unwrap_or_default()
                            )),
                    ),
            )
            .child(
                div()
                    .id("shortcuts-body")
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_x_8()
                    .gap_y_5()
                    .p_5()
                    .overflow_y_scroll()
                    .children(groups),
            );

        Some(
            div()
                .id("shortcuts-backdrop")
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .bg(gpui::black().opacity(0.5))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _w, cx| {
                        this.shortcut_sheet = false;
                        cx.notify();
                    }),
                )
                .child(card)
                .into_any_element(),
        )
    }
}

/// A shortcut drawn as a small keycap.
fn key_chip(keys: String, theme: &Theme) -> AnyElement {
    div()
        .flex_shrink_0()
        .px_1p5()
        .h(sp(20.0))
        .flex()
        .items_center()
        .rounded_sm()
        .bg(theme.background)
        .border_1()
        .border_color(theme.border)
        .text_size(sp(11.0))
        .text_color(theme.text)
        .child(keys)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searching_finds_commands_by_label_group_and_keyword() {
        let first = |q: &str| matching(q).first().map(|c| c.label);
        assert_eq!(first("auto t"), Some("Auto tone"));
        assert_eq!(first("virtual"), Some("Create virtual copy"));
        // By keyword: "blurry" finds rejecting soft photos.
        assert_eq!(first("blurry"), Some("Reject soft photos"));
        assert!(matching("zzzz").is_empty());
        assert_eq!(matching("").len(), commands().len());
    }

    #[test]
    fn every_group_is_listed() {
        for command in commands() {
            assert!(GROUPS.contains(&command.group), "{}", command.group);
        }
    }
}
