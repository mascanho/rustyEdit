//! Right-click menu for photos, available wherever a photo is shown: grid
//! cards, filmstrip frames, compare panes, survey tiles, the loupe and the
//! Develop viewer. Entries act on the whole multi-selection when the clicked
//! photo is part of one, otherwise on the clicked photo.
//!
//! Layout, top to bottom: the photo (thumbnail, name, details); the quick
//! row of flag, stars and colour label; opening; edits; organising (albums
//! and XMP in submenus); the file; and, last and in red, removing it.
//! Each action appears once. Shortcuts shown are the live key bindings,
//! written the platform's way.
//!
//! Keyboard: ↑↓ move, → opens a submenu, ← closes it, Enter runs, Esc
//! closes. While it's open, P / X / U, 0-5 and 6-9 flag, rate and label
//! straight away, as they do outside it.

use rustyedit_ui::{sp, Icon, Theme};
use std::{path::PathBuf, rc::Rc};

use gpui::{
    div, img, prelude::*, px, Action, AnyElement, App, ClickEvent, ClipboardItem, Context,
    KeyDownEvent, MouseButton, MouseDownEvent, ObjectFit, Pixels, Point, SharedString, Window,
};
use rustyedit_core::{ColorLabel, ExportScope, Flag};

use crate::{
    actions::{
        CreateVirtualCopy, DeleteFromCatalog, DeleteFromDisk, FlagPick, FlagReject, FlagUnflag,
        GoDevelop, RenamePhoto, RevealInFinder, SyncSettings,
    },
    selection::CullOp,
    workspace::{LibraryView, Mode, Workspace},
};

const MENU_WIDTH: f32 = 264.0;
const SUBMENU_WIDTH: f32 = 230.0;
const ROW_HEIGHT: f32 = 28.0;
const QUICK_HEIGHT: f32 = 32.0;
const HEADER_HEIGHT: f32 = 54.0;
const SEPARATOR_HEIGHT: f32 = 9.0;
/// The menu's own padding (`p_1`) on each side.
const MENU_PAD: f32 = 4.0;

/// An open context menu: where it was opened, and the row the keyboard (or
/// the pointer) is on.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ContextMenu {
    pub position: Point<Pixels>,
    /// Index into the entries of the highlighted row.
    focus: Option<usize>,
    /// The submenu that's open, and its highlighted row.
    submenu: Option<OpenSubmenu>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct OpenSubmenu {
    /// Entry index of the row it hangs off.
    at: usize,
    focus: Option<usize>,
}

type Run = Rc<dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

struct Item {
    id: String,
    icon: Icon,
    label: String,
    hint: Option<String>,
    checked: bool,
    danger: bool,
    run: Run,
}

enum Entry {
    Header {
        title: String,
        subtitle: String,
        thumb: Option<PathBuf>,
        count: usize,
    },
    Separator,
    /// Pick / Unflag / Reject as one segmented row.
    Flags { current: Option<Flag> },
    Stars { current: Option<u8> },
    Swatches { current: Option<ColorLabel> },
    Action(Item),
    Submenu {
        id: String,
        icon: Icon,
        label: String,
        /// Shown faintly at the right, e.g. how many albums.
        detail: Option<String>,
        items: Vec<Item>,
    },
}

impl Entry {
    fn height(&self) -> f32 {
        match self {
            Entry::Header { .. } => HEADER_HEIGHT,
            Entry::Separator => SEPARATOR_HEIGHT,
            Entry::Flags { .. } | Entry::Stars { .. } | Entry::Swatches { .. } => QUICK_HEIGHT,
            Entry::Action(_) | Entry::Submenu { .. } => ROW_HEIGHT,
        }
    }

    /// Rows the keyboard moves between.
    fn navigable(&self) -> bool {
        matches!(self, Entry::Action(_) | Entry::Submenu { .. })
    }
}

fn item(
    id: impl Into<String>,
    icon: Icon,
    label: impl Into<String>,
    run: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
) -> Item {
    Item {
        id: id.into(),
        icon,
        label: label.into(),
        hint: None,
        checked: false,
        danger: false,
        run: Rc::new(run),
    }
}

impl Item {
    /// Shows `action`'s shortcut, if it has one.
    fn keys(mut self, cx: &App, action: &dyn Action) -> Self {
        self.hint = crate::commands::shortcut_label(cx, action);
        self
    }

    fn checked(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }

    fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    fn entry(self) -> Entry {
        Entry::Action(self)
    }
}

