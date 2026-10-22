//! Right-click menu for photos, available wherever a photo is shown: grid
//! cards, filmstrip frames, compare panes, survey tiles, the loupe and the
//! Develop viewer. Entries act on the whole multi-selection when the clicked
//! photo is part of one, otherwise on the clicked photo.

use std::rc::Rc;

use gpui::{
    div, prelude::*, px, AnyElement, ClickEvent, ClipboardItem, Context, MouseButton,
    MouseDownEvent, Pixels, Point, SharedString, Window,
};
use rustyedit_core::{ColorLabel, ExportScope, Flag};

use crate::{
    selection::CullOp,
    workspace::{LibraryView, Mode, Workspace},
};

const MENU_WIDTH: f32 = 236.0;
const ROW_HEIGHT: f32 = 28.0;
const SEPARATOR_HEIGHT: f32 = 9.0;
const HEADING_HEIGHT: f32 = 22.0;
const MENU_PADDING: f32 = 8.0;

/// An open context menu: where it was opened.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ContextMenu {
    pub position: Point<Pixels>,
}

type Run = Rc<dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

enum Entry {
    Header(String),
    Separator,
    Heading(&'static str),
    Action {
        id: String,
        icon: &'static str,
        label: String,
        hint: Option<&'static str>,
        checked: bool,
        run: Run,
    },
    /// Five clickable stars; `current` is the shared rating, if any.
    Stars {
        current: Option<u8>,
    },
    Swatches {
        current: Option<ColorLabel>,
    },
}

impl Entry {
    fn height(&self) -> f32 {
        match self {
            Entry::Separator => SEPARATOR_HEIGHT,
            Entry::Heading(_) => HEADING_HEIGHT,
            _ => ROW_HEIGHT,
        }
    }
}

fn action(
    id: impl Into<String>,
    icon: &'static str,
    label: impl Into<String>,
    run: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
) -> Entry {
    Entry::Action {
        id: id.into(),
        icon,
        label: label.into(),
        hint: None,
        checked: false,
        run: Rc::new(run),
    }
}

impl Entry {
    fn hint(mut self, text: &'static str) -> Self {
        if let Entry::Action { hint, .. } = &mut self {
            *hint = Some(text);
        }
        self
    }

