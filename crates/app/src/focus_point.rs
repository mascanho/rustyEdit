//! The camera's autofocus point, drawn as a square over the photo in
//! Develop (switch with A, or from the command palette). The point is
//! read from the file's metadata (see `rustyedit_io::read_focus_point`) in
//! the background, once per photo.

use gpui::{hsla, AnyElement, Context};
use rustyedit_core::to_display;

use crate::{actions::ToggleFocusPoint, workspace::Workspace};

/// The square's side as a fraction of the photo's longer edge on screen,
/// never smaller than [`MIN_SIDE`] pixels.
const SIDE: f32 = 0.05;
const MIN_SIDE: f32 = 30.0;

impl Workspace {
    pub fn toggle_focus_point(
        &mut self,
        _: &ToggleFocusPoint,
        _window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.update_settings(|s| s.layout.show_focus_point = !s.layout.show_focus_point, cx);
        let on = self.settings.layout.show_focus_point;
        self.status_message = Some(
            if on { "Focus point shown (A to hide)" } else { "Focus point hidden" }.into(),
        );
        if on {
            self.request_focus_point(cx);
        }
        cx.notify();
    }

    /// Reads the selected photo's focus point on a background thread, once.
    pub(crate) fn request_focus_point(&mut self, cx: &mut Context<Self>) {
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        let path = asset.path.clone();
        if self.focus_cache.contains_key(&path) || !self.focus_loading.insert(path.clone()) {
            return;
        }
        let job = {
            let path = path.clone();
            cx.background_executor()
                .spawn(async move { rustyedit_io::read_focus_point(&path) })
        };
        cx.spawn(async move |this, cx| {
            let point = job.await;
            this.update(cx, |this, cx| {
                this.focus_loading.remove(&path);
                this.focus_cache.insert(path, point);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The square over the Develop photo, when it's switched on and the
    /// file recorded a focus point that is still in view.
    pub(super) fn render_focus_overlay(&self) -> Option<AnyElement> {
        if !self.settings.layout.show_focus_point {
            return None;
        }
        let asset = self.catalog.selected_asset()?;
        let (x, y) = (*self.focus_cache.get(&asset.path)?)?;
        let develop = asset.develop;
        let photo = self
            .develop_ui
            .zoom
            .photo_rect(self.develop_image_size()?)?;

        // Turned and flipped as the photo is, then into the crop when the
        // photo is shown cropped.
        let (mut x, mut y) = to_display((x, y), &develop);
        let crop = develop.crop;
        let cropped = crop.rect != (0.0, 0.0, 1.0, 1.0) && !self.live_preview.shown_crop_full();
        if cropped {
            // A tilted or perspective-corrected crop has no simple mapping.
            if crop.angle != 0.0 || crop.vertical != 0.0 || crop.horizontal != 0.0 {
                return None;
            }
            let (cx, cy, cw, ch) = crop.rect;
            x = (x - cx) / cw.max(1e-3);
            y = (y - cy) / ch.max(1e-3);
        }
        if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
            return None;
        }

        let (w, h) = (f32::from(photo.size.width), f32::from(photo.size.height));
        let side = (w.max(h) * SIDE).max(MIN_SIDE);
        let (hx, hy) = (side / 2.0 / w.max(1.0), side / 2.0 / h.max(1.0));
        let square = vec![vec![
            (x - hx, y - hy),
            (x + hx, y - hy),
            (x + hx, y + hy),
            (x - hx, y + hy),
            (x - hx, y - hy),
        ]];
        Some(crate::develop::grid_overlay::overlay_canvas(
            square,
            photo,
            self.develop_ui.zoom.viewer_bounds.get(),
            hsla(130.0 / 360.0, 0.85, 0.6, 1.0),
        ))
    }
}
