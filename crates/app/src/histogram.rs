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