    fn checked(mut self, on: bool) -> Self {
        if let Entry::Action { checked, .. } = &mut self {
            *checked = on;
        }
        self
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
        // Right-clicking inside a multi-selection keeps it; anywhere else
        // selects just that photo.
        if !self.in_selection(ix) {
            self.selection.clear();
        }
        if self.catalog.selected != ix {
            self.catalog.select(ix);
            self.on_selection_changed(cx);
        }
        self.bottom_menu = None;
        self.context_menu = Some(ContextMenu { position });
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

    fn context_entries(&self) -> Vec<Entry> {
        let targets = self.target_indices();
        let count = targets.len();
        let assets: Vec<_> = targets.iter().map(|&ix| &self.catalog.assets[ix]).collect();
        let Some(first) = assets.first() else {
            return Vec::new();
        };

        let same_flag = assets
            .iter()
            .all(|a| a.flag == first.flag)
            .then_some(first.flag);
        let same_rating = assets
            .iter()
            .all(|a| a.rating == first.rating)
            .then(|| first.rating.value());
        let same_label = assets
            .iter()
            .all(|a| a.color_label == first.color_label)
            .then_some(first.color_label);
        let path = first.path.clone();
        let multi = count > 1;

        let mut entries = vec![Entry::Header(if multi {
            format!("{count} photos")
        } else {
            first.file_name()
        })];

        // --- Open ---------------------------------------------------------------
        if self.mode != Mode::Develop {
            entries.push(
                action(
                    "ctx-develop",
                    "\u{270E}",
                    "Open in Develop",
                    |this, _w, cx| this.enter_develop(cx),
                )
                .hint("D"),
            );
        }
        if !(self.mode == Mode::Library && self.library_view == LibraryView::Loupe) {
            entries.push(action(
                "ctx-loupe",
                "\u{25FB}",
                "Open in Loupe",
                |this, _w, cx| {
                    this.mode = Mode::Library;
                    this.set_library_view(LibraryView::Loupe, cx);
                },
            ));
        }
        entries.push(
            action("ctx-info", "\u{24D8}", "Show info", |this, _w, cx| {
                this.mode = Mode::Library;
                this.set_library_view(LibraryView::Loupe, cx);
                this.info_visible = true;
                this.request_metadata(cx);
            })
            .hint("I"),
        );
        entries.push(Entry::Separator);

        // --- Cull ---------------------------------------------------------------
        entries.push(
            action("ctx-pick", "\u{2691}", "Pick", |this, _w, cx| {
                this.apply_cull(CullOp::Flag(Flag::Picked), cx)
            })
            .hint("P")
            .checked(same_flag == Some(Flag::Picked)),
        );
        entries.push(
            action("ctx-reject", "\u{2715}", "Reject", |this, _w, cx| {
                this.apply_cull(CullOp::Flag(Flag::Rejected), cx)
            })
            .hint("X")
            .checked(same_flag == Some(Flag::Rejected)),
        );
        entries.push(
            action("ctx-unflag", "\u{25CB}", "Unflag", |this, _w, cx| {
                this.apply_cull(CullOp::Flag(Flag::Unflagged), cx)
            })
            .hint("U")
            .checked(same_flag == Some(Flag::Unflagged)),
        );
        // Not just reject: actually get rid of the photo.
        entries.push(
            action(
                "ctx-trash",
                "\u{2326}",
                if multi {
                    format!("Delete {count} photos\u{2026}")
                } else {
                    "Delete photo\u{2026}".to_string()
                },
                |this, _w, cx| this.delete_selected(true, cx),
            )
            .hint("\u{2318}\u{232B}"),
        );
        entries.push(Entry::Stars {
            current: same_rating,
        });
        entries.push(Entry::Swatches {
            current: same_label,
        });
        entries.push(Entry::Separator);

        // --- Albums -------------------------------------------------------------
        entries.push(Entry::Heading("ADD TO ALBUM"));
        let paths: Vec<_> = assets.iter().map(|a| a.path.clone()).collect();
        for album in &self.catalog_def.albums.items {
            let id = album.id;
            let in_all = paths.iter().all(|p| album.contains(p));
            entries.push(
                action(
                    format!("ctx-album-{id}"),
                    "\u{2750}",
                    album.name.clone(),
                    move |this, _w, cx| this.toggle_targets_in_album(id, cx),
                )
                .checked(in_all),
            );
        }
        let new_paths = paths.clone();
        entries.push(action(
            "ctx-album-new",
            "+",
            "New album\u{2026}",
            move |this, window, cx| {
                this.pending_album_photos = new_paths.clone();
                this.open_left_tool(rustyedit_core::RailTool::Collections, cx);
                this.begin_text_draft(crate::workspace::TextTarget::Album, window, cx);
            },
        ));
        if let Some(active) = self.active_album {
            entries.push(action(
                "ctx-album-remove",
                "\u{2212}",
                "Remove from this album",
                move |this, _w, cx| this.remove_targets_from_album(active, cx),
            ));
        }
        entries.push(Entry::Separator);

        // --- Edits --------------------------------------------------------------
        if !multi {
            entries.push(action(
                "ctx-copy-edits",
                "\u{2398}",
                "Copy edits",
                |this, _w, cx| this.copy_edits(cx),
            ));
        }
        if self.edit_clipboard.is_some() {
            entries.push(action(
                "ctx-paste-edits",
                "\u{2399}",
                "Paste edits",
                |this, _w, cx| this.paste_edits(cx),
            ));
        }
        if multi && self.can_sync() {
            entries.push(
                action(
                    "ctx-sync-settings",
                    "\u{21C4}",
                    "Sync settings\u{2026}",
                    |this, _w, cx| this.open_sync_dialog(cx),
                )
                .hint("\u{21E7}\u{2318}S"),
            );
        }
        entries.push(
            action(
                "ctx-virtual-copy",
                "\u{2750}",
                if multi {
                    "Create virtual copies"
                } else {
                    "Create virtual copy"
                },
                |this, _w, cx| this.create_virtual_copies(cx),
            )
            .hint("\u{2318}'"),
        );
        if assets.iter().any(|a| a.develop.is_edited()) {
            entries.push(action(
                "ctx-reset-edits",
                "\u{21BA}",
                "Reset edits",
                |this, _w, cx| {
                    this.reset_edits(cx);
                },
            ));
        }
        entries.push(Entry::Separator);

        // --- File ---------------------------------------------------------------
        entries.push(action(
            "ctx-export",
            "\u{21E5}",
            "Export\u{2026}",
            |this, _w, cx| {
                this.export_options.scope = ExportScope::Selected;
                this.mode = Mode::Export;
                cx.notify();
            },
        ));
        let reveal = path.clone();
        entries.push(action(
            "ctx-reveal",
            "\u{25A4}",
            "Reveal in Finder",
            move |_this, _w, _cx| {
                let _ = std::process::Command::new("open")
                    .arg("-R")
                    .arg(&reveal)
                    .spawn();
            },
        ));
        let copy_paths: Vec<String> = assets
            .iter()
            .map(|a| a.path.display().to_string())
            .collect();
        entries.push(action(
            "ctx-copy-path",
            "\u{2398}",
            if multi {
                "Copy file paths"
            } else {
                "Copy file path"
            },
            move |_this, _w, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(copy_paths.join("\n")));
            },
        ));
        entries.push(Entry::Separator);

