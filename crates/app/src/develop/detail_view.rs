//! The sharp zoomed-in view: when Develop is zoomed past what the working
//! preview can show, the part of the photo on screen is rendered from the
//! full-size photo at screen resolution and laid over the (enlarged)
//! preview.
//!
//! Only what's visible is rendered, so it costs about as much as one
//! screenful of pixels whatever the photo's size. It waits until panning,
//! zooming or a slider drag pauses (`SETTLE`), and while it's out of date
//! the enlarged preview shows instead, so dragging stays as quick as at
//! Fit. It only runs where the GPU can do it (see
//! `rustyedit_processing::detail_is_fast`).

use std::{sync::Arc, time::Duration};

use gpui::{Context, RenderImage, Task};
use rustyedit_core::DevelopSettings;

use crate::workspace::{Mode, Workspace};

/// How long the view has to stay put before the sharp render starts.
const SETTLE: Duration = Duration::from_millis(140);

/// Left, top, width, height as fractions of the cropped photo.
pub(crate) type Region = (f32, f32, f32, f32);

/// What a detail render is of: the photo, its settings, the region
/// (fractions of the cropped photo) and the pixel size it's delivered at.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct DetailKey {
    pub(crate) asset: u64,
    pub(crate) settings: DevelopSettings,
    pub(crate) region: Region,
    pub(crate) size: (u32, u32),
}

#[derive(Default)]
pub(crate) struct DetailView {
    /// The last finished render and what it is of.
    pub(crate) shown: Option<(Arc<RenderImage>, DetailKey)>,
    /// What was last asked for (waiting to settle, or rendering).
    requested: Option<DetailKey>,
    busy: bool,
    /// The settle timer; replacing it cancels the previous one.
    timer: Option<Task<()>>,
}

impl DetailView {
    /// Takes the shown render out (to be retired), forgetting any request.
    pub(crate) fn take(&mut self) -> Option<Arc<RenderImage>> {
        self.requested = None;
        self.timer = None;
        self.shown.take().map(|(image, _)| image)
    }
}

impl Workspace {
    /// The photo's size in full-resolution pixels, as cropped: what Develop's
    /// zoom percentages are of, so 100% is one photo pixel per screen pixel.
    pub(crate) fn develop_zoom_size(&self) -> Option<(f32, f32)> {
        let (w, h) = self.develop_image_size()?;
        let factor = match (&self.full_source, &self.source_image) {
            (Some(full), Some(preview)) if preview.width() > 0 => {
                (full.width() as f32 / preview.width() as f32).max(1.0)
            }
            // Still decoding: the preview will be the photo's long edge
            // brought down to the preview size.
            _ => self.catalog.selected_asset().map_or(1.0, |a| {
                let long = a.width.max(a.height) as f32;
                let max_edge = self.settings.develop.preview_quality.max_edge() as f32;
                (long / max_edge).max(1.0)
            }),
        };
        Some((w * factor, h * factor))
    }

    /// Whether the detail view can show now: zoomed in Develop on the
    /// plain photo (no comparison, crop canvas, clipping warnings, soft
    /// proof or preset preview, which only the full preview draws).
    fn detail_allowed(&self) -> bool {
        let zoomed = match self.mode {
            Mode::Develop => self.develop_ui.zoom.scale.is_some(),
            Mode::Library => self.loupe_zoom.is_some() && self.loupe_shows_edits(),
            Mode::Export => false,
        };
        zoomed
            && !self.ba_active()
            && !self.crop_in_view()
            && !self.histogram_ui.show_shadow_clipping
            && !self.histogram_ui.show_highlight_clipping
            && !self.develop_ui.proof.active()
            && self.preview_override.is_none()
            && self.live_preview.image().is_some()
            && self
                .full_source
                .as_ref()
                .is_some_and(rustyedit_processing::detail_is_fast)
    }

    /// The detail render the view needs right now, if any: none when the
    /// enlarged preview is already as sharp as the screen.
    fn wanted_detail(&self) -> Option<DetailKey> {
        if !self.detail_allowed() {
            return None;
        }
        let asset = self.catalog.selected_asset()?;
        if self.source_image_for != Some(asset.id) {
            return None;
        }
        let (region, (w, h), ratio) = self.detail_viewport()?;
        let (preview_w, _) = self.develop_image_size()?;
        let (full_w, full_h) = self.develop_zoom_size()?;
        // Screen pixels the region covers, and photo pixels it holds.
        let screen = (region.2 * w * ratio, region.3 * h * ratio);
        if screen.0 <= region.2 * preview_w * 1.05 {
            return None;
        }
        let size = (
            screen.0.min(region.2 * full_w).round().max(1.0) as u32,
            screen.1.min(region.3 * full_h).round().max(1.0) as u32,
        );
        Some(DetailKey {
            asset: asset.id,
            settings: asset.develop,
            region,
            size,
        })
    }