/// Runs `action` as if its shortcut were pressed, after the menu closes.
fn dispatch(action: impl Action + Clone) -> impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) {
    move |_this, window, cx| window.dispatch_action(action.boxed_clone(), cx)
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        one.to_string()
    } else {
        format!("{n} {many}")
    }
}

impl Workspace {
    /// Opens the menu for the photo at `ix`. Clicking inside a multi-
    /// selection keeps it; clicking anything else selects just that photo.
    pub(crate) fn open_context_menu(
        &mut self,
        ix: usize,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if !self.in_selection(ix) {
            self.selection.clear();
        }
        if self.catalog.selected != ix {
            self.catalog.select(ix);
            self.on_selection_changed(cx);
        }
        self.bottom_menu = None;
        self.card_info = None;
        self.context_menu = Some(ContextMenu {
            position,
            focus: None,
            submenu: None,
        });
        cx.notify();
    }

    /// Attach to any element showing photo `ix`: right-click opens the menu.
    pub(crate) fn right_click_listener(
        entity: gpui::Entity<Self>,
        ix: usize,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static {
        move |event: &MouseDownEvent, _window, cx| {
            let position = event.position;
            entity.update(cx, |this, cx| this.open_context_menu(ix, position, cx));
        }
    }

    // --- Building the entries --------------------------------------------------

    fn context_entries(&self, cx: &App) -> Vec<Entry> {
        let targets = self.target_indices();
        let count = targets.len();
        let assets: Vec<_> = targets.iter().map(|&ix| &self.catalog.assets[ix]).collect();
        let Some(first) = assets.first() else {
            return Vec::new();
        };
        let multi = count > 1;
        let same = |f: &dyn Fn(&rustyedit_core::Asset) -> u32| {
            let v = f(first);
            assets.iter().all(|a| f(a) == v)
        };
        let same_flag = same(&|a| a.flag as u32).then_some(first.flag);
        let same_rating = same(&|a| a.rating.value() as u32).then(|| first.rating.value());
        let same_label = same(&|a| a.color_label as u32).then_some(first.color_label);

        // --- The photo ----------------------------------------------------------
        let subtitle = if multi {
            format!("{count} photos selected")
        } else {
            let mut facts = Vec::new();
            if let Some(year) = first.meta.year() {
                facts.push(year.to_string());
            }
            if !first.meta.camera.trim().is_empty() {
                facts.push(first.meta.camera.trim().to_string());
            }
            if first.width > 0 {
                facts.push(format!("{}\u{00D7}{}", first.width, first.height));
            }
            facts.join(" \u{00B7} ")
        };
        let mut entries = vec![
            Entry::Header {
                title: if multi {
                    format!("{} and {} more", first.file_name(), count - 1)
                } else {
                    first.file_name()
                },
                subtitle,
                thumb: self.export_thumbnail(first),
                count,
            },
            Entry::Flags { current: same_flag },
            Entry::Stars {
                current: same_rating,
            },
            Entry::Swatches {
                current: same_label,
            },
            Entry::Separator,
        ];

        // --- Open ---------------------------------------------------------------
        if self.mode != Mode::Develop {
            entries.push(
                item("ctx-develop", Icon::Adjust, "Edit in Develop", dispatch(GoDevelop))
                    .keys(cx, &GoDevelop)
                    .entry(),
            );
        }
        if !(self.mode == Mode::Library && self.library_view == LibraryView::Loupe) {
            entries.push(
                item("ctx-loupe", Icon::ZoomIn, "View in Loupe", |this, _w, cx| {
                    this.mode = Mode::Library;
                    this.set_library_view(LibraryView::Loupe, cx);
                })
                .keys(cx, &crate::actions::ToggleLoupe)
                .entry(),
            );
        }
        let at = self.context_menu.map(|m| m.position).unwrap_or_default();
        let selected = self.catalog.selected;
        entries.push(
            item("ctx-info", Icon::Info, "Photo info", move |this, _w, cx| {
                this.open_card_info(selected, at, cx);
            })
            .entry(),
        );
        entries.push(Entry::Separator);

        // --- Edits --------------------------------------------------------------
        if !multi {
            entries.push(
                item("ctx-copy-edits", Icon::Copy, "Copy edits", |this, _w, cx| {
                    this.copy_edits(cx)
                })
                .entry(),
            );
        }
        if self.edit_clipboard.is_some() {
            entries.push(
                item(
                    "ctx-paste-edits",
                    Icon::Paste,
                    if multi {
                        format!("Paste edits to {count} photos")
                    } else {
                        "Paste edits".to_string()
                    },
                    |this, _w, cx| this.paste_edits(cx),
                )
                .entry(),
            );
        }
        if multi && self.can_sync() {
            entries.push(
                item("ctx-sync", Icon::Sync, "Sync settings\u{2026}", |this, _w, cx| {
                    this.open_sync_dialog(cx)
                })
                .keys(cx, &SyncSettings)
                .entry(),
            );
        }
        if self.mode == Mode::Library && !self.library_presets_open() {
            entries.push(
                item("ctx-presets", Icon::Presets, "Apply a preset\u{2026}", |this, _w, cx| {
                    this.toggle_library_presets(cx)
                })
                .entry(),
            );
        }
        entries.push(
            item(
                "ctx-virtual-copy",
                Icon::Duplicates,
                if multi {
                    "Create virtual copies"
                } else {
                    "Create virtual copy"
                },
                |this, _w, cx| this.create_virtual_copies(cx),
            )
            .keys(cx, &CreateVirtualCopy)
            .entry(),
        );
        if assets.iter().any(|a| a.develop.is_edited()) {
            entries.push(
                item("ctx-reset-edits", Icon::Reset, "Reset edits", |this, _w, cx| {
                    this.reset_edits(cx);
                })
                .entry(),
            );
        }
        entries.push(Entry::Separator);

        // --- Organise -----------------------------------------------------------
        let paths: Vec<_> = assets.iter().map(|a| a.path.clone()).collect();
        let mut albums: Vec<Item> = self
            .catalog_def
            .albums
            .items
            .iter()
            .map(|album| {
                let id = album.id;
                item(
                    format!("ctx-album-{id}"),
                    Icon::Collections,
                    album.name.clone(),
                    move |this, _w, cx| this.toggle_targets_in_album(id, cx),
                )
                .checked(paths.iter().all(|p| album.contains(p)))
            })
            .collect();
        let album_count = albums.len();
        let new_paths = paths.clone();
        albums.push(item(
            "ctx-album-new",
            Icon::Plus,
            "New album\u{2026}",
            move |this, window, cx| {
                this.pending_album_photos = new_paths.clone();
                this.open_left_tool(rustyedit_core::RailTool::Collections, cx);
                this.begin_text_draft(crate::workspace::TextTarget::Album, window, cx);
            },
        ));
        entries.push(Entry::Submenu {
            id: "ctx-albums".into(),
            icon: Icon::Collections,
            label: "Add to album".into(),
            detail: (album_count > 0).then(|| album_count.to_string()),
            items: albums,
        });
        if let Some(active) = self.active_album {
            let name = self
                .catalog_def
                .albums
                .get(active)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "this album".into());
            entries.push(
                item(
                    "ctx-album-remove",
                    Icon::Minus,
                    format!("Remove from \u{201C}{name}\u{201D}"),
                    move |this, _w, cx| this.remove_targets_from_album(active, cx),
                )
                .entry(),
            );
        }
        let mut sidecar = Vec::new();
        if self.targets_have_sidecars() {
            sidecar.push(item(
                "ctx-read-sidecar",
                Icon::Download,
                "Read metadata from file",
                |this, _w, cx| this.read_from_sidecars(cx),
            ));
        }
        sidecar.push(item(
            "ctx-write-sidecar",
            Icon::Upload,
            "Save metadata to file",
            |this, _w, cx| this.write_sidecars_now(cx),
        ));
        entries.push(Entry::Submenu {
            id: "ctx-sidecar".into(),
            icon: Icon::Sidecar,
            label: "XMP sidecar".into(),
            detail: None,
            items: sidecar,
        });
        entries.push(Entry::Separator);

        // --- The file -----------------------------------------------------------
        entries.push(
            item(
                "ctx-export",
                Icon::Upload,
                if multi {
                    format!("Export {count} photos\u{2026}")
                } else {
                    "Export\u{2026}".to_string()
                },
                |this, _w, cx| {
                    this.export_options.scope = ExportScope::Selected;
                    this.mode = Mode::Export;
                    cx.notify();
                },
            )
            .entry(),
        );
        if !multi {
            entries.push(
                item("ctx-rename", Icon::Edit, "Rename\u{2026}", dispatch(RenamePhoto))
                    .keys(cx, &RenamePhoto)
                    .entry(),
            );
        }
        entries.push(
            item(
                "ctx-reveal",
                Icon::Folder,
                crate::file_ops::reveal_label(),
                dispatch(RevealInFinder),
            )
            .keys(cx, &RevealInFinder)
            .entry(),
        );
        let copy_paths: Vec<String> = assets
            .iter()
            .map(|a| a.path.display().to_string())
            .collect();
        entries.push(
            item(
                "ctx-copy-path",
                Icon::Link,
                if multi { "Copy file paths" } else { "Copy file path" },
                move |this, _w, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(copy_paths.join("\n")));
                    this.status_message = Some("Copied to the clipboard.".into());
                },
            )
            .entry(),
        );
        entries.push(Entry::Separator);

        // --- Removing (last, in red) --------------------------------------------
        let all_marked = assets.iter().all(|a| a.marked_delete);
        entries.push(
            item(
                "ctx-mark-delete",
                Icon::MarkDelete,
                if all_marked {
                    "Unmark for deletion"
                } else {
                    "Mark for deletion"
                },
                |this, _w, cx| this.toggle_mark_for_deletion(cx),
            )
            .checked(all_marked)
            .entry(),
        );
        entries.push(
            item(
                "ctx-remove",
                Icon::RemoveCircle,
                if multi {
                    format!("Remove {count} from catalog")
                } else {
                    "Remove from catalog".to_string()
                },
                dispatch(DeleteFromCatalog),
            )
            .keys(cx, &DeleteFromCatalog)
            .entry(),
        );
        entries.push(
            item(
                "ctx-trash",
                Icon::Trash,
                if multi {
                    format!("Move {count} to Trash\u{2026}")
                } else {
                    "Move to Trash\u{2026}".to_string()
                },
                dispatch(DeleteFromDisk),
            )
            .keys(cx, &DeleteFromDisk)
            .danger()
            .entry(),
        );
        // Catalog-wide clean-up, out of the way in a submenu.
        let marked = self.marked_count();
        let rejected = self
            .catalog
            .assets
            .iter()
            .filter(|a| a.flag == Flag::Rejected && !a.missing)
            .count();
        let mut cleanup = Vec::new();
        if marked > 0 {
            cleanup.push(
                item(
                    "ctx-delete-marked",
                    Icon::MarkDelete,
                    format!("Trash {}\u{2026}", plural(marked, "the marked photo", "marked photos")),
                    |this, _w, cx| this.delete_marked(cx),
                )
                .danger(),
            );
        }
        if rejected > 0 {
            cleanup.push(
                item(
                    "ctx-delete-rejected",
                    Icon::Close,
                    format!(
                        "Trash {}\u{2026}",
                        plural(rejected, "the rejected photo", "rejected photos")
                    ),
                    |this, _w, cx| this.delete_rejected(cx),
                )
                .danger(),
            );
        }
        if !cleanup.is_empty() {
            entries.push(Entry::Submenu {
                id: "ctx-cleanup".into(),
                icon: Icon::Trash,
                label: "Clean up catalog".into(),
                detail: Some((marked + rejected).to_string()),
                items: cleanup,
            });
        }
        entries
    }

    // --- Keyboard ----------------------------------------------------------------

    /// Keys while the menu is open (its key context keeps them from reaching
    /// the usual shortcuts).
    pub(crate) fn on_context_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mut menu) = self.context_menu else {
            return;
        };
        if self.text_draft.is_some() {
            return;
        }
        let stroke = &event.keystroke;
        if stroke.modifiers.platform || stroke.modifiers.control {
            return;
        }
        let key = stroke.key.as_str();
        cx.stop_propagation();

        // The quick row's letters and digits work straight away.
        let quick = match key {
            "p" => Some(CullOp::Flag(Flag::Picked)),
            "x" => Some(CullOp::Flag(Flag::Rejected)),
            "u" => Some(CullOp::Flag(Flag::Unflagged)),
            "0" | "1" | "2" | "3" | "4" | "5" => key.parse().ok().map(CullOp::Rating),
            "6" => Some(CullOp::Label(ColorLabel::Red)),
            "7" => Some(CullOp::Label(ColorLabel::Yellow)),
            "8" => Some(CullOp::Label(ColorLabel::Green)),
            "9" => Some(CullOp::Label(ColorLabel::Blue)),
            _ => None,
        };
        if let Some(op) = quick {
            self.context_menu = None;
            self.apply_cull(op, cx);
            cx.notify();
            return;
        }

        let app: &App = cx;
        let entries = self.context_entries(app);
        let nav: Vec<usize> = (0..entries.len()).filter(|&i| entries[i].navigable()).collect();
        let step = |current: Option<usize>, len: usize, down: bool| -> Option<usize> {
            if len == 0 {
                return None;
            }
            Some(match current {
                None if down => 0,
                None => len - 1,
                Some(i) if down => (i + 1) % len,
                Some(i) => (i + len - 1) % len,
            })
        };
        let submenu_items = |at: usize| match entries.get(at) {
            Some(Entry::Submenu { items, .. }) => Some(items),
            _ => None,
        };

        let mut run: Option<Run> = None;
        match key {
            "escape" => {
                if menu.submenu.take().is_none() {
                    self.context_menu = None;
                    cx.notify();
                    return;
                }
            }
            "down" | "up" | "tab" => {
                let down = key != "up" && !(key == "tab" && stroke.modifiers.shift);
                match menu.submenu.as_mut() {
                    Some(sub) => {
                        let len = submenu_items(sub.at).map_or(0, |i| i.len());
                        sub.focus = step(sub.focus, len, down);
                    }
                    None => {
                        let at = menu.focus.and_then(|f| nav.iter().position(|&i| i == f));
                        menu.focus = step(at, nav.len(), down).map(|p| nav[p]);
                    }
                }
            }
            "right" => {
                if let Some(at) = menu.focus.filter(|&f| submenu_items(f).is_some()) {
                    menu.submenu = Some(OpenSubmenu { at, focus: Some(0) });
                }
            }
            "left" => menu.submenu = None,
            "enter" | "space" => match (menu.submenu, menu.focus) {
                (Some(sub), _) => {
                    run = sub
                        .focus
                        .and_then(|f| submenu_items(sub.at)?.get(f))
                        .map(|item| item.run.clone());
                }
                (None, Some(f)) => match entries.get(f) {
                    Some(Entry::Action(item)) => run = Some(item.run.clone()),
                    Some(Entry::Submenu { .. }) => {
                        menu.submenu = Some(OpenSubmenu { at: f, focus: Some(0) })
                    }
                    _ => {}
                },
                _ => {}
            },
            _ => {}
        }
        drop(entries);
        match run {
            Some(run) => {
                self.context_menu = None;
                run(self, window, cx);
            }
            None => self.context_menu = Some(menu),
        }
        cx.notify();
    }

    // --- Actions ---------------------------------------------------------------

    /// Adds the targets to `album`, or removes them all if they're already
    /// all in it.
    fn toggle_targets_in_album(&mut self, album: u64, cx: &mut Context<Self>) {
        let paths: Vec<_> = self
            .target_indices()
            .into_iter()
            .map(|ix| self.catalog.assets[ix].path.clone())
            .collect();
        let all_in = self
            .catalog_def
            .albums
            .get(album)
            .is_some_and(|a| paths.iter().all(|p| a.contains(p)));
        for path in &paths {
            let is_in = self
                .catalog_def
                .albums
                .get(album)
                .is_some_and(|a| a.contains(path));
            if is_in == all_in {
                self.catalog_def.albums.toggle(album, path);
            }
        }
        self.after_album_change(album, cx);
    }

    fn remove_targets_from_album(&mut self, album: u64, cx: &mut Context<Self>) {
        let paths: Vec<_> = self
            .target_indices()
            .into_iter()
            .map(|ix| self.catalog.assets[ix].path.clone())
            .collect();
        for path in &paths {
            if self
                .catalog_def
                .albums
                .get(album)
                .is_some_and(|a| a.contains(path))
            {
                self.catalog_def.albums.toggle(album, path);
            }
        }
        self.after_album_change(album, cx);
    }

    fn after_album_change(&mut self, album: u64, cx: &mut Context<Self>) {
        self.save_catalog_def();
        if self.active_album == Some(album) {
            self.view_album(Some(album), cx);
        }
        cx.notify();
    }

    // --- Rendering -------------------------------------------------------------

    pub(crate) fn render_context_menu(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let Some(menu) = self.context_menu else {
            return div().into_any_element();
        };
        let app: &App = cx;
        let entries = self.context_entries(app);
        let height = entries.iter().map(Entry::height).sum::<f32>() + MENU_PAD * 2.0 + 2.0;

        // Keep the whole menu on screen.
        let viewport = window.viewport_size();
        let (vw, vh) = (f32::from(viewport.width), f32::from(viewport.height));
        let x = f32::from(menu.position.x).min(vw - MENU_WIDTH - 8.0).max(8.0);
        let y = f32::from(menu.position.y).min(vh - height - 8.0).max(8.0);

        // Where each row starts, for hanging a submenu off it.
        let mut tops = Vec::with_capacity(entries.len());
        let mut top = MENU_PAD + 1.0;
        for entry in &entries {
            tops.push(top);
            top += entry.height();
        }

        let mut submenu_panel: Option<AnyElement> = None;
        let mut rows: Vec<AnyElement> = Vec::with_capacity(entries.len());
        for (ix, entry) in entries.into_iter().enumerate() {
            let focused = menu.focus == Some(ix);
            rows.push(match entry {
                Entry::Header {
                    title,
                    subtitle,
                    thumb,
                    count,
                } => header_row(title, subtitle, thumb, count, &theme),
                Entry::Separator => div()
                    .h(px(SEPARATOR_HEIGHT))
                    .px_1()
                    .flex()
                    .items_center()
                    .child(div().h(sp(1.0)).w_full().bg(theme.border_subtle))
                    .into_any_element(),
                Entry::Flags { current } => self.flags_row(current, cx),
                Entry::Stars { current } => self.stars_row(current, cx),
                Entry::Swatches { current } => self.swatches_row(current, cx),
                Entry::Action(item) => {
                    let row = menu_row(&item, focused, false, None, &theme);
                    let run = item.run.clone();
                    row.on_hover(cx.listener(move |this, hovered: &bool, _w, cx| {
                        if let Some(menu) = this.context_menu.as_mut().filter(|_| *hovered) {
                            menu.focus = Some(ix);
                            menu.submenu = None;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.context_menu = None;
                        run(this, window, cx);
                        cx.notify();
                    }))
                    .into_any_element()
                }
                Entry::Submenu {
                    id,
                    icon,
                    label,
                    detail,
                    items,
                } => {
                    let open = menu.submenu.filter(|s| s.at == ix);
                    let head = Item {
                        id,
                        icon,
                        label,
                        hint: None,
                        checked: false,
                        danger: false,
                        run: Rc::new(|_, _, _| {}),
                    };
                    if let Some(sub) = open {
                        submenu_panel =
                            Some(self.submenu_panel(items, sub, (x, y + tops[ix]), (vw, vh), cx));
                    }
                    menu_row(&head, focused || open.is_some(), true, detail, &theme)
                        .on_hover(cx.listener(move |this, hovered: &bool, _w, cx| {
                            if let Some(menu) = this.context_menu.as_mut().filter(|_| *hovered) {
                                menu.focus = Some(ix);
                                if menu.submenu.map(|s| s.at) != Some(ix) {
                                    menu.submenu = Some(OpenSubmenu { at: ix, focus: None });
                                }
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            if let Some(menu) = this.context_menu.as_mut() {
                                menu.submenu = Some(OpenSubmenu { at: ix, focus: Some(0) });
                                cx.notify();
                            }
                        }))
                        .into_any_element()
                }
            });
        }

        let close = |this: &mut Workspace, _: &MouseDownEvent, _w: &mut Window, cx: &mut Context<Workspace>| {
            this.context_menu = None;
            cx.notify();
        };

        div()
            .id("context-menu-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_mouse_down(MouseButton::Left, cx.listener(close))
            .on_mouse_down(MouseButton::Right, cx.listener(close))
            .child(
                div()
                    .id("context-menu")
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(MENU_WIDTH))
                    .occlude()
                    .p(px(MENU_PAD))
                    .flex()
                    .flex_col()
                    .rounded_lg()
                    .bg(theme.surface_elevated)
                    .border_1()
                    .border_color(theme.border)
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(rows),
            )
            .children(submenu_panel)
            .into_any_element()
    }

    /// A submenu beside its row: to the right, or the left if that's off
    /// screen.
    fn submenu_panel(
        &self,
        items: Vec<Item>,
        open: OpenSubmenu,
        (menu_x, row_y): (f32, f32),
        (vw, vh): (f32, f32),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let height = items.len() as f32 * ROW_HEIGHT + MENU_PAD * 2.0 + 2.0;
        let right = menu_x + MENU_WIDTH - 2.0;
        let x = if right + SUBMENU_WIDTH <= vw - 8.0 {
            right
        } else {
            (menu_x - SUBMENU_WIDTH + 2.0).max(8.0)
        };
        let y = (row_y - MENU_PAD - 1.0).min(vh - height - 8.0).max(8.0);
        let rows = items.into_iter().enumerate().map(|(i, item)| {
            let run = item.run.clone();
            menu_row(&item, open.focus == Some(i), false, None, &theme)
                .on_hover(cx.listener(move |this, hovered: &bool, _w, cx| {
                    if let Some(sub) = this
                        .context_menu
                        .as_mut()
                        .and_then(|m| m.submenu.as_mut())
                        .filter(|_| *hovered)
                    {
                        sub.focus = Some(i);
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.context_menu = None;
                    run(this, window, cx);
                    cx.notify();
                }))
        });
        div()
            .id("context-submenu")
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(SUBMENU_WIDTH))
            .max_h(px(vh - 16.0))
            .overflow_y_scroll()
            .occlude()
            .p(px(MENU_PAD))
            .flex()
            .flex_col()
            .rounded_lg()
            .bg(theme.surface_elevated)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(rows)
            .into_any_element()
    }

    /// Pick / Unflag / Reject, side by side; the current one lit.
    fn flags_row(&self, current: Option<Flag>, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let app: &App = cx;
        let button = |flag: Flag, label: &'static str, icon: Icon, color: gpui::Hsla, action: &dyn Action| {
            let on = current == Some(flag);
            let tip = match crate::commands::shortcut_label(app, action) {
                Some(keys) => format!("{label} ({keys})"),
                None => label.to_string(),
            };
            div()
                .id(SharedString::from(format!("ctx-flag-{label}")))
                .flex_1()
                .flex()
                .flex_row()
                .items_center()
                .justify_center()
                .gap_1()
                .h(sp(24.0))
                .rounded_md()
                .cursor_pointer()
                .text_size(sp(11.0))
                .when(on, |el| el.bg(color.opacity(0.18)).text_color(theme.text))
                .when(!on, |el| {
                    el.text_color(theme.text_muted)
                        .hover(|el| el.bg(theme.surface).text_color(theme.text))
                })
                .child(rustyedit_ui::icon(icon, 12.0, if on { color } else { theme.text_faint }))
                .child(label)
                .tooltip(rustyedit_ui::tip(tip, &theme))
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.context_menu = None;
                    this.apply_cull(CullOp::Flag(flag), cx);
                }))
        };
        div()
            .h(px(QUICK_HEIGHT))
            .flex()
            .flex_row()
            .items_center()
            .gap_0p5()
            .p_0p5()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .w_full()
                    .gap_0p5()
                    .p_0p5()
                    .rounded_md()
                    .bg(theme.background.opacity(0.6))
                    .child(button(Flag::Picked, "Pick", Icon::Flag, theme.pick, &FlagPick))
                    .child(button(Flag::Unflagged, "Unflag", Icon::Minus, theme.text_muted, &FlagUnflag))
                    .child(button(Flag::Rejected, "Reject", Icon::Close, theme.reject, &FlagReject)),
            )
            .into_any_element()
    }

    /// Five stars to click, and Clear.
    fn stars_row(&self, current: Option<u8>, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        div()
            .h(px(QUICK_HEIGHT))
            .flex()
            .flex_row()
            .items_center()
            .gap_0p5()
            .px_2()
            .child(quick_label("Rating", &theme))
            .children((1..=5u8).map(|star| {
                let filled = current.is_some_and(|c| star <= c);
                div()
                    .id(SharedString::from(format!("ctx-star-{star}")))
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(sp(22.0))
                    .rounded_sm()
                    .cursor_pointer()
                    .text_size(sp(15.0))
                    .text_color(if filled { theme.star } else { theme.text_faint })
                    .hover(|el| el.text_color(theme.star).bg(theme.surface))
                    .tooltip(rustyedit_ui::tip(format!("{star} star{} ({star})", if star == 1 { "" } else { "s" }), &theme))
                    .child(if filled { "\u{2605}" } else { "\u{2606}" })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.context_menu = None;
                        this.apply_cull(CullOp::Rating(star), cx);
                    }))
            }))
            .child(div().flex_1())
            .when(current.is_none_or(|c| c > 0), |el| {
                el.child(clear_chip("ctx-star-clear", "Clear (0)", &theme).on_click(cx.listener(
                    |this, _: &ClickEvent, _w, cx| {
                        this.context_menu = None;
                        this.apply_cull(CullOp::Rating(0), cx);
                    },
                )))
            })
            .into_any_element()
    }

    /// The colour labels, and None.
    fn swatches_row(&self, current: Option<ColorLabel>, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        div()
            .h(px(QUICK_HEIGHT))
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .px_2()
            .child(quick_label("Label", &theme))
            .children(ColorLabel::ALL.into_iter().filter_map(|label| {
                let color = theme.color_label(label)?;
                let active = current == Some(label);
                Some(
                    div()
                        .id(SharedString::from(format!("ctx-label-{}", label.label())))
                        .size(sp(16.0))
                        .rounded_full()
                        .bg(color)
                        .border_2()
                        .border_color(if active { theme.text } else { color.opacity(0.0) })
                        .cursor_pointer()
                        .hover(|el| el.border_color(theme.text_muted))
                        .tooltip(rustyedit_ui::tip(label.label(), &theme))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            this.context_menu = None;
                            this.apply_cull(CullOp::Label(label), cx);
                        })),
                )
            }))
            .child(div().flex_1())
            .when(current != Some(ColorLabel::None), |el| {
                el.child(clear_chip("ctx-label-clear", "None", &theme).on_click(cx.listener(
                    |this, _: &ClickEvent, _w, cx| {
                        this.context_menu = None;
                        this.apply_cull(CullOp::Label(ColorLabel::None), cx);
                    },
                )))
            })
            .into_any_element()
    }
}

