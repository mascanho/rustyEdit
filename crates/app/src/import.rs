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

    /// Scans `folder` and generates thumbnails entirely off the main
    /// thread, only touching `self` to report progress and to install the
    /// finished catalog. This is the "Import" flow: the one-time cost that
    /// makes everything after it (culling) fast, since thumbnails are
    /// already decoded and cached on disk. If `select` names a photo in the
    /// folder, it's selected once the import completes.
    pub(crate) fn import_folder(
        &mut self,
        folder: PathBuf,
        select: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if self.is_importing {
            return;
        }
        self.is_importing = true;
        self.status_message = Some(format!("Scanning {}…", folder.display()).into());
        self.reveal_folder_in_tree(&folder, cx);
        cx.notify();

        cx.spawn(async move |this, cx| {
            let scan_folder = folder.clone();
            let (mut assets, cache) = cx
                .background_executor()
                .spawn(async move {
                    let assets = rustyedit_io::scan_folder(&scan_folder);
                    let cache = ThumbnailCache::for_folder(&scan_folder);
                    (assets, cache)
                })
                .await;

            if let Some(saved) = rustyedit_io::load_catalog(&folder) {
                rustyedit_io::merge_saved_state(&mut assets, &saved);
            }

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

            // `scan_folder` assigns ids as list positions, so look each one up
            // directly instead of searching (which is O(n²) on big imports).
            for (id, path, width, height) in thumbnails {
                if let Some(asset) = assets.get_mut(id as usize).filter(|a| a.id == id) {
                    asset.thumbnail_path = Some(path);
                    asset.width = width;
                    asset.height = height;
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
                this.save_outgoing_catalog();
                this.catalog = Catalog::new(folder, assets);
                this.thumbnail_cache = Some(Arc::new(cache));
                this.is_importing = false;
                this.import_progress = None;
                this.status_message = None;
                this.mark_catalog_dirty(cx);
                // Asset ids restart at 0 per folder, so cached histograms
                // from the previous folder would match the wrong photos.
                this.histogram_cache.clear();
                this.histogram_loading = None;
                this.develop_histogram = None;
                this.metadata_cache.clear();
                this.metadata_loading.clear();
                this.loupe_paths.clear();
                this.loupe_loading.clear();

                let selected =
                    select.and_then(|path| this.catalog.assets.iter().position(|a| a.path == path));
                match selected {
                    Some(ix) => this.select_asset(ix, cx),
                    None => this.on_selection_changed(cx),
                }
            })
            .ok();
        })
        .detach();
    }
}
