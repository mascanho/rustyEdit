//! File lifecycle actions for photos: delete (catalog or Trash), rename,
//! real file relocation for missing originals, non-destructive rotation,
//! "Reveal in Finder", folder synchronization and the missing-files panel.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicUsize,
};

use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context, SharedString, Window};
use rustyedit_core::AssetState;

use crate::{
    actions::{
        DeleteFromCatalog, DeleteFromDisk, RenamePhoto, RestoreRemoved, RevealInFinder, RotateLeft,
        RotateRight, SyncFolders,
    },
    workspace::{Mode, TextTarget, Workspace},
};

impl Workspace {
    fn lifecycle_allowed(&self) -> bool {
        !self.is_importing && self.mode != Mode::Export
    }

    // --- Delete -------------------------------------------------------------

    pub fn delete_from_catalog(
        &mut self,
        _: &DeleteFromCatalog,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_selected(false, cx);
    }

    pub fn delete_from_disk(
        &mut self,
        _: &DeleteFromDisk,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_selected(true, cx);
    }

    pub(crate) fn delete_selected(&mut self, from_disk: bool, cx: &mut Context<Self>) {
        if !self.lifecycle_allowed() {
            return;
        }
        let paths: HashSet<PathBuf> = self
            .target_indices()
            .into_iter()
            .filter_map(|ix| self.catalog.assets.get(ix))
            .filter(|asset| !asset.missing)
            .map(|asset| asset.path.clone())
            .collect();
        if paths.is_empty() {
            return;
        }

        if !from_disk {
            for path in &paths {
                self.catalog_def.exclude(path);
            }
            self.save_catalog_def();
            self.finish_removal(paths, cx);
            return;
        }

        let count = paths.len();
        let cache = self.thumbnail_cache.clone();
        cx.spawn(async move |this, cx| {
            let confirmed = cx
                .background_executor()
                .spawn(async move {
                    rfd::MessageDialog::new()
                        .set_title("Move to Trash?")
                        .set_description(format!(
                            "Move {count} photo{} to the Trash? The originals will be removed from disk.",
                            if count == 1 { "" } else { "s" }
                        ))
                        .set_buttons(rfd::MessageButtons::YesNo)
                        .show()
                        == rfd::MessageDialogResult::Yes
                })
                .await;
            if !confirmed {
                return;
            }

            let to_move = paths.clone();
            let moved = cx
                .background_executor()
                .spawn(async move {
                    let mut moved = HashSet::new();
                    let mut failed = Vec::new();
                    for path in to_move {
                        if let Some(cache) = &cache {
                            cache.forget(&path);
                        }
                        match rustyedit_io::move_to_trash(&path) {
                            Ok(_) => {
                                moved.insert(path);
                            }
                            Err(err) => failed.push((path, err.to_string())),
                        }
                    }
                    (moved, failed)
                })
                .await;
            this.update(cx, |this, cx| {
                if !moved.1.is_empty() {
                    this.status_message =
                        Some(format!("Couldn't move {} photo(s) to Trash.", moved.1.len()).into());
                }
                if !moved.0.is_empty() {
                    this.finish_removal(moved.0, cx);
                    this.status_message = Some(
                        format!(
                            "Moved {count} photo{} to the Trash.",
                            if count == 1 { "" } else { "s" }
                        )
                        .into(),
                    );
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Removes `paths` from the in-memory catalog and from any albums, then
    /// resets all id-keyed caches because asset ids are positions.
    fn finish_removal(&mut self, paths: HashSet<PathBuf>, cx: &mut Context<Self>) {
        if self.remove_paths_from_albums(&paths) {
            self.save_catalog_def();
        }
        let removed = self.catalog.remove_assets(&paths);
        if removed == 0 {
            return;
        }
        self.clear_asset_caches();
        self.mark_catalog_dirty(cx);
        self.selection.clear();
        self.selection_anchor = None;
        if let Some(&first) = self.catalog.visible_indices().first() {
            self.catalog.select(first);
        }
        self.on_selection_changed(cx);
    }

    /// Removes one photo (used by the missing-files list, where the asset is
    /// not selectable through the normal grid selection).
    pub(crate) fn remove_asset_from_catalog(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(path) = self.catalog.assets.get(index).map(|a| a.path.clone()) else {
            return;
        };
        self.catalog_def.exclude(&path);
        self.save_catalog_def();
        let paths: HashSet<PathBuf> = [path].into_iter().collect();
        self.finish_removal(paths, cx);
    }

    fn remove_paths_from_albums(&mut self, paths: &HashSet<PathBuf>) -> bool {
        let mut changed = false;
        for album in &mut self.catalog_def.albums.items {
            let before = album.photos.len();
            album.photos.retain(|photo| !paths.contains(photo));
            changed |= album.photos.len() != before;
        }
        changed
    }

    fn replace_path_in_albums(&mut self, old: &Path, new: &Path) -> bool {
        let mut changed = false;
        for album in &mut self.catalog_def.albums.items {
            for photo in &mut album.photos {
                if photo == old {
                    *photo = new.to_path_buf();
                    changed = true;
                }
            }
        }
        changed
    }

    /// Drops every cache keyed by asset id. Asset ids are catalog positions,
    /// so they all shift when photos are removed.
    fn clear_asset_caches(&mut self) {
        self.metadata_cache.clear();
        self.metadata_loading.clear();
        self.histogram_cache.clear();
        self.histogram_loading = None;
        self.develop_histogram = None;
        self.loupe_paths.clear();
        self.loupe_loading.clear();
        self.loupe_full_paths.clear();
        self.loupe_full_loading.clear();
        self.duplicate_groups = None;
        self.duplicates_loading = false;
        self.reset_develop_preview();
    }

    // --- Rename -------------------------------------------------------------

    pub fn rename_photo(&mut self, _: &RenamePhoto, window: &mut Window, cx: &mut Context<Self>) {
        if !self.lifecycle_allowed() {
            return;
        }
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        if asset.missing {
            self.status_message = Some("Locate this file before renaming it.".into());
            cx.notify();
            return;
        }
        let stem = asset
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.rename_target = Some(self.catalog.selected);
        self.text_draft = Some((TextTarget::Rename, stem));
        window.focus(&self.focus_handle);
        cx.notify();
    }

    /// Applies the inline rename draft to the original file on disk.
    pub(crate) fn commit_rename(&mut self, text: String, cx: &mut Context<Self>) {
        let index = self
            .rename_target
            .take()
            .or(Some(self.catalog.selected))
            .unwrap_or(self.catalog.selected);
        let Some(asset) = self.catalog.assets.get(index) else {
            return;
        };
        if asset.missing {
            return;
        }
        let old_path = asset.path.clone();
        let Some(parent) = old_path.parent().map(Path::to_path_buf) else {
            return;
        };
        let name = text.trim();
        if name.is_empty() {
            self.status_message = Some("Rename cancelled: the name was empty.".into());
            cx.notify();
            return;
        }
        if name.contains('/') || name.contains('\\') || name.contains('\0') {
            self.status_message = Some("A file name can't contain a slash.".into());
            cx.notify();
            return;
        }
        let new_name = if Path::new(name).extension().is_some() {
            name.to_string()
        } else {
            match old_path.extension() {
                Some(ext) => format!("{name}.{}", ext.to_string_lossy()),
                None => name.to_string(),
            }
        };
        let new_path = parent.join(&new_name);
        if new_path == old_path {
            return;
        }
        if new_path.exists() {
            self.status_message = Some(format!("\"{new_name}\" already exists.").into());
            cx.notify();
            return;
        }
        if let Err(err) = std::fs::rename(&old_path, &new_path) {
            self.status_message = Some(format!("Couldn't rename: {err}").into());
            cx.notify();
            return;
        }
        if let Some(cache) = &self.thumbnail_cache {
            cache.forget(&old_path);
        }
        if self.replace_path_in_albums(&old_path, &new_path) {
            self.save_catalog_def();
        }
        if let Some(asset) = self.catalog.assets.get_mut(index) {
            asset.path = new_path.clone();
            asset.thumbnail_path = None;
            asset.width = 0;
            asset.height = 0;
        }
        self.status_message = Some(format!("Renamed to {new_name}").into());
        self.mark_catalog_dirty(cx);
        self.regenerate_thumbnails(&[index], cx);
        self.on_selection_changed(cx);
    }

    /// Small floating field shown while the rename draft is being typed.
    pub(crate) fn render_rename_chip(&self, _cx: &mut Context<Self>) -> Option<AnyElement> {
        let draft = self.draft_for(TextTarget::Rename)?;
        let theme = self.theme;
        Some(
            div()
                .id("rename-chip")
                .absolute()
                .top(px(12.0))
                .left(px(12.0))
                .occlude()
                .flex()
                .flex_col()
                .gap_0p5()
                .w(px(320.0))
                .p_2()
                .rounded_lg()
                .bg(theme.surface_elevated)
                .border_1()
                .border_color(theme.accent)
                .shadow_lg()
                .text_sm()
                .text_color(theme.text)
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme.text_faint)
                        .child("RENAME PHOTO"),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .h(px(24.0))
                        .child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(draft.to_string()),
                        )
                        .child(div().w(px(1.0)).h(px(14.0)).ml(px(1.0)).bg(theme.accent)),
                )
                .child(
                    div()
                        .text_size(px(9.5))
                        .text_color(theme.text_faint)
                        .child("Enter to rename \u{00B7} Esc to cancel"),
                )
                .into_any_element(),
        )
    }

    // --- Locate missing files ----------------------------------------------

    pub(crate) fn locate_missing_asset(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(asset) = self.catalog.assets.get(index) else {
            return;
        };
        if !asset.missing {
            return;
        }
        let suggested = asset
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        cx.spawn(async move |this, cx| {
            let picked = cx
                .background_executor()
                .spawn(async move {
                    rfd::FileDialog::new()
                        .set_title("Locate Missing Photo")
                        .set_file_name(&suggested)
                        .pick_file()
                })
                .await;
            let Some(new_path) = picked else {
                return;
            };
            this.update(cx, |this, cx| this.relink_asset(index, new_path, cx))
                .ok();
        })
        .detach();
    }

    fn relink_asset(&mut self, index: usize, new_path: PathBuf, cx: &mut Context<Self>) {
        let Some(asset) = self.catalog.assets.get(index) else {
            return;
        };
        if !new_path.is_file() {
            self.status_message = Some("That file doesn't exist.".into());
            cx.notify();
            return;
        }
        let old_path = asset.path.clone();
        if old_path == new_path {
            return;
        }
        if let Some(cache) = &self.thumbnail_cache {
            cache.forget(&old_path);
        }
        if self.replace_path_in_albums(&old_path, &new_path) {
            self.save_catalog_def();
        }
        // A relinked original may live outside the catalog's folders. Adopt
        // its folder so the photo, and its saved edits, survive a rescan.
        let mut adopted_folder = false;
        if !self
            .catalog_def
            .folders
            .iter()
            .any(|folder| new_path.starts_with(folder))
        {
            if let Some(parent) = new_path.parent() {
                if self.catalog_def.add_folder(parent) {
                    self.catalog.folders = self.catalog_def.folders.clone();
                    self.save_catalog_def();
                    adopted_folder = true;
                }
            }
        }
        if let Some(asset) = self.catalog.assets.get_mut(index) {
            asset.path = new_path.clone();
            asset.missing = false;
            asset.thumbnail_path = None;
            asset.width = 0;
            asset.height = 0;
            asset.meta = Default::default();
            asset.content_hash = 0;
            asset.phash = 0;
        }
        self.catalog.refresh();
        self.catalog.select(index);
        self.status_message = Some(if adopted_folder {
            format!(
                "Relocated {} and added its folder to this catalog.",
                new_path.display()
            )
            .into()
        } else {
            format!("Relocated {}", new_path.display()).into()
        });
        self.mark_catalog_dirty(cx);
        self.regenerate_thumbnails(&[index], cx);
        self.on_selection_changed(cx);
    }

    /// The missing-files section of the Navigator. Each row can relink the
    /// original or drop the photo from the catalog.
    pub(crate) fn missing_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let missing = self.catalog.missing_indices();
        if missing.is_empty() {
            return Vec::new();
        }
        let theme = self.theme;
        let mut rows: Vec<AnyElement> = Vec::new();
        rows.push(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .pt_3()
                .pb_1()
                .child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.reject)
                        .child(format!("MISSING FILES ({})", missing.len())),
                )
                .into_any_element(),
        );
        for index in missing {
            let Some(asset) = self.catalog.assets.get(index) else {
                continue;
            };
            let name = asset.file_name();
            let folder = asset
                .path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            rows.push(
                div()
                    .id(("missing-row", index))
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .py_1()
                    .px_1()
                    .rounded_md()
                    .hover(|el| el.bg(theme.surface))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .flex_1()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_xs()
                                    .text_color(theme.text)
                                    .child(name),
                            )
                            .child(rustyedit_ui::glyph_button(
                                SharedString::from(format!("missing-locate-{index}")),
                                "\u{2315}",
                                "Locate this file",
                                theme.accent,
                                true,
                                &theme,
                                cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                    this.locate_missing_asset(index, cx)
                                }),
                            ))
                            .child(rustyedit_ui::glyph_button(
                                SharedString::from(format!("missing-remove-{index}")),
                                "\u{2715}",
                                "Remove from catalog",
                                theme.text_muted,
                                true,
                                &theme,
                                cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                    this.remove_asset_from_catalog(index, cx)
                                }),
                            )),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(9.0))
                            .text_color(theme.text_faint)
                            .child(folder),
                    )
                    .into_any_element(),
            );
        }
        rows.push(
            div()
                .pt_1()
                .text_size(px(9.0))
                .text_color(theme.text_faint)
                .child("Use the arrow to locate a moved file, or remove it from this catalog.")
                .into_any_element(),
        );
        vec![div()
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(theme.border_subtle)
            .children(rows)
            .into_any_element()]
    }

    // --- Rotate -------------------------------------------------------------

    pub fn rotate_left(&mut self, _: &RotateLeft, _w: &mut Window, cx: &mut Context<Self>) {
        self.rotate_selected(-90.0, cx);
    }

    pub fn rotate_right(&mut self, _: &RotateRight, _w: &mut Window, cx: &mut Context<Self>) {
        self.rotate_selected(90.0, cx);
    }

    pub(crate) fn rotate_selected(&mut self, delta: f32, cx: &mut Context<Self>) {
        if !self.lifecycle_allowed() || self.catalog.selected_asset().is_none() {
            return;
        }
        let targets = self.target_indices();
        let mut changes: Vec<(usize, AssetState, AssetState)> = Vec::new();
        for &index in &targets {
            let Some(asset) = self.catalog.assets.get(index) else {
                continue;
            };
            if asset.missing {
                continue;
            }
            let before = asset.state();
            if let Some(asset) = self.catalog.assets.get_mut(index) {
                let old = asset.develop.rotation_degrees;
                let new = (old + delta).rem_euclid(360.0);
                asset.develop.rotation_degrees = new;
                asset.record_develop_change(
                    "Rotate",
                    format!("{old:.0}\u{00B0}"),
                    format!("{new:.0}\u{00B0}"),
                );
            }
            changes.push((index, before, self.catalog.assets[index].state()));
        }
        if changes.is_empty() {
            return;
        }
        self.push_undo(
            if targets.len() > 1 {
                format!("rotate {} photos", targets.len())
            } else {
                "rotate".to_string()
            },
            changes,
            false,
        );
        self.mark_catalog_dirty(cx);
        self.refresh_asset_thumbnails(&targets, cx);
        self.on_selection_changed(cx);
    }

    /// Rebuilds the grid thumbnail (and drops loupe/histogram caches) after a
    /// path or rotation change. Also used by undo, so rotating back restores
    /// the original orientation.
    pub(crate) fn refresh_asset_thumbnails(&mut self, indices: &[usize], cx: &mut Context<Self>) {
        self.regenerate_thumbnails(indices, cx);
    }

    fn regenerate_thumbnails(&mut self, indices: &[usize], cx: &mut Context<Self>) {
        let Some(cache) = self.thumbnail_cache.clone() else {
            return;
        };
        let mut jobs = Vec::new();
        for &index in indices {
            let Some(asset) = self.catalog.assets.get(index) else {
                continue;
            };
            if asset.missing {
                continue;
            }
            let mut asset = asset.clone();
            // Force a fresh decode so dimensions and hashes follow the new
            // file name or rotation.
            asset.width = 0;
            asset.height = 0;
            jobs.push((index, asset));
        }
        if jobs.is_empty() {
            return;
        }
        let job = cx.background_executor().spawn(async move {
            let counter = AtomicUsize::new(0);
            let mut out = Vec::new();
            for (index, asset) in jobs {
                let infos = rustyedit_io::generate_thumbnails(
                    &cache,
                    std::slice::from_ref(&asset),
                    &counter,
                );
                if let Some(info) = infos.into_iter().next() {
                    out.push((index, info));
                }
            }
            out
        });
        cx.spawn(async move |this, cx| {
            let results = job.await;
            this.update(cx, |this, cx| {
                for (index, info) in results {
                    let Some(asset) = this.catalog.assets.get_mut(index) else {
                        continue;
                    };
                    if asset.id != info.id {
                        continue;
                    }
                    asset.thumbnail_path = Some(info.thumbnail);
                    asset.width = info.width;
                    asset.height = info.height;
                    asset.orientation = info.orientation;
                    asset.meta = info.meta;
                    asset.content_hash = info.content_hash;
                    asset.phash = info.phash;
                    let id = asset.id;
                    this.histogram_cache.remove(&id);
                    this.histogram_loading = None;
                    this.loupe_paths.remove(&id);
                    this.loupe_loading.remove(&id);
                    this.loupe_full_paths.remove(&id);
                    this.loupe_full_loading.remove(&id);
                    if this.catalog.selected == index {
                        this.request_loupe(cx);
                        this.request_histogram(cx);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // --- Reveal -------------------------------------------------------------

    pub fn reveal_in_finder(
        &mut self,
        _: &RevealInFinder,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        let mut target = asset.path.clone();
        if !target.exists() {
            if let Some(parent) = target.parent() {
                target = parent.to_path_buf();
            }
        }
        match Command::new("open").arg("-R").arg(&target).spawn() {
            Ok(_) => {
                self.status_message = Some("Revealed in Finder.".into());
            }
            Err(err) => {
                self.status_message = Some(format!("Couldn't open Finder: {err}").into());
            }
        }
        cx.notify();
    }

    /// The small action row shown in the Inspector's Info section.
    pub(crate) fn file_actions_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .pt_1()
            .child(rustyedit_ui::glyph_button(
                "file-reveal",
                "\u{2197}",
                "Reveal in Finder",
                theme.text_muted,
                true,
                &theme,
                cx.listener(|this, _: &ClickEvent, w, cx| {
                    this.reveal_in_finder(&RevealInFinder, w, cx)
                }),
            ))
            .child(rustyedit_ui::glyph_button(
                "file-rename",
                "\u{270E}",
                "Rename photo (F2)",
                theme.text_muted,
                true,
                &theme,
                cx.listener(|this, _: &ClickEvent, w, cx| this.rename_photo(&RenamePhoto, w, cx)),
            ))
            .child(rustyedit_ui::glyph_button(
                "file-rotate-left",
                "\u{27F2}",
                "Rotate left (\u{2318}[)",
                theme.text_muted,
                true,
                &theme,
                cx.listener(|this, _: &ClickEvent, w, cx| this.rotate_left(&RotateLeft, w, cx)),
            ))
            .child(rustyedit_ui::glyph_button(
                "file-rotate-right",
                "\u{27F3}",
                "Rotate right (\u{2318}])",
                theme.text_muted,
                true,
                &theme,
                cx.listener(|this, _: &ClickEvent, w, cx| this.rotate_right(&RotateRight, w, cx)),
            ))
            .child(rustyedit_ui::glyph_button(
                "file-delete",
                "\u{232B}",
                "Remove from catalog (\u{232B})",
                theme.text_muted,
                true,
                &theme,
                cx.listener(|this, _: &ClickEvent, w, cx| {
                    this.delete_from_catalog(&DeleteFromCatalog, w, cx)
                }),
            ))
            .into_any_element()
    }

    // --- Synchronize --------------------------------------------------------

    pub fn sync_folders_action(
        &mut self,
        _: &SyncFolders,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_folders(cx);
    }

    pub(crate) fn sync_folders(&mut self, cx: &mut Context<Self>) {
        if self.is_importing {
            return;
        }
        self.pending_sync_report = true;
        self.open_catalog(None, cx);
    }

    pub fn restore_removed_photos(
        &mut self,
        _: &RestoreRemoved,
        _w: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.restore_excluded(cx);
    }

    pub(crate) fn restore_excluded(&mut self, cx: &mut Context<Self>) {
        if self.catalog_def.excluded.is_empty() {
            return;
        }
        self.catalog_def.restore_excluded();
        self.save_catalog_def();
        self.sync_folders(cx);
    }
}
