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
use image::{Frame, RgbImage, RgbaImage};
use rustyedit_core::Histogram;

use crate::workspace::Workspace;

/// The current preview frame plus the bookkeeping that coalesces renders.
#[derive(Default)]
pub(crate) struct LivePreview {
    /// What the viewer shows. `None` until the first render of this photo.
    image: Option<Arc<RenderImage>>,
    /// Frames that were replaced and still occupy GPU atlas space. Freed on
    /// the next render pass, which has the window that owns the atlas.
    retired: Vec<Arc<RenderImage>>,
    /// A render is running on the background executor.
    busy: bool,
    /// Adjustments changed while busy; render again when it finishes.
    pending: bool,
}

impl LivePreview {
    pub(crate) fn image(&self) -> Option<Arc<RenderImage>> {
        self.image.clone()
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
            self.retired.push(old);
        }
    }

    /// Forgets the current frame (another photo was selected, caches were
    /// cleared). The viewer falls back to the photo's file until the next
    /// render arrives.
    pub(crate) fn clear(&mut self) {
        if let Some(old) = self.image.take() {
            self.retired.push(old);
        }
    }

    /// Frees the GPU memory of every retired frame. Call once per render.
    pub(crate) fn release_retired(&mut self, window: &mut Window) {
        for image in self.retired.drain(..) {
            let _ = window.drop_image(image);
        }
    }
}

impl Workspace {
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

        let settings = asset.develop;
        let asset_id = asset.id;
        let epoch = self.preview_epoch;
        let clipping = ClippingOverlay {
            shadows: self.histogram_ui.show_shadow_clipping,
            highlights: self.histogram_ui.show_highlight_clipping,
        };
        self.live_preview.busy = true;
        self.live_preview.pending = false;

        let render = cx.background_executor().spawn(async move {
            let rendered = rustyedit_processing::apply(&source, &settings);
            let histogram = Histogram::from_rgb8(rendered.as_raw());
            (to_render_image(&rendered, clipping), histogram)
        });

        cx.spawn(async move |this, cx| {
            let (image, histogram) = render.await;
            this.update(cx, |this, cx| {
                this.live_preview.busy = false;
                // A render for a photo that's no longer selected is dropped
                // without ever reaching the GPU.
                if this.preview_epoch == epoch {
                    this.live_preview.show(image);
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

/// Which clipped pixels to paint over the preview (toggled from the
/// histogram, or with J).
#[derive(Clone, Copy)]
struct ClippingOverlay {
    shadows: bool,
    highlights: bool,
}

impl ClippingOverlay {
    /// The warning colour for an RGB pixel, if it's clipped: blue where
    /// every channel is pure black, red where any channel is blown.
    fn colour(&self, [r, g, b]: [u8; 3]) -> Option<[u8; 3]> {
        if self.highlights && r.max(g).max(b) == 255 {
            Some([255, 40, 40])
        } else if self.shadows && r.max(g).max(b) == 0 {
            Some([40, 110, 255])
        } else {
            None
        }
    }
}

/// Packs an RGB render into the BGRA frame GPUI uploads to the GPU,
/// painting clipped pixels when asked. The histogram is taken from the
/// render beforehand, so the warning colours never skew it.
fn to_render_image(rgb: &RgbImage, clipping: ClippingOverlay) -> Arc<RenderImage> {
    let (width, height) = rgb.dimensions();
    let mut bgra = Vec::with_capacity(width as usize * height as usize * 4);
    let overlay = clipping.shadows || clipping.highlights;
    for pixel in rgb.as_raw().chunks_exact(3) {
        let [r, g, b] = [pixel[0], pixel[1], pixel[2]];
        let [r, g, b] = if overlay {
            clipping.colour([r, g, b]).unwrap_or([r, g, b])
        } else {
            [r, g, b]
        };
        bgra.extend_from_slice(&[b, g, r, 255]);
    }
    let buffer = RgbaImage::from_raw(width, height, bgra)
        .expect("buffer is exactly width * height * 4 bytes");
    Arc::new(RenderImage::new(vec![Frame::new(buffer)]))
}
