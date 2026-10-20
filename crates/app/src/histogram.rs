use std::sync::Arc;

use gpui::{div, prelude::*, AnyElement, Context};
use rustyedit_core::Histogram;

use crate::workspace::{Mode, Workspace};

/// Histograms are ~4 KB each; cap the cache so browsing a huge catalog
/// can't grow it without bound. Simply cleared when full — recomputing
/// from a cached thumbnail takes only a few milliseconds.
const HISTOGRAM_CACHE_LIMIT: usize = 2048;

impl Workspace {
    /// Computes the selected photo's histogram from its cached thumbnail on
    /// a background thread, unless it's already cached. Only one job runs
    /// at a time; when it finishes it re-checks the selection, so holding
    /// an arrow key skips straight to wherever the user stopped.
    pub(crate) fn request_histogram(&mut self, cx: &mut Context<Self>) {
        if self.histogram_loading.is_some() {
            return;
        }
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        if self.histogram_cache.contains_key(&asset.id) {
            return;
        }
        // Thumbnails are small (512px), which keeps this fast and is
        // plenty of samples for an accurate tonal distribution.
        let Some(thumbnail) = asset.thumbnail_path.clone() else {
            return;
        };
        let asset_id = asset.id;
        self.histogram_loading = Some(asset_id);

        let compute = cx.background_executor().spawn(async move {
            image::open(&thumbnail)
                .ok()
                .map(|image| Histogram::from_rgb8(image.to_rgb8().as_raw()))
        });

        cx.spawn(async move |this, cx| {
            let histogram = compute.await;
            this.update(cx, |this, cx| {
                // An import resets `histogram_loading`; a job started before
                // it belongs to the previous folder and must be discarded.
                if this.histogram_loading != Some(asset_id) {
                    return;
                }
                this.histogram_loading = None;
                if let Some(histogram) = histogram {
                    if this.histogram_cache.len() >= HISTOGRAM_CACHE_LIMIT {
                        this.histogram_cache.clear();
                    }
                    this.histogram_cache.insert(asset_id, Arc::new(histogram));
                }
                this.request_histogram(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// In Develop, the live histogram of the edited preview once one has
    /// been rendered; otherwise the original photo's histogram.
    fn current_histogram(&self) -> (Option<Arc<Histogram>>, &'static str) {
        let Some(asset) = self.catalog.selected_asset() else {
            return (None, "");
        };
        if self.mode == Mode::Develop {
            if let Some((id, histogram)) = &self.develop_histogram {
                if *id == asset.id {
                    return (Some(histogram.clone()), "Edited preview");
                }
            }
        }
        (self.histogram_cache.get(&asset.id).cloned(), "Original")
    }

    /// The "Main" section at the top of the right-hand panels, currently
    /// holding the histogram.
    pub(crate) fn render_main_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let (histogram, source) = self.current_histogram();
        let caption = if histogram.is_none() && self.catalog.selected_asset().is_some() {
            "Calculating…"
        } else {
            source
        };

        self.disclosure(
            "main",
            "Main",
            vec![
                rustyedit_ui::histogram_view(&theme, histogram).into_any_element(),
                div()
                    .text_xs()
                    .text_color(theme.text_faint)
                    .child(caption)
                    .into_any_element(),
            ],
            cx,
        )
    }
}

impl Workspace {
    /// What the loupe should display for `asset`: the original when GPUI can
    /// render it directly, otherwise the generated large preview, falling
    /// back to the small thumbnail while that is still being produced.
    pub(crate) fn loupe_source(&self, asset: &rustyedit_core::Asset) -> std::path::PathBuf {
        if !rustyedit_io::ThumbnailCache::needs_loupe_preview(
            &asset.path,
            asset.orientation,
            asset.rotation_quarters(),
        ) {
            return asset.path.clone();
        }
        self.loupe_paths
            .get(&asset.id)
            .cloned()
            .unwrap_or_else(|| asset.display_source().to_path_buf())
    }

    /// Generates a loupe-size preview in the background for formats the UI
    /// can't render natively (RAW, or rotated by EXIF). No-op for the rest.
    pub(crate) fn request_loupe(&mut self, cx: &mut Context<Self>) {
        self.request_loupe_for(self.catalog.selected, cx);
    }

    pub(crate) fn request_loupe_for(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(asset) = self.catalog.assets.get(index) else {
            return;
        };
        let Some(cache) = self.thumbnail_cache.clone() else {
            return;
        };
        let quarters = asset.rotation_quarters();
        if !rustyedit_io::ThumbnailCache::needs_loupe_preview(
            &asset.path,
            asset.orientation,
            quarters,
        ) || self.loupe_paths.contains_key(&asset.id)
            || !self.loupe_loading.insert(asset.id)
        {
            return;
        }
        let (asset_id, path) = (asset.id, asset.path.clone());
        let job = cx.background_executor().spawn(async move {
            if quarters == 0 {
                cache.ensure_loupe(&path).ok()
            } else {
                cache.ensure_loupe_rotated(&path, quarters).ok()
            }
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |this, cx| {
                this.loupe_loading.remove(&asset_id);
                if let Some(path) = result {
                    this.loupe_paths.insert(asset_id, path);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Full-resolution copy for zooming past the loupe preview. Only needed
    /// when the original can't be displayed directly.
    pub(crate) fn request_full_preview(&mut self, cx: &mut Context<Self>) {
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        let Some(cache) = self.thumbnail_cache.clone() else {
            return;
        };
        let quarters = asset.rotation_quarters();
        if !rustyedit_io::ThumbnailCache::needs_loupe_preview(
            &asset.path,
            asset.orientation,
            quarters,
        ) || self.loupe_full_paths.contains_key(&asset.id)
            || !self.loupe_full_loading.insert(asset.id)
        {
            return;
        }
        let (asset_id, path) = (asset.id, asset.path.clone());
        let job = cx.background_executor().spawn(async move {
            if quarters == 0 {
                cache.ensure_full(&path).ok()
            } else {
                cache.ensure_full_rotated(&path, quarters).ok()
            }
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |this, cx| {
                this.loupe_full_loading.remove(&asset_id);
                if let Some(path) = result {
                    this.loupe_full_paths.insert(asset_id, path);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
