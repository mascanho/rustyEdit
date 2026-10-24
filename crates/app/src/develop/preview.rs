//! The live preview shown while editing: the develop pipeline runs on a
//! background thread and its output goes straight to the GPU as an
//! in-memory image.
//!
//! Earlier versions wrote each render to a temp JPEG and showed it by path.
//! GPUI loads path images asynchronously, so every slider step left the
//! viewer empty for a frame or two until the new file was read back. That
//! was the flicker while dragging. An in-memory `RenderImage` is ready the
//! moment it's swapped in, so the old frame stays until the new one replaces it.

use std::sync::Arc;

use gpui::{Context, RenderImage, Window};
use image::{Frame, RgbaImage};
use rustyedit_core::DevelopSettings;
use rustyedit_processing::ClipWarnings;

use crate::workspace::Workspace;

/// The current preview frame plus the bookkeeping that coalesces renders.
#[derive(Default)]
pub(crate) struct LivePreview {
    /// What the viewer shows. `None` until the first render of this photo.
    image: Option<Arc<RenderImage>>,
    /// Frames that were replaced and still occupy GPU atlas space, with
    /// how many render passes have happened since. See `release_retired`.
    retired: Vec<(Arc<RenderImage>, u8)>,
    /// The photo without its edits (but with its crop and rotation), for
    /// Before / After; `before_key` says which photo and geometry it is for.
    before: Option<Arc<RenderImage>>,
    before_key: Option<(u64, DevelopSettings)>,
    /// A render is running on the background executor.
    busy: bool,
    /// Adjustments changed while busy; render again when it finishes.
    pending: bool,
}

impl LivePreview {
    pub(crate) fn image(&self) -> Option<Arc<RenderImage>> {
        self.image.clone()
    }

    pub(crate) fn before(&self) -> Option<Arc<RenderImage>> {
        self.before.clone()
    }

    fn show_before(&mut self, image: Arc<RenderImage>, key: (u64, DevelopSettings)) {
        if let Some(old) = self.before.replace(image) {
            self.retire(old);
        }
        self.before_key = Some(key);
    }

    fn retire(&mut self, image: Arc<RenderImage>) {
        self.retired.push((image, 0));
    }

    /// Pixel size of the current frame.
    pub(crate) fn size(&self) -> Option<(f32, f32)> {
        self.image.as_ref().map(|image| {
            let size = image.size(0);
            (size.width.0 as f32, size.height.0 as f32)
        })
    }

    /// Swaps in a freshly rendered frame, retiring the one it replaces.
    fn show(&mut self, image: Arc<RenderImage>) {
        if let Some(old) = self.image.replace(image) {
            self.retire(old);
        }
    }

    /// Forgets the current frame (another photo was selected, caches were
    /// cleared). The viewer falls back to the photo's file until the next
    /// render arrives.
    pub(crate) fn clear(&mut self) {
        if let Some(old) = self.image.take() {
            self.retire(old);
        }
        if let Some(old) = self.before.take() {
            self.retire(old);
        }
        self.before_key = None;
    }

    /// Frees the GPU memory of retired frames. Call once per render.
    ///
    /// A frame is only freed on the third render pass after it was
    /// replaced. GPUI destroys an image's texture immediately, but the
    /// frame drawn just before may still be running on the GPU and reading
    /// it; freeing it then faults the GPU, and GPUI waits forever for that
    /// frame (the app froze mid slider drag). GPUI waits for each frame
    /// to finish before drawing the next, so two passes later nothing in
    /// flight can still be using it; the third is a margin.
    pub(crate) fn release_retired(&mut self, window: &mut Window) {
        const PASSES: u8 = 2;
        self.retired.retain_mut(|(image, passes)| {
            if *passes >= PASSES {
                let _ = window.drop_image(image.clone());
                false
            } else {
                *passes += 1;
                true
            }
        });
    }
}

