//! The catalog system: named catalogs that each gather any number of
//! folders (and own their albums), a switcher menu in the title bar, and
//! the plumbing to create, switch, and delete them. The data model is
//! `rustyedit_core::CatalogDef`; storage is `rustyedit_io::registry`.

use rustyedit_ui::sp;
use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context, SharedString, Window};
use rustyedit_core::CatalogDef;
use rustyedit_io::registry;
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

    fn remove_folder_from_catalog(&mut self, folder: std::path::PathBuf, cx: &mut Context<Self>) {
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

    pub(crate) fn render_catalog_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let heading = |text: &'static str| {
            div()
                .pt_2()
                .pb_1()
                .px_2()
                .text_size(sp(10.0))
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
                .h(sp(28.0))
                .px_2()
                .rounded_md()
                .cursor_pointer()
                .text_sm()
                .when(active, |el| {
                    el.bg(theme.accent_muted).text_color(theme.text)
                })
                .when(!active, |el| {
                    el.text_color(theme.text_muted)
                        .hover(|el| el.bg(theme.surface).text_color(theme.text))
                })
        };

        let mut panel = div()
            .id("catalog-menu")
            .absolute()
            .top(px(TITLE_BAR_HEIGHT + 2.0))
            .left(sp(8.0))
            .w(px(MENU_WIDTH))
            .max_h(sp(520.0))
            .overflow_y_scroll()
            .occlude()
            .p_1()
            .flex()
            .flex_col()
            .rounded_xl()
            .bg(theme.surface_elevated)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .child(heading("CATALOGS"));

        for def in &self.catalog_list {
            let active = def.id == self.catalog_def.id;
            let id = def.id.clone();
            let delete_id = def.id.clone();
            let folders = def.folders.len();
            panel = panel.child(
                row(SharedString::from(format!("catalog-{}", def.id)), active)
                    .child(
                        div()
                            .w(sp(12.0))
                            .child(if active { "\u{2713}" } else { "" }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(sp(0.0))
                            .overflow_hidden()
                            .child(def.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(sp(10.0))
                            .text_color(theme.text_faint)
                            .child(format!(
                                "{folders} folder{}",
                                if folders == 1 { "" } else { "s" }
                            )),
                    )
                    .when(!active, |el| {
                        el.child(rustyedit_ui::glyph_button(
                            SharedString::from(format!("catalog-delete-{}", def.id)),
                            "\u{2715}",
                            "Delete catalog (photos are kept)",
                            theme.text_muted,
                            true,
                            &theme,
                            cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                this.delete_catalog(&delete_id, cx)
                            }),
                        ))
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.switch_catalog(&id, cx)
                    })),
            );
        }

        panel = match self.draft_for(TextTarget::NewCatalog) {
            Some(draft) => panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .h(sp(26.0))
                            .px_2()
                            .rounded_md()
                            .bg(theme.background)
                            .border_1()
                            .border_color(theme.accent)
                            .text_sm()
                            .text_color(theme.text)
                            .child(draft.to_string())
                            .child(div().w(sp(1.0)).h(sp(14.0)).ml(sp(1.0)).bg(theme.accent)),
                    )
                    .child(
                        div()
                            .text_size(sp(10.0))
                            .text_color(theme.text_faint)
                            .child(
                                "Name the catalog \u{00B7} Enter to create \u{00B7} Esc to cancel",
                            ),
                    ),
            ),
            None => panel.child(
                row("catalog-new".into(), false)
                    .child(div().w(sp(12.0)).child("+"))
                    .child("New catalog\u{2026}")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.begin_text_draft(TextTarget::NewCatalog, window, cx)
                    })),
            ),
        };

        panel = panel.child(heading("FOLDERS IN THIS CATALOG"));
        for folder in &self.catalog_def.folders {
            let remove = folder.clone();
            let label = folder
                .components()
                .rev()
                .take(2)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            panel = panel.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .h(sp(26.0))
                    .px_2()
                    .text_sm()
                    .text_color(theme.text_muted)
                    .child(div().text_size(sp(12.0)).child("\u{25A4}"))
                    .child(div().flex_1().min_w(sp(0.0)).overflow_hidden().child(label))
                    .child(rustyedit_ui::glyph_button(
                        SharedString::from(format!("catalog-folder-remove-{}", folder.display())),
                        "\u{2715}",
                        "Remove from catalog (files are not deleted)",
                        theme.text_muted,
                        true,
                        &theme,
                        cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            this.remove_folder_from_catalog(remove.clone(), cx)
                        }),
                    )),
            );
        }
        panel = panel.child(
            row("catalog-add-folder".into(), false)
                .child(div().w(sp(12.0)).child("+"))
                .child("Add folder\u{2026}")
                .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                    this.catalog_menu_open = false;
                    this.start_import(cx);
                })),
        );

        // A full-window backdrop closes the menu when clicking elsewhere.
        div()
            .id("catalog-menu-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_click(cx.listener(|this, _: &ClickEvent, _w: &mut Window, cx| {
                this.catalog_menu_open = false;
                this.text_draft = None;
                cx.notify();
            }))
            .child(panel)
            .into_any_element()
    }
}