    /// Whether the Library loupe shows the selected photo with its edits
    /// (rendered) rather than its file.
    pub(crate) fn loupe_shows_edits(&self) -> bool {
        self.mode == Mode::Library
            && self.library_view == crate::workspace::LibraryView::Loupe
            && self.catalog.selected_asset().is_some_and(|a| {
                a.develop.is_edited() && self.source_image_for == Some(a.id)
            })
            && self.live_preview.image().is_some()
    }

    /// What's in view of the zoomed photo (fractions), the photo's size on
    /// screen (logical pixels) and the display's pixel ratio.
    fn detail_viewport(&self) -> Option<(Region, (f32, f32), f32)> {
        match self.mode {
            Mode::Develop => {
                let zoom = &self.develop_ui.zoom;
                Some((zoom.visible_region()?, zoom.zoomed_size.get(), zoom.pixel_ratio.get()))
            }
            Mode::Library => {
                let (w, h) = self.loupe_zoomed_size.get();
                let view = self.loupe_scroll.bounds().size;
                let (vw, vh) = (f32::from(view.width), f32::from(view.height));
                if w <= 0.0 || h <= 0.0 || vw <= 0.0 || vh <= 0.0 {
                    return None;
                }
                let offset = self.loupe_scroll.offset();
                let (fw, fh) = ((vw / w).min(1.0), (vh / h).min(1.0));
                let left = (-f32::from(offset.x) / w).clamp(0.0, 1.0 - fw);
                let top = (-f32::from(offset.y) / h).clamp(0.0, 1.0 - fh);
                Some(((left, top, fw, fh), (w, h), self.pixel_ratio.get()))
            }
            Mode::Export => None,
        }
    }

    /// The finished render to lay over the zoomed photo: only one of this
    /// photo with its current settings (an out-of-date one would show old
    /// edits).
    pub(crate) fn detail_tile(&self) -> Option<(Arc<RenderImage>, Region)> {
        if !self.detail_allowed() {
            return None;
        }
        let asset = self.catalog.selected_asset()?;
        let (image, key) = self.live_preview.detail.shown.as_ref()?;
        (key.asset == asset.id && key.settings == asset.develop)
            .then(|| (image.clone(), key.region))
    }

    /// Called as the window is drawn: asks for a new detail render once the
    /// view settles, if what's shown isn't what's needed.
    pub(crate) fn sync_detail_view(&mut self, cx: &mut Context<Self>) {
        let Some(wanted) = self.wanted_detail() else {
            self.live_preview.detail.requested = None;
            self.live_preview.detail.timer = None;
            return;
        };
        let detail = &self.live_preview.detail;
        if detail.shown.as_ref().map(|(_, k)| k) == Some(&wanted)
            || detail.requested == Some(wanted)
        {
            return;
        }
        self.live_preview.detail.requested = Some(wanted);
        self.live_preview.detail.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            this.update(cx, |this, cx| this.start_detail_render(cx)).ok();
        }));
    }

    fn start_detail_render(&mut self, cx: &mut Context<Self>) {
        self.live_preview.detail.timer = None;
        if self.live_preview.detail.busy {
            // Asked again when the running one finishes.
            self.live_preview.detail.requested = None;
            return;
        }
        // The latest view, which may have moved since the timer started.
        let Some(key) = self.wanted_detail() else {
            return;
        };
        let Some(full) = self.full_source.clone() else {
            return;
        };
        self.live_preview.detail.requested = Some(key);
        self.live_preview.detail.busy = true;
        let epoch = self.preview_epoch;
        let render = cx.background_executor().spawn(async move {
            let bgra =
                rustyedit_processing::render_detail(&full, &key.settings, key.region, key.size);
            Arc::new(RenderImage::new(vec![image::Frame::new(bgra)]))
        });
        cx.spawn(async move |this, cx| {
            let image = render.await;
            this.update(cx, |this, cx| {
                let detail = &mut this.live_preview.detail;
                detail.busy = false;
                if this.preview_epoch == epoch {
                    let old = detail.shown.replace((image, key)).map(|(old, _)| old);
                    if let Some(old) = old {
                        this.live_preview.retire_image(old);
                    }
                } else {
                    this.live_preview.retire_image(image);
                }
                if this.live_preview.detail.requested == Some(key) {
                    this.live_preview.detail.requested = None;
                }
                // Draws it, and asks again if the view moved meanwhile.
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