impl Workspace {
    /// Whether something on screen shows the selected photo rendered with
    /// its edits: Develop, Before / After, the Library's Presets panel, or
    /// the Library loupe for an edited photo.
    pub(crate) fn wants_live_preview(&self) -> bool {
        use crate::workspace::{LibraryView, Mode};
        let edited = self
            .catalog
            .selected_asset()
            .is_some_and(|a| a.develop.is_edited());
        self.mode == Mode::Develop
            || self.ba_active()
            || self.library_presets_open()
            // Not in the cull view: decoding there would slow culling.
            || (self.mode == Mode::Library
                && self.library_view == LibraryView::Loupe
                && !self.cull_view
                && edited)
    }

    /// After the selected photo's edits changed: re-renders its preview if
    /// one is on screen, decoding the photo first if it isn't yet.
    pub(crate) fn refresh_live_preview(&mut self, cx: &mut Context<Self>) {
        if !self.wants_live_preview() {
            return;
        }
        let selected = self.catalog.selected_asset().map(|a| a.id);
        if selected.is_some() && self.source_image_for != selected {
            self.load_source_image(cx);
        } else {
            self.request_preview_update(cx);
        }
    }

    /// Re-renders the develop pipeline for the current asset's adjustments.
    /// Coalesces rapid changes (e.g. a slider drag) into a single trailing
    /// render instead of queueing one render per mouse-move event.
    pub(crate) fn request_preview_update(&mut self, cx: &mut Context<Self>) {
        if self.live_preview.busy {
            self.live_preview.pending = true;
            return;
        }

        let Some(source) = self.source_image.clone() else {
            return;
        };
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        if self.source_image_for != Some(asset.id) {
            return;
        }

        // A hovered preset is previewed without touching the photo.
        let settings = self.preview_override.unwrap_or(asset.develop);
        let asset_id = asset.id;
        let epoch = self.preview_epoch;
        let clipping = ClipWarnings {
            shadows: self.histogram_ui.show_shadow_clipping,
            highlights: self.histogram_ui.show_highlight_clipping,
        };
        // The before image only depends on the photo and its geometry, so it
        // is rendered once, not on every slider step.
        let geometry = DevelopSettings {
            crop: settings.crop,
            rotation_degrees: settings.rotation_degrees,
            flip_horizontal: settings.flip_horizontal,
            flip_vertical: settings.flip_vertical,
            ..DevelopSettings::default()
        };
        let proof = self.develop_ui.proof.settings();
        let before_key = (asset_id, geometry);
        let need_before =
            self.ba_active() && self.live_preview.before_key.as_ref() != Some(&before_key);
        self.live_preview.busy = true;
        self.live_preview.pending = false;

        // The frame, its clipping warnings and the histogram all come
        // from one GPU pass (or the CPU fallback); nothing per-pixel is
        // left to do here.
        let render = cx.background_executor().spawn(async move {
            let mut frame = rustyedit_processing::render_preview(&source, &settings, clipping);
            // Soft proof (and the gamut warning) on top of the frame.
            if let Some(proof) = &proof {
                rustyedit_processing::apply_proof(&mut frame.bgra, proof);
            }
            let before = need_before.then(|| {
                let mut plain = rustyedit_processing::render_preview(
                    &source,
                    &geometry,
                    ClipWarnings::default(),
                );
                if let Some(proof) = &proof {
                    rustyedit_processing::apply_proof(&mut plain.bgra, proof);
                }
                to_render_image(plain.bgra)
            });
            (to_render_image(frame.bgra), frame.histogram, before)
        });

        cx.spawn(async move |this, cx| {
            let (image, histogram, before) = render.await;
            this.update(cx, |this, cx| {
                this.live_preview.busy = false;
                // A render for a photo that's no longer selected is dropped
                // without ever reaching the GPU.
                if this.preview_epoch == epoch {
                    this.live_preview.show(image);
                    if let Some(before) = before {
                        this.live_preview.show_before(before, before_key);
                    }
                    this.develop_histogram = Some((asset_id, Arc::new(histogram)));
                }
                if this.live_preview.pending {
                    this.request_preview_update(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

/// Wraps a BGRA frame as an image GPUI can upload as-is.
fn to_render_image(bgra: RgbaImage) -> Arc<RenderImage> {
    Arc::new(RenderImage::new(vec![Frame::new(bgra)]))
}
