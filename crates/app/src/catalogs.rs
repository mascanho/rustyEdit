//! The catalog system: named catalogs that each gather any number of
//! folders (and own their albums), a switcher menu in the title bar, and
//! the plumbing to create, switch, and delete them. The data model is
//! `rustyedit_core::CatalogDef`; storage is `rustyedit_io::registry`.

use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context, SharedString, Window};
use rustyedit_core::CatalogDef;
use rustyedit_io::registry;
use rustyedit_ui::sp;
use rustyedit_ui::TITLE_BAR_HEIGHT;

use crate::workspace::{TextTarget, Workspace};

const MENU_WIDTH: f32 = 320.0;

impl Workspace {
    /// Loads the catalog list and the one that was open last, creating a
    /// first catalog (adopting any albums from before catalogs existed) on a
    /// fresh install.
    pub(crate) fn init_catalogs() -> (CatalogDef, Vec<CatalogDef>) {
        let mut list = registry::list();
        if list.is_empty() {
            if let Ok(mut first) = registry::create("My Catalog") {
                first.albums = rustyedit_io::load_albums();
                let _ = registry::save(&first);
                list.push(first);
            }
        }
        let active = registry::active_id()
            .and_then(|id| list.iter().find(|d| d.id == id).cloned())
            .or_else(|| list.first().cloned())
            .unwrap_or_else(|| CatalogDef::new("my-catalog", "My Catalog"));
        registry::set_active(&active.id);
        (active, list)
    }

    /// Persists the active catalog's definition (folders and albums) and
    /// refreshes its entry in the switcher list.
    pub(crate) fn save_catalog_def(&mut self) {
        if let Err(err) = registry::save(&self.catalog_def) {
            self.status_message = Some(format!("Couldn't save catalog: {err}").into());
        }
        match self
            .catalog_list
            .iter_mut()
            .find(|d| d.id == self.catalog_def.id)
        {
            Some(entry) => *entry = self.catalog_def.clone(),
            None => self.catalog_list.push(self.catalog_def.clone()),
        }
    }

    pub(crate) fn switch_catalog(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.is_importing || id == self.catalog_def.id {
            return;
        }
        let Some(next) = self.catalog_list.iter().find(|d| d.id == id).cloned() else {
            return;
        };
        self.save_outgoing_catalog();
        self.save_catalog_def();
        registry::set_active(&next.id);
        self.catalog_def = next;
        self.catalog_menu_open = false;
        self.open_catalog(None, cx);
    }

    pub(crate) fn create_catalog(&mut self, name: &str, cx: &mut Context<Self>) {
        match registry::create(name) {
            Ok(def) => {
                self.catalog_list.push(def.clone());
                self.catalog_list.sort_by_key(|d| d.name.to_lowercase());
                self.switch_catalog(&def.id, cx);
            }
            Err(err) => {
                self.status_message = Some(format!("Couldn't create catalog: {err}").into());
                cx.notify();
            }
        }
    }

    /// Deletes a catalog's definition. Photos, edits and thumbnails are not
    /// touched; the folders just stop being part of it.
    fn delete_catalog(&mut self, id: &str, cx: &mut Context<Self>) {
        if id == self.catalog_def.id {
            return;
        }
        registry::delete(id);
        self.catalog_list.retain(|d| d.id != id);
        cx.notify();
    }

