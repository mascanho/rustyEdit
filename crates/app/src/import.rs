use std::{
    path::PathBuf,
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};

use gpui::Context;
use rustyedit_core::Catalog;
use rustyedit_io::ThumbnailCache;

use crate::workspace::Workspace;

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
                .spawn(async { rfd::FileDialog::new().pick_folder() })
                .await;

            this.update(cx, |this, cx| {
                this.is_importing = false;
                match folder {
                    Some(folder) => this.import_folder(folder, None, cx),
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
        if self.catalog_def.add_folder(&folder) {
            self.save_catalog_def();
        }
        self.open_catalog(select, cx);
    }

    /// Scans every folder of the active catalog and generates thumbnails
    /// entirely off the main thread, only touching `self` to report progress
    /// and to install the finished catalog.
    pub(crate) fn open_catalog(&mut self, select: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.is_importing {
            return;
        }
        let folders = self.catalog_def.folders.clone();
        if folders.is_empty() {
            self.install_catalog(Catalog::default(), self.thumbnail_cache.clone(), None, cx);
            self.status_message = Some("This catalog is empty \u{2014} import a folder.".into());
            return;
        }
        self.is_importing = true;
        self.status_message = Some(format!("Scanning {}…", self.catalog_def.name).into());
        if let Some(folder) = folders.last() {
            self.reveal_folder_in_tree(folder, cx);
        }
        cx.notify();

        cx.spawn(async move |this, cx| {
            let scan_folders = folders.clone();
            let (mut assets, cache) = cx
                .background_executor()
                .spawn(async move {
                    let mut all = Vec::new();
                    for folder in &scan_folders {
                        let mut assets = rustyedit_io::scan_folder(folder);
                        if let Some(saved) = rustyedit_io::load_catalog(folder) {
                            rustyedit_io::merge_saved_state(&mut assets, &saved);
                        }
                        all.extend(assets);
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

            this.update(cx, |this, cx| {
                this.status_message =
                    Some(format!("Generating thumbnails for {} photos…", assets.len()).into());
                cx.notify();
            })
            .ok();

            let thumb_assets = assets.clone();
            let thumb_cache = cache.clone();
            let done = Arc::new(AtomicUsize::new(0));
            let worker_done = done.clone();
            this.update(cx, |this, _| {
                this.import_progress = Some((done, thumb_assets.len()));
            })
            .ok();
            let job = cx.background_executor().spawn(async move {
                rustyedit_io::generate_thumbnails(&thumb_cache, &thumb_assets, &worker_done)
            });
            // Repaint a few times a second so the loader's progress bar moves.
            let ticker = cx.spawn({
                let this = this.clone();
                async move |cx| {
                    while this.update(cx, |_, cx| cx.notify()).is_ok() {
                        cx.background_executor()
                            .timer(Duration::from_millis(100))
                            .await;
                    }
                }
            });
            let thumbnails = job.await;
            drop(ticker);

            // Ids are list positions, so look each one up directly instead
            // of searching (which is O(n²) on big imports).
            for (id, path, width, height, orientation) in thumbnails {
                if let Some(asset) = assets.get_mut(id as usize).filter(|a| a.id == id) {
                    asset.thumbnail_path = Some(path);
                    asset.width = width;
                    asset.height = height;
                    asset.orientation = orientation;
                }
            }

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

            this.update(cx, |this, cx| {
                let catalog = Catalog::new_multi(folders, assets);
                this.install_catalog(catalog, Some(Arc::new(cache)), select, cx);
                this.is_importing = false;
                this.import_progress = None;
                this.status_message = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
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
        self.thumbnail_cache = cache;
        self.mark_catalog_dirty(cx);
        self.histogram_cache.clear();
        self.histogram_loading = None;
        self.develop_histogram = None;
        self.active_album = None;
        self.metadata_cache.clear();
        self.metadata_loading.clear();
        self.loupe_paths.clear();
        self.loupe_loading.clear();

        let selected =
            select.and_then(|path| self.catalog.assets.iter().position(|a| a.path == path));
        match selected {
            Some(ix) => self.select_asset(ix, cx),
            None => self.on_selection_changed(cx),
        }
    }
}