/// The photo the menu is for: its thumbnail (stacked for a selection),
/// name and a line of details.
fn header_row(
    title: String,
    subtitle: String,
    thumb: Option<PathBuf>,
    count: usize,
    theme: &Theme,
) -> AnyElement {
    let picture = div()
        .relative()
        .flex_shrink_0()
        .size(sp(40.0))
        // A second card peeking out behind says "several".
        .when(count > 1, |el| {
            el.child(
                div()
                    .absolute()
                    .top(sp(-3.0))
                    .left(sp(3.0))
                    .size(sp(40.0))
                    .rounded_md()
                    .bg(theme.border)
                    .border_1()
                    .border_color(theme.surface_elevated),
            )
        })
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded_md()
                .overflow_hidden()
                .bg(gpui::black())
                .border_1()
                .border_color(theme.border)
                .when_some(thumb, |el, path| {
                    el.child(img(path).object_fit(ObjectFit::Cover).size_full())
                }),
        )
        .when(count > 1, |el| {
            el.child(
                div()
                    .absolute()
                    .bottom(sp(-4.0))
                    .right(sp(-4.0))
                    .px_1()
                    .h(sp(14.0))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .bg(theme.accent)
                    .text_size(sp(9.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(gpui::white())
                    .child(count.to_string()),
            )
        });
    div()
        .h(px(HEADER_HEIGHT))
        .flex()
        .flex_row()
        .items_center()
        .gap_2p5()
        .px_2()
        .child(picture)
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .flex_1()
                .min_w(sp(0.0))
                .child(
                    div()
                        .truncate()
                        .text_size(sp(12.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.text)
                        .child(title),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(sp(10.5))
                        .text_color(theme.text_faint)
                        .child(subtitle),
                ),
        )
        .into_any_element()
}

/// One clickable row: icon, label, then the shortcut, a tick or a submenu
/// arrow. Handlers are added by the caller.
fn menu_row(
    item: &Item,
    focused: bool,
    submenu: bool,
    detail: Option<String>,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let base = if item.danger { theme.danger } else { theme.text_muted };
    let strong = if item.danger { theme.danger } else { theme.text };
    let icon_color = if item.danger {
        theme.danger
    } else if focused {
        theme.text
    } else {
        theme.text_faint
    };
    div()
        .id(SharedString::from(item.id.clone()))
        .flex()
        .flex_row()
        .items_center()
        .gap_2p5()
        .h(px(ROW_HEIGHT))
        .px_2()
        .rounded_md()
        .cursor_pointer()
        .text_size(sp(12.5))
        .when(focused, |el| {
            el.bg(if item.danger {
                theme.danger.opacity(0.14)
            } else {
                theme.accent_muted
            })
            .text_color(strong)
        })
        .when(!focused, |el| el.text_color(base))
        .child(rustyedit_ui::icon(item.icon, 14.0, icon_color))
        .child(
            div()
                .flex_1()
                .min_w(sp(0.0))
                .truncate()
                .child(item.label.clone()),
        )
        .when_some(detail, |el, detail| {
            el.child(
                div()
                    .text_size(sp(10.5))
                    .text_color(theme.text_faint)
                    .child(detail),
            )
        })
        .when_some(item.hint.clone(), |el, hint| {
            el.child(
                div()
                    .flex_shrink_0()
                    .text_size(sp(10.5))
                    .text_color(theme.text_faint)
                    .child(hint),
            )
        })
        .when(item.checked, |el| {
            el.child(rustyedit_ui::icon(Icon::Check, 12.0, theme.accent))
        })
        .when(submenu, |el| {
            el.child(rustyedit_ui::icon(Icon::ChevronRight, 11.0, theme.text_faint))
        })
}

/// The small faint name at the start of a quick row.
fn quick_label(text: &'static str, theme: &Theme) -> impl IntoElement {
    div()
        .w(sp(40.0))
        .flex_shrink_0()
        .text_size(sp(10.5))
        .text_color(theme.text_faint)
        .child(text)
}

/// A small text button at the end of a quick row.
fn clear_chip(id: &'static str, text: &'static str, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px_1p5()
        .h(sp(20.0))
        .flex()
        .items_center()
        .rounded_sm()
        .cursor_pointer()
        .text_size(sp(10.5))
        .text_color(theme.text_faint)
        .hover(|el| el.bg(theme.surface).text_color(theme.text))
        .child(text)
}
