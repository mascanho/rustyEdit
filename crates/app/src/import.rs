use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};

use gpui::Context;
use rustyedit_core::Catalog;
use rustyedit_io::ThumbnailCache;

use crate::workspace::Workspace;

/// How many thumbnails are made before the library is updated with them.
const THUMBNAIL_BATCH: usize = 24;

impl Workspace {
    /// Opens a native folder picker, then imports the chosen folder.
    pub(crate) fn start_import(&mut self, cx: &mut Context<Self>) {
        if self.is_importing {
            return;
        }
        self.is_importing = true;
        self.status_message = Some("Choose a folder…".into());
        cx.notify();

        cx.spawn(async move |this, cx| {
            let folder = cx
                .background_executor()
                // The async dialog opens on a thread of its own. The blocking
                // one fails at once on Windows, whose background threads are
                // already set up for COM in a mode the dialog can't use.
                .spawn(async {
                    rfd::AsyncFileDialog::new()
                        .pick_folder()
                        .await
                        .map(|handle| handle.path().to_path_buf())
                })
                .await;

            this.update(cx, |this, cx| {
                this.is_importing = false;
                match folder {
                    // Choose what to bring in, then import.
                    Some(folder) => this.open_import_dialog(folder, cx),
                    None => {
                        this.status_message = Some("Import cancelled.".into());
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    /// Imports `folder` except for the photos in `skipped` (the ones the
    /// import dialog's filter left out). They're remembered as not
    /// belonging to the library, so a rescan doesn't bring them in; an
    /// import of the folder with other choices does.
    pub(crate) fn import_folder_without(
        &mut self,
        folder: PathBuf,
        skipped: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if self.is_importing {
            return;
        }
        self.catalog_def.add_folder(&folder);
        // Earlier choices for this folder are replaced by this one.
        self.catalog_def.restore_excluded_in(&folder);
        for path in &skipped {
            self.catalog_def.exclude(path);
        }
        self.save_catalog_def();
        self.import_append = true;
        self.open_catalog(None, cx);
    }

    /// Adds `folder` to the active catalog and (re)loads it. This is the
    /// "Import" flow: the one-time cost that makes everything after it
    /// (culling) fast, since thumbnails are decoded and cached on disk. If
    /// `select` names a photo, it's selected once loading completes.
    pub(crate) fn import_folder(
        &mut self,
        folder: PathBuf,
        select: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if self.is_importing {
            return;
        }
        // Already-covered folders still get rescanned, which picks up any
        // new files; everything already in the library stays as it is.
        // Importing a folder again also brings back photos in it that were
        // removed from the library, rather than skipping them as rescans do.
        let added = self.catalog_def.add_folder(&folder);
        let restored = select.is_none() && self.catalog_def.restore_excluded_in(&folder);
        if added || restored {
            self.save_catalog_def();
        }
        self.import_append = select.is_none();
        self.open_catalog(select, cx);
    }

    /// Scans every folder of the active catalog and generates thumbnails
    /// entirely off the main thread, only touching `self` to report progress
    /// and to install the finished catalog.
    pub(crate) fn open_catalog(&mut self, select: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.is_importing {
            return;
        }
        // Whatever thumbnails were still being made for the library that's
        // being replaced aren't wanted any more.
        self.import_cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        self.import_cancel = Default::default();
        self.import_epoch += 1;
        let epoch = self.import_epoch;
        let cancel = self.import_cancel.clone();
        self.import_progress = None;
        let folders = self.catalog_def.folders.clone();
        if folders.is_empty() {
            self.install_catalog(Catalog::default(), self.thumbnail_cache.clone(), None, cx);
            self.pending_sync_report = false;
            self.post_scan_message = None;
            self.status_message = Some("This catalog is empty \u{2014} import a folder.".into());
            return;
        }
        // Write any pending edits first, and keep the library's current state
        // to carry over: a reload (an import, a sync) only ever adds to it.
        self.save_outgoing_catalog();
        let excluded: HashSet<PathBuf> = self.catalog_def.excluded.iter().cloned().collect();
        let before_paths: HashSet<PathBuf> =
            self.catalog.assets.iter().map(|a| a.path.clone()).collect();
        // Keyed by file and copy name: a virtual copy is a separate entry
        // for the same file.
        let current: std::collections::HashMap<(PathBuf, Option<String>), rustyedit_core::Asset> =
            self.catalog
                .assets
                .iter()
                .map(|a| ((a.path.clone(), a.copy_name.clone()), a.clone()))
                .collect();
        let import_append = std::mem::take(&mut self.import_append);
        let measure_sharpness = self.settings.culling.enabled;
        // Sidecars (from this app or others) fill in photos new to us.
        let read_sidecars = self
            .settings
            .sidecars
            .read_on_import
            .then_some(self.settings.sidecars.include_edits);
        self.is_importing = true;
        self.status_message = Some(format!("Scanning {}…", self.catalog_def.name).into());
        if let Some(folder) = folders.last() {
            self.reveal_folder_in_tree(folder, cx);
        }
        cx.notify();

        cx.spawn(async move |this, cx| {
            let scan_folders = folders.clone();
            let (assets, cache) = cx
                .background_executor()
                .spawn(async move {
                    let mut all = Vec::new();
                    for folder in &scan_folders {
                        let mut assets = rustyedit_io::scan_folder(folder);
                        assets.retain(|asset| !excluded.contains(&asset.path));
                        let mut missing = Vec::new();
                        let mut known: HashSet<PathBuf> = HashSet::new();
                        if let Some(saved) = rustyedit_io::load_catalog(folder) {
                            rustyedit_io::merge_saved_state(&mut assets, &saved);
                            missing = rustyedit_io::missing_assets(&saved);
                            known.extend(saved.assets.iter().map(|a| a.path.clone()));
                        }
                        // Photos neither the folder's catalog nor the
                        // library has seen: take what their sidecars say.
                        if let Some(with_edits) = read_sidecars {
                            for asset in assets.iter_mut().filter(|a| {
                                !known.contains(&a.path)
                                    && !current.contains_key(&(a.path.clone(), None))
                            }) {
                                if let Some(data) = rustyedit_io::xmp::read_sidecar(&asset.path) {
                                    data.apply_to(asset, with_edits);
                                }
                            }
                        }
                        missing.retain(|asset| !excluded.contains(&asset.path));
                        all.extend(assets);
                        // Keep vanished photos listed so they can be located
                        // or explicitly removed instead of silently dropped.
                        all.extend(missing);
                    }
                    // The same file under two folders (a parent and a child
                    // added separately) is listed once.
                    // Virtual copies share their original's file, so they
                    // count by file and copy name.
                    let mut seen = HashSet::new();
                    all.retain(|asset| seen.insert((asset.path.clone(), asset.copy_name.clone())));
                    // Photos already in the library keep everything set on
                    // them, even if a folder's saved catalog didn't have it
                    // (e.g. a parent folder absorbing a child's).
                    for asset in all.iter_mut() {
                        if let Some(old) = current.get(&(asset.path.clone(), asset.copy_name.clone())) {
                            asset.carry_over_from(old);
                        }
                    }
                    // Ids are positions in the combined list.
                    for (ix, asset) in all.iter_mut().enumerate() {
                        asset.id = ix as u64;
                    }
                    // One shared thumbnail namespace, so a folder that's in
                    // several catalogs is only thumbnailed once.
                    (
                        all,
                        ThumbnailCache::for_folder(std::path::Path::new("library")),
                    )
                })
                .await;

            // The photos go on show now, as they are found; their thumbnails
            // follow batch by batch, so the grid and the filmstrip fill in
            // while the rest are still being made.
            // Let any background save of the outgoing catalog finish first,
            // so it can't land on disk after the final save below.
            while this
                .update(cx, |this, _| this.catalog_save_in_flight)
                .unwrap_or(false)
            {
                cx.background_executor()
                    .timer(Duration::from_millis(20))
                    .await;
            }
            let total = assets.len();
            let done = Arc::new(AtomicUsize::new(0));
            let shown = this.update(cx, |this, cx| {
                let after_paths: HashSet<PathBuf> = assets.iter().map(|a| a.path.clone()).collect();
                let missing = assets.iter().filter(|a| a.missing).count();
                let added = after_paths.difference(&before_paths).count();
                let removed = before_paths.difference(&after_paths).count();
                // After an import, go to the first new photo; otherwise stay
                // on the one that was selected.
                let first_new = assets
                    .iter()
                    .find(|a| !before_paths.contains(&a.path))
                    .map(|a| a.path.clone());
                let previous = this.catalog.selected_asset().map(|a| a.path.clone());
                let select = select.or(if import_append {
                    first_new.or(previous)
                } else {
                    previous
                });
                let kept = after_paths.intersection(&before_paths).count();
                let catalog = Catalog::new_multi(folders, assets);
                this.install_catalog(catalog, Some(Arc::new(cache.clone())), select, cx);
                this.import_progress = Some((done.clone(), total));
                // Scanning is over: the library is on show and can be used
                // (or switched away from) while its thumbnails come in.
                this.is_importing = false;
                this.status_message = Some(format!("Generating thumbnails for {total} photos…").into());
                cx.notify();
                (added, removed, missing, kept)
            });
            let Ok((added, removed, missing, kept)) = shown else {
                return;
            };

            // The thumbnails, in batches, in the order of the catalog.
            let pending: Arc<std::sync::Mutex<Vec<rustyedit_io::ThumbInfo>>> = Default::default();
            let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let thumb_assets = this
                .update(cx, |this, _| this.catalog.assets.clone())
                .unwrap_or_default();
            let cancel_job = cancel.clone();
            let job = cx.background_executor().spawn({
                let (pending, finished, done) = (pending.clone(), finished.clone(), done.clone());
                async move {
                    rustyedit_io::generate_thumbnails_in_batches(
                        &cache,
                        &thumb_assets,
                        &done,
                        measure_sharpness,
                        THUMBNAIL_BATCH,
                        &cancel_job,
                        |batch| pending.lock().unwrap().extend(batch),
                    );
                    finished.store(true, std::sync::atomic::Ordering::Release);
                }
            });
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                // Read this first: what was finished is all in `pending`.
                let last = finished.load(std::sync::atomic::Ordering::Acquire);
                let batch = std::mem::take(&mut *pending.lock().unwrap());
                let alive = this
                    .update(cx, |this, cx| {
                        if this.import_epoch != epoch {
                            return false;
                        }
                        this.apply_thumbnails(batch);
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if last || !alive {
                    break;
                }
            }
            job.await;

            this.update(cx, |this, cx| {
                // Another catalog was opened meanwhile; it reports for itself.
                if this.import_epoch != epoch {
                    return;
                }
                this.facets = crate::filters_panel::Facets::build(&this.catalog);
                this.mark_catalog_dirty(cx);
                if import_append {
                    this.post_scan_message = Some(
                        if added == 0 && kept == 0 {
                            "No photos found in that folder.".to_string()
                        } else if added == 0 {
                            format!(
                                "No new photos \u{2014} everything was already in the library ({kept} photos)."
                            )
                        } else {
                            format!(
                                "Added {added} new photo{} to the library ({} in total).",
                                if added == 1 { "" } else { "s" },
                                kept + added
                            )
                        }
                        .into(),
                    );
                }
                this.is_importing = false;
                this.import_progress = None;
                if this.pending_sync_report {
                    this.pending_sync_report = false;
                    let removed_note = if removed > 0 {
                        format!(", {removed} removed")
                    } else {
                        String::new()
                    };
                    let missing_note = if missing > 0 {
                        format!(", {missing} missing")
                    } else {
                        String::new()
                    };
                    this.status_message = Some(
                        format!("Synchronized: {added} added{removed_note}{missing_note}.").into(),
                    );
                } else {
                    this.status_message = this.post_scan_message.take();
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Puts what a batch of thumbnails found (the thumbnail, size, EXIF facts
    /// and hashes) on the photos in the library, which is already on show.
    /// Ids are list positions, so each is looked up directly.
    fn apply_thumbnails(&mut self, batch: Vec<rustyedit_io::ThumbInfo>) {
        if batch.is_empty() {
            return;
        }
        for info in batch {
            let id = info.id;
            if let Some(asset) = self.catalog.assets.get_mut(id as usize).filter(|a| a.id == id) {
                asset.thumbnail_path = Some(info.thumbnail);
                asset.width = info.width;
                asset.height = info.height;
                asset.orientation = info.orientation;
                asset.meta = info.meta;
                asset.content_hash = info.content_hash;
                asset.phash = info.phash;
                asset.sharpness = info.sharpness;
            }
        }
        // Dates and sizes arrive with the thumbnails, and the order and
        // the filters depend on them.
        let sort = self.catalog.sort();
        self.catalog.set_sort(sort);
    }

    /// Swaps in a freshly loaded catalog, resetting everything keyed by the
    /// old one's asset ids.
    fn install_catalog(
        &mut self,
        catalog: Catalog,
        cache: Option<Arc<ThumbnailCache>>,
        select: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.save_outgoing_catalog();
        self.catalog = catalog;
        // Fresh catalog: re-apply the saved sort and rebuild the facet lists.
        self.catalog.set_sort(self.settings.library.sort);
        self.catalog.set_pair_view(self.settings.library.pair_view);
        self.facets = crate::filters_panel::Facets::build(&self.catalog);
        self.thumbnail_cache = cache;
        self.mark_catalog_dirty(cx);
        self.histogram_cache.clear();
        self.histogram_loading = None;
        self.develop_histogram = None;
        self.active_album = None;
        self.active_smart = None;
        self.duplicate_groups = None;
        self.duplicates_loading = false;
        self.metadata_cache.clear();
        self.metadata_loading.clear();
        self.loupe_paths.clear();
        self.loupe_loading.clear();
        self.loupe_full_paths.clear();
        self.loupe_full_loading.clear();
        self.selection.clear();
        self.selection_anchor = None;
        // Keyed by asset id, and ids are reassigned on every load.
        self.edited_thumbs = Default::default();
        self.export_excluded.clear();
        self.export_added.clear();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.loupe_zoom = None;

        let selected =
            select.and_then(|path| self.catalog.assets.iter().position(|a| a.path == path));
        match selected {
            Some(ix) => self.select_asset(ix, cx),
            None => self.on_selection_changed(cx),
        }
    }
}