        // --- Remove -------------------------------------------------------------
        entries.push(
            action(
                "ctx-remove",
                "\u{232B}",
                if multi {
                    format!("Remove {count} photos from library (keep files)")
                } else {
                    "Remove from library (keep file)".to_string()
                },
                |this, _w, cx| this.delete_selected(false, cx),
            )
            .hint("\u{232B}"),
        );
        let rejected = self
            .catalog
            .assets
            .iter()
            .filter(|a| a.flag == Flag::Rejected && !a.missing)
            .count();
        if rejected > 0 {
            entries.push(action(
                "ctx-delete-rejected",
                "\u{2715}",
                format!(
                    "Delete {rejected} rejected photo{}\u{2026}",
                    if rejected == 1 { "" } else { "s" }
                ),
                |this, _w, cx| this.delete_rejected(cx),
            ));
        }
        entries
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
        let entries = self.context_entries();
        let height = entries.iter().map(Entry::height).sum::<f32>() + MENU_PADDING;

        // Keep the whole menu on screen.
        let viewport = window.viewport_size();
        let x = f32::from(menu.position.x)
            .min(f32::from(viewport.width) - MENU_WIDTH - 8.0)
            .max(8.0);
        let y = f32::from(menu.position.y)
            .min(f32::from(viewport.height) - height - 8.0)
            .max(8.0);