    pub(crate) fn remove_folder_from_catalog(
        &mut self,
        folder: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        let label = folder
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| folder.display().to_string());
        self.catalog_def.remove_folder(&folder);
        self.save_catalog_def();
        self.post_scan_message = Some(
            format!("Removed \u{201C}{label}\u{201D} from this catalog; files were not deleted.")
                .into(),
        );
        self.open_catalog(None, cx);
    }

    /// Renames the catalog being edited in the switcher menu.
    pub(crate) fn rename_catalog(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(id) = self.catalog_renaming.take() else {
            return;
        };
        let name = name.trim();
        if name.is_empty() {
            cx.notify();
            return;
        }
        if id == self.catalog_def.id {
            self.catalog_def.name = name.to_string();
            self.save_catalog_def();
        } else if let Some(def) = self.catalog_list.iter_mut().find(|d| d.id == id) {
            def.name = name.to_string();
            let _ = registry::save(def);
        }
        self.catalog_list.sort_by_key(|d| d.name.to_lowercase());
        cx.notify();
    }

    /// The catalogs with the folders inside each, for the left column. The
    /// open catalog starts unfolded; clicking another one switches to it.
    pub(crate) fn render_catalog_tree(&self, cx: &mut Context<Self>) -> AnyElement {
        use rustyedit_ui::Icon;
        let theme = self.theme;
        let mini = |id: SharedString, icon: Icon, tip: &'static str, danger: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .justify_center()
                .flex_shrink_0()
                .size(sp(18.0))
                .cursor_pointer()
                .hover(|el| el.bg(theme.surface_elevated))
                .tooltip(rustyedit_ui::tip(tip, &theme))
                .child(rustyedit_ui::icon(
                    icon,
                    11.0,
                    if danger { theme.danger } else { theme.text_muted },
                ))
        };
        let field = |draft: &str| {
            div()
                .px_2()
                .py_0p5()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .h(sp(24.0))
                        .px_2()
                        .bg(theme.background)
                        .border_1()
                        .border_color(theme.accent)
                        .text_size(sp(12.0))
                        .text_color(theme.text)
                        .child(draft.to_string())
                        .child(div().w(sp(1.0)).h(sp(13.0)).ml(sp(1.0)).bg(theme.accent)),
                )
        };

        let mut rows: Vec<AnyElement> = Vec::new();
        for def in &self.catalog_list {
            let active = def.id == self.catalog_def.id;
            // Open by default for the active catalog, closed for the rest;
            // clicking the arrow flips that.
            let open = active != self.catalog_tree_toggled.contains(&def.id);
            let group = SharedString::from(format!("cat-tree-{}", def.id));

            if self.catalog_renaming.as_deref() == Some(def.id.as_str()) {
                if let Some(draft) = self.draft_for(TextTarget::RenameCatalog) {
                    rows.push(field(draft).into_any_element());
                    continue;
                }
            }

            let (toggle_id, switch_id, rename_id, delete_id) =
                (def.id.clone(), def.id.clone(), def.id.clone(), def.id.clone());
            let rename_seed = def.name.clone();
            let count = if active {
                self.catalog.assets.len().to_string()
            } else {
                String::new()
            };
            rows.push(
                div()
                    .id(SharedString::from(format!("cat-tree-row-{}", def.id)))
                    .group(group.clone())
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .h(sp(26.0))
                    .px_1()
                    .cursor_pointer()
                    .text_size(sp(12.0))
                    .when(active, |el| {
                        el.bg(theme.accent_muted.opacity(0.5)).text_color(theme.text)
                    })
                    .when(!active, |el| {
                        el.text_color(theme.text_muted)
                            .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                    })
                    .child(
                        div()
                            .id(SharedString::from(format!("cat-tree-arrow-{}", def.id)))
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(sp(16.0))
                            .child(rustyedit_ui::icon(
                                if open { Icon::ChevronDown } else { Icon::ChevronRight },
                                11.0,
                                theme.text_muted,
                            ))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                cx.stop_propagation();
                                if !this.catalog_tree_toggled.remove(&toggle_id) {
                                    this.catalog_tree_toggled.insert(toggle_id.clone());
                                }
                                cx.notify();
                            })),
                    )
                    .child(rustyedit_ui::icon(
                        Icon::Collections,
                        13.0,
                        if active { theme.accent } else { theme.text_muted },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w(sp(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .font_weight(if active {
                                gpui::FontWeight::SEMIBOLD
                            } else {
                                gpui::FontWeight::NORMAL
                            })
                            .child(def.name.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .flex_shrink_0()
                            .invisible()
                            .group_hover(group.clone(), |s| s.visible())
                            .child(
                                mini(
                                    SharedString::from(format!("cat-tree-rename-{}", def.id)),
                                    Icon::Edit,
                                    "Rename",
                                    false,
                                )
                                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                    cx.stop_propagation();
                                    this.catalog_renaming = Some(rename_id.clone());
                                    this.begin_text_draft(TextTarget::RenameCatalog, window, cx);
                                    if let Some((_, draft)) = this.text_draft.as_mut() {
                                        *draft = rename_seed.clone();
                                    }
                                })),
                            )
                            .when(!active, |el| {
                                el.child(
                                    mini(
                                        SharedString::from(format!("cat-tree-delete-{}", def.id)),
                                        Icon::Close,
                                        "Delete catalog (photos are kept)",
                                        true,
                                    )
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                        cx.stop_propagation();
                                        this.delete_catalog(&delete_id, cx)
                                    })),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(sp(10.0))
                            .text_color(theme.text_faint)
                            .child(count),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        // Clicking another catalog opens it; the open one
                        // just folds or unfolds.
                        if switch_id == this.catalog_def.id {
                            if !this.catalog_tree_toggled.remove(&switch_id) {
                                this.catalog_tree_toggled.insert(switch_id.clone());
                            }
                            cx.notify();
                        } else {
                            this.catalog_tree_toggled.remove(&switch_id);
                            this.switch_catalog(&switch_id, cx);
                        }
                    }))
                    .into_any_element(),
            );

            if !open {
                continue;
            }
            for folder in &def.folders {
                let name = folder
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| folder.display().to_string());
                let photos = active.then(|| {
                    self.catalog
                        .assets
                        .iter()
                        .filter(|a| a.path.starts_with(folder))
                        .count()
                });
                let fgroup = SharedString::from(format!("cat-folder-{}-{}", def.id, folder.display()));
                let (open_path, reveal_path, remove_path, switch_to) = (
                    folder.clone(),
                    folder.clone(),
                    folder.clone(),
                    def.id.clone(),
                );
                rows.push(
                    div()
                        .id(SharedString::from(format!("cat-folder-row-{}-{}", def.id, folder.display())))
                        .group(fgroup.clone())
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .h(sp(24.0))
                        .pl(sp(26.0))
                        .pr_1()
                        .cursor_pointer()
                        .text_size(sp(11.5))
                        .text_color(theme.text_muted)
                        .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                        .tooltip(rustyedit_ui::tip(folder.display().to_string(), &theme))
                        .child(rustyedit_ui::icon(Icon::Folder, 12.0, theme.text_muted))
                        .child(
                            div()
                                .flex_1()
                                .min_w(sp(0.0))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(name),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .flex_shrink_0()
                                .invisible()
                                .group_hover(fgroup.clone(), |s| s.visible())
                                .child(
                                    mini(
                                        SharedString::from(format!("cat-folder-reveal-{}-{}", def.id, folder.display())),
                                        Icon::Search,
                                        "Show in Finder",
                                        false,
                                    )
                                    .on_click(move |_: &ClickEvent, _w: &mut Window, cx: &mut gpui::App| {
                                        cx.stop_propagation();
                                        let _ = std::process::Command::new("open")
                                            .arg("-R")
                                            .arg(&reveal_path)
                                            .spawn();
                                    }),
                                )
                                .when(active, |el| {
                                    el.child(
                                        mini(
                                            SharedString::from(format!("cat-folder-remove-{}-{}", def.id, folder.display())),
                                            Icon::Close,
                                            "Remove from catalog (files stay)",
                                            true,
                                        )
                                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                            cx.stop_propagation();
                                            this.remove_folder_from_catalog(remove_path.clone(), cx)
                                        })),
                                    )
                                }),
                        )
                        .children(photos.map(|n| {
                            div()
                                .flex_shrink_0()
                                .text_size(sp(10.0))
                                .text_color(theme.text_faint)
                                .child(n.to_string())
                        }))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            if switch_to != this.catalog_def.id {
                                this.switch_catalog(&switch_to, cx);
                                return;
                            }
                            // In the open catalog: jump to the folder's
                            // first photo and show the folder in the tree.
                            this.reveal_folder_in_tree(&open_path, cx);
                            if let Some(ix) = this
                                .catalog
                                .assets
                                .iter()
                                .position(|a| a.path.starts_with(&open_path))
                            {
                                this.select_asset(ix, cx);
                            }
                        }))
                        .into_any_element(),
                );
            }
            if open && def.folders.is_empty() {
                rows.push(
                    div()
                        .pl(sp(26.0))
                        .h(sp(22.0))
                        .flex()
                        .items_center()
                        .text_size(sp(11.0))
                        .text_color(theme.text_faint)
                        .child("No folders")
                        .into_any_element(),
                );
            }
        }

        // New catalog and (for the open one) add folder.
        let action = |id: &'static str, icon: Icon, label: &'static str| {
            div()
                .id(id)
                .flex()
                .flex_row()
                .items_center()
                .gap_1p5()
                .h(sp(24.0))
                .px_2()
                .cursor_pointer()
                .text_size(sp(11.5))
                .text_color(theme.text_muted)
                .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                .child(rustyedit_ui::icon(icon, 12.0, theme.text_muted))
                .child(label)
        };
        let footer = match self.draft_for(TextTarget::NewCatalog) {
            Some(draft) => field(draft).into_any_element(),
            None => div()
                .flex()
                .flex_row()
                .child(
                    action("cat-tree-new", Icon::Plus, "Catalog").on_click(cx.listener(
                        |this, _: &ClickEvent, window, cx| {
                            this.begin_text_draft(TextTarget::NewCatalog, window, cx)
                        },
                    )),
                )
                .child(
                    action("cat-tree-add-folder", Icon::Folder, "Folder").on_click(cx.listener(
                        |this, _: &ClickEvent, _w, cx| this.start_import(cx),
                    )),
                )
                .into_any_element(),
        };

        self.disclosure(
            "catalogs",
            "Catalogs",
            vec![
                div()
                    .flex()
                    .flex_col()
                    .children(rows)
                    .child(footer)
                    .into_any_element(),
            ],
            cx,
        )
    }

    pub(crate) fn render_catalog_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        use rustyedit_ui::Icon;
        let theme = self.theme;
        let heading = |text: &'static str| {
            div()
                .pt_2()
                .pb_0p5()
                .px_2()
                .text_size(sp(9.5))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(theme.text_faint)
                .child(text)
        };
        let row = |id: SharedString, active: bool| {
            div()
                .id(id)
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .h(sp(26.0))
                .px_2()
                .cursor_pointer()
                .text_size(sp(12.0))
                .when(active, |el| {
                    el.bg(theme.accent_muted).text_color(theme.text)
                })
                .when(!active, |el| {
                    el.text_color(theme.text_muted)
                        .hover(|el| el.bg(theme.surface).text_color(theme.text))
                })
        };
        // A hover action: an icon with a tooltip.
        let mini = |id: SharedString, icon: Icon, tip: &'static str, danger: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .justify_center()
                .flex_shrink_0()
                .size(sp(20.0))
                .cursor_pointer()
                .hover(|el| el.bg(theme.surface_elevated))
                .tooltip(rustyedit_ui::tip(tip, &theme))
                .child(rustyedit_ui::icon(
                    icon,
                    12.0,
                    if danger {
                        theme.danger
                    } else {
                        theme.text_muted
                    },
                ))
        };
        let text_field = |draft: &str| {
            div().px_2().py_1().child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h(sp(26.0))
                    .px_2()
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.accent)
                    .text_sm()
                    .text_color(theme.text)
                    .child(draft.to_string())
                    .child(div().w(sp(1.0)).h(sp(14.0)).ml(sp(1.0)).bg(theme.accent)),
            )
        };

        let folder_counts: Vec<usize> = self
            .catalog_def
            .folders
            .iter()
            .map(|folder| {
                self.catalog
                    .assets
                    .iter()
                    .filter(|a| a.path.starts_with(folder))
                    .count()
            })
            .collect();

        let mut panel = div()
            .id("catalog-menu")
            .absolute()
            .top(px(TITLE_BAR_HEIGHT + 2.0))
            .left(sp(8.0))
            .w(px(MENU_WIDTH))
            .max_h(sp(520.0))
            .overflow_y_scroll()
            .occlude()
            .py_1()
            .flex()
            .flex_col()
            .rounded_lg()
            .bg(theme.surface_elevated)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .child(heading("CATALOGS"));

        for def in &self.catalog_list {
            let active = def.id == self.catalog_def.id;
            let id = def.id.clone();
            let group = SharedString::from(format!("catalog-row-{}", def.id));

            if self.catalog_renaming.as_deref() == Some(def.id.as_str()) {
                if let Some(draft) = self.draft_for(TextTarget::RenameCatalog) {
                    panel = panel.child(text_field(draft));
                    continue;
                }
            }

            let delete_id = def.id.clone();
            let rename_id = def.id.clone();
            let rename_seed = def.name.clone();
            panel = panel.child(
                row(SharedString::from(format!("catalog-{}", def.id)), active)
                    .group(group.clone())
                    .child(div().w(sp(14.0)).flex().justify_center().children(
                        active.then(|| rustyedit_ui::icon(Icon::Check, 12.0, theme.accent)),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w(sp(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(def.name.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .flex_shrink_0()
                            .invisible()
                            .group_hover(group.clone(), |s| s.visible())
                            .child(
                                mini(
                                    SharedString::from(format!("catalog-rename-{}", def.id)),
                                    Icon::Edit,
                                    "Rename",
                                    false,
                                )
                                .on_click(cx.listener(
                                    move |this, _: &ClickEvent, window, cx| {
                                        cx.stop_propagation();
                                        this.catalog_renaming = Some(rename_id.clone());
                                        this.begin_text_draft(
                                            TextTarget::RenameCatalog,
                                            window,
                                            cx,
                                        );
                                        if let Some((_, draft)) = this.text_draft.as_mut() {
                                            *draft = rename_seed.clone();
                                        }
                                    },
                                )),
                            )
                            .when(!active, |el| {
                                el.child(
                                    mini(
                                        SharedString::from(format!("catalog-delete-{}", def.id)),
                                        Icon::Close,
                                        "Delete catalog (photos are kept)",
                                        true,
                                    )
                                    .on_click(cx.listener(
                                        move |this, _: &ClickEvent, _w, cx| {
                                            cx.stop_propagation();
                                            this.delete_catalog(&delete_id, cx)
                                        },
                                    )),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .text_size(sp(10.0))
                            .text_color(theme.text_faint)
                            .child(rustyedit_ui::icon(Icon::Folder, 11.0, theme.text_faint))
                            .child(def.folders.len().to_string()),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.switch_catalog(&id, cx)
                    })),
            );
        }

        panel = match self.draft_for(TextTarget::NewCatalog) {
            Some(draft) => panel.child(text_field(draft)),
            None => panel.child(
                row("catalog-new".into(), false)
                    .child(
                        div()
                            .w(sp(14.0))
                            .flex()
                            .justify_center()
                            .child(rustyedit_ui::icon(Icon::Plus, 12.0, theme.text_muted)),
                    )
                    .child("New catalog")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.begin_text_draft(TextTarget::NewCatalog, window, cx)
                    })),
            ),
        };

        panel = panel.child(heading("FOLDERS"));
        for (folder, count) in self.catalog_def.folders.iter().zip(&folder_counts) {
            let remove = folder.clone();
            let reveal = folder.clone();
            let name = folder
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| folder.display().to_string());
            let group = SharedString::from(format!("catalog-folder-{}", folder.display()));
            panel = panel.child(
                row(
                    SharedString::from(format!("catalog-folder-row-{}", folder.display())),
                    false,
                )
                .group(group.clone())
                .cursor_default()
                .tooltip(rustyedit_ui::tip(folder.display().to_string(), &theme))
                .child(
                    div()
                        .w(sp(14.0))
                        .flex()
                        .justify_center()
                        .child(rustyedit_ui::icon(Icon::Folder, 13.0, theme.text_muted)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(sp(0.0))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(name),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .flex_shrink_0()
                        .invisible()
                        .group_hover(group.clone(), |s| s.visible())
                        .child(
                            mini(
                                SharedString::from(format!(
                                    "catalog-folder-reveal-{}",
                                    folder.display()
                                )),
                                Icon::Search,
                                "Show in Finder",
                                false,
                            )
                            .on_click(
                                move |_: &ClickEvent, _w: &mut Window, _cx: &mut gpui::App| {
                                    let _ = std::process::Command::new("open")
                                        .arg("-R")
                                        .arg(&reveal)
                                        .spawn();
                                },
                            ),
                        )
                        .child(
                            mini(
                                SharedString::from(format!(
                                    "catalog-folder-remove-{}",
                                    folder.display()
                                )),
                                Icon::Close,
                                "Remove from catalog (files stay)",
                                true,
                            )
                            .on_click(cx.listener(
                                move |this, _: &ClickEvent, _w, cx| {
                                    this.remove_folder_from_catalog(remove.clone(), cx)
                                },
                            )),
                        ),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(sp(10.0))
                        .text_color(theme.text_faint)
                        .child(count.to_string()),
                ),
            );
        }

        // Add and sync, side by side.
        panel = panel.child(
            div()
                .flex()
                .flex_row()
                .gap_1()
                .px_2()
                .pt_1p5()
                .child(
                    row("catalog-add-folder".into(), false)
                        .flex_1()
                        .justify_center()
                        .bg(theme.background)
                        .child(rustyedit_ui::icon(Icon::Plus, 12.0, theme.text_muted))
                        .child("Add folder")
                        .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                            this.catalog_menu_open = false;
                            this.start_import(cx);
                        })),
                )
                .child(
                    row("catalog-sync".into(), false)
                        .flex_1()
                        .justify_center()
                        .bg(theme.background)
                        .tooltip(rustyedit_ui::tip("Pick up new and missing files", &theme))
                        .child(rustyedit_ui::icon(Icon::Reset, 12.0, theme.text_muted))
                        .child("Sync")
                        .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                            this.catalog_menu_open = false;
                            this.sync_folders(cx);
                        })),
                ),
        );

        div()
            .id("catalog-menu-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_click(cx.listener(|this, _: &ClickEvent, _w: &mut Window, cx| {
                this.catalog_menu_open = false;
                this.catalog_renaming = None;
                this.text_draft = None;
                cx.notify();
            }))
            .child(panel)
            .into_any_element()
    }
}
