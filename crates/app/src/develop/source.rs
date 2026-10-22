//! Decoding the selected photo: in full (RAW files as sensor data, other
//! images converted into the editing colour space), plus the downscaled
//! "smart preview" the live preview renders from.

use gpui::Context;

use crate::workspace::Workspace;

impl Workspace {
    /// Decodes the currently selected asset's full source file on a
    /// background thread, downscales it to a manageable "preview" size, and
    /// stashes the result so sliders can re-render quickly without
    /// re-reading the file from disk on every change.
    pub(crate) fn load_source_image(&mut self, cx: &mut Context<Self>) {
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        let asset_id = asset.id;
        let path = asset.path.clone();
        let source_path = path.clone();
        let epoch = self.preview_epoch;
        // Longest edge the decoded source is downscaled to before it ever
        // touches the develop pipeline. Lightroom calls this a "smart
        // preview"; it keeps slider drags fast regardless of how big the
        // original is, at the cost of not previewing at 100% detail.
        let max_edge = self.settings.develop.preview_quality.max_edge();

        crate::display_profile::refresh();
        let decode = cx.background_executor().spawn(async move {
            let full = rustyedit_io::open_for_develop(&path).ok()?;
            let preview = rustyedit_processing::prepare_preview(&full, max_edge);
            Some((full, preview))
        });

        cx.spawn(async move |this, cx| {
            let decoded = decode.await;
            this.update(cx, |this, cx| {
                if this.preview_epoch != epoch {
                    return;
                }
                if let Some((full, preview)) = decoded {
                    this.source_image = Some(preview);
                    this.full_source = Some(full);
                    this.source_image_for = Some(asset_id);
                    this.source_image_path = Some(source_path);
                    this.request_preview_update(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Drops the decoded source and the live preview, e.g. when another
    /// photo is selected. The viewer shows the photo's file until a new
    /// preview has been rendered.
    pub(crate) fn reset_develop_preview(&mut self) {
        self.source_image = None;
        self.full_source = None;
        self.source_image_for = None;
        self.source_image_path = None;
        self.live_preview.clear();
        self.develop_ui.retouch.reset_selection();
        self.develop_ui.gradients.reset_selection();
        // A new photo keeps the zoom level, opening on its centre.
        self.develop_ui.zoom.centre_on(0.5, 0.5);
    }
}