        let rows: Vec<AnyElement> = entries
            .into_iter()
            .map(|entry| match entry {
                Entry::Header(text) => div()
                    .h(px(ROW_HEIGHT))
                    .px_2()
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text_muted)
                    .child(text)
                    .into_any_element(),
                Entry::Separator => div()
                    .h(px(SEPARATOR_HEIGHT))
                    .flex()
                    .items_center()
                    .child(div().h(px(1.0)).w_full().bg(theme.border_subtle))
                    .into_any_element(),
                Entry::Heading(text) => div()
                    .h(px(HEADING_HEIGHT))
                    .px_2()
                    .flex()
                    .items_end()
                    .text_size(px(9.5))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.text_faint)
                    .child(text)
                    .into_any_element(),
                Entry::Action {
                    id,
                    icon,
                    label,
                    hint,
                    checked,
                    run,
                } => div()
                    .id(SharedString::from(id))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .h(px(ROW_HEIGHT))
                    .px_2()
                    .rounded_md()
                    .cursor_pointer()
                    .text_sm()
                    .text_color(theme.text_muted)
                    .hover(|el| el.bg(theme.accent_muted).text_color(theme.text))
                    .child(div().w(px(16.0)).text_center().child(icon))
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(label),
                    )
                    .when_some(hint, |el, hint| {
                        el.child(
                            div()
                                .text_size(px(10.0))
                                .text_color(theme.text_faint)
                                .child(hint),
                        )
                    })
                    .child(
                        div()
                            .w(px(12.0))
                            .text_xs()
                            .child(if checked { "\u{2713}" } else { "" }),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.context_menu = None;
                        run(this, window, cx);
                        cx.notify();
                    }))
                    .into_any_element(),
                Entry::Stars { current } => div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .h(px(ROW_HEIGHT))
                    .px_2()
                    .child(
                        div()
                            .w(px(16.0))
                            .text_center()
                            .text_color(theme.text_muted)
                            .child("\u{2605}"),
                    )
                    .children((1..=5u8).map(|star| {
                        let filled = current.is_some_and(|c| star <= c);
                        div()
                            .id(SharedString::from(format!("ctx-star-{star}")))
                            .cursor_pointer()
                            .text_size(px(16.0))
                            .text_color(if filled { theme.star } else { theme.text_faint })
                            .hover(|el| el.text_color(theme.star))
                            .child(if filled { "\u{2605}" } else { "\u{2606}" })
                            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                this.context_menu = None;
                                this.apply_cull(CullOp::Rating(star), cx);
                            }))
                    }))
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("ctx-star-clear")
                            .cursor_pointer()
                            .text_xs()
                            .text_color(theme.text_faint)
                            .hover(|el| el.text_color(theme.text))
                            .child("Clear")
                            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                                this.context_menu = None;
                                this.apply_cull(CullOp::Rating(0), cx);
                            })),
                    )
                    .into_any_element(),
                Entry::Swatches { current } => div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .h(px(ROW_HEIGHT))
                    .px_2()
                    .child(
                        div()
                            .w(px(16.0))
                            .text_center()
                            .text_color(theme.text_muted)
                            .child("\u{25CF}"),
                    )
                    .children(ColorLabel::ALL.into_iter().filter_map(|label| {
                        let color = theme.color_label(label)?;
                        let active = current == Some(label);
                        Some(
                            div()
                                .id(SharedString::from(format!("ctx-label-{}", label.label())))
                                .size(px(14.0))
                                .rounded_full()
                                .bg(color)
                                .border_2()
                                .border_color(if active { theme.text } else { color })
                                .cursor_pointer()
                                .hover(|el| el.border_color(theme.text_muted))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                    this.context_menu = None;
                                    this.apply_cull(CullOp::Label(label), cx);
                                })),
                        )
                    }))
                    .into_any_element(),
            })
            .collect();

        let close = cx.listener(|this, _: &MouseDownEvent, _w, cx| {
            this.context_menu = None;
            cx.notify();
        });

        div()
            .id("context-menu-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_mouse_down(MouseButton::Left, close)
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _: &MouseDownEvent, _w, cx| {
                    this.context_menu = None;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .id("context-menu")
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(MENU_WIDTH))
                    .occlude()
                    .p_1()
                    .flex()
                    .flex_col()
                    .rounded_xl()
                    .bg(theme.surface_elevated)
                    .border_1()
                    .border_color(theme.border)
                    .shadow_lg()
                    .children(rows),
            )
            .into_any_element()
    }
}
