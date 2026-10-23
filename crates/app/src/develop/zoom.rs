//! Zooming and panning the photo in Develop.
//!
//! - Ctrl + mouse wheel zooms smoothly between Fit and 800%, keeping the
//!   point under the pointer still.
//! - Z toggles Fit and 100%; the Navigator jumps to 100% where clicked.
//! - Dragging, or Space + dragging, pans a zoomed photo; Space + click on
//!   the fitted photo zooms to 100% there and pans from it.
//!
//! A zoom change can't place the photo until the next layout, so it leaves
//! a [`ZoomAnchor`] saying which photo point should land where; the viewer
//! applies it when it renders.

use rustyedit_ui::sp;
use std::{cell::Cell, rc::Rc};

use gpui::{
    point, px, Bounds, Context, KeyDownEvent, KeyUpEvent, Pixels, Point, ScrollHandle, Window,
};

use crate::workspace::{Mode, Workspace};

/// Most the photo can be magnified (800%).
const MAX_ZOOM: f32 = 8.0;
/// How quickly the wheel zooms: the scale multiplies by `e^(delta * this)`.
const WHEEL_SENSITIVITY: f32 = 0.004;

/// A photo point (fractions of its width and height) and where in the
/// viewer it should appear; `None` means the viewer's centre.
#[derive(Clone, Copy)]
pub(crate) struct ZoomAnchor {
    fx: f32,
    fy: f32,
    at: Option<(f32, f32)>,
}

/// The viewer's zoom and pan state.
pub(crate) struct DevelopZoom {
    /// `None` fits the photo to the viewer; `Some(1.0)` is 100%, one photo
    /// pixel per screen pixel.
    pub(crate) scale: Option<f32>,
    pub(crate) anchor: Cell<Option<ZoomAnchor>>,
    /// Scroll position of the zoomed photo.
    pub(crate) scroll: ScrollHandle,
    /// The viewer's bounds, recorded every frame.
    pub(crate) viewer_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The zoomed photo's size in logical pixels, from the last render.
    pub(crate) zoomed_size: Cell<(f32, f32)>,
    /// The display's pixel ratio, from the last render.
    pub(crate) pixel_ratio: Cell<f32>,
    /// A pan in progress: where the pointer was at the last step.
    pub(crate) pan: Option<Point<Pixels>>,
    /// Space is held, so dragging pans.
    pub(crate) space_held: bool,
}

impl Default for DevelopZoom {
    fn default() -> Self {
        Self {
            scale: None,
            anchor: Cell::new(None),
            scroll: ScrollHandle::new(),
            viewer_bounds: Rc::new(Cell::new(Bounds::default())),
            zoomed_size: Cell::new((0.0, 0.0)),
            pixel_ratio: Cell::new(1.0),
            pan: None,
            space_held: false,
        }
    }
}

impl DevelopZoom {
    fn viewer_size(&self) -> (f32, f32) {
        let size = self.viewer_bounds.get().size;
        (f32::from(size.width), f32::from(size.height))
    }

    /// Where the photo's left/top edge sits inside the scrolled content:
    /// centred while it's smaller than the viewer.
    fn inset(view: f32, photo: f32) -> f32 {
        ((view - photo) / 2.0).max(0.0)
    }

    /// Places the zoomed photo so the pending anchor lands where asked.
    /// Call while rendering at the new size.
    pub(crate) fn apply_anchor(&self) {
        let Some(anchor) = self.anchor.get() else {
            return;
        };
        let (vw, vh) = self.viewer_size();
        if vw <= 0.0 || vh <= 0.0 {
            return;
        }
        let (w, h) = self.zoomed_size.get();
        let (lx, ly) = anchor.at.unwrap_or((vw / 2.0, vh / 2.0));
        let x = anchor.fx * w + Self::inset(vw, w) - lx;
        let y = anchor.fy * h + Self::inset(vh, h) - ly;
        self.scroll.set_offset(point(
            px(-x.clamp(0.0, (w - vw).max(0.0))),
            px(-y.clamp(0.0, (h - vh).max(0.0))),
        ));
        self.anchor.set(None);
    }

    /// The part of the photo in view as fractions `(left, top, width,
    /// height)`; `None` when fitted.
    pub(crate) fn visible_region(&self) -> Option<(f32, f32, f32, f32)> {
        self.scale?;
        let (w, h) = self.zoomed_size.get();
        let (vw, vh) = self.viewer_size();
        if w <= 0.0 || h <= 0.0 || vw <= 0.0 {
            return None;
        }
        let offset = self.scroll.offset();
        let (fw, fh) = ((vw / w).min(1.0), (vh / h).min(1.0));
        let left = (-f32::from(offset.x) / w).clamp(0.0, 1.0 - fw);
        let top = (-f32::from(offset.y) / h).clamp(0.0, 1.0 - fh);
        Some((left, top, fw, fh))
    }

    /// Where the photo is drawn on screen, given its size in image pixels:
    /// letterboxed in the viewer when fitted, or scrolled when zoomed.
    pub(crate) fn photo_rect(&self, image: (f32, f32)) -> Option<Bounds<Pixels>> {
        let viewer = self.viewer_bounds.get();
        let (vw, vh) = self.viewer_size();
        if vw <= 0.0 || vh <= 0.0 || image.0 <= 0.0 || image.1 <= 0.0 {
            return None;
        }
        let ((w, h), (x, y)) = match self.scale {
            None => {
                let fit = (vw / image.0).min(vh / image.1);
                let (w, h) = (image.0 * fit, image.1 * fit);
                ((w, h), ((vw - w) / 2.0, (vh - h) / 2.0))
            }
            Some(_) => {
                let (w, h) = self.zoomed_size.get();
                let offset = self.scroll.offset();
                (
                    (w, h),
                    (
                        Self::inset(vw, w) + f32::from(offset.x),
                        Self::inset(vh, h) + f32::from(offset.y),
                    ),
                )
            }
        };
        Some(Bounds::new(
            point(viewer.origin.x + px(x), viewer.origin.y + px(y)),
            gpui::size(px(w), px(h)),
        ))
    }

    /// Centre the photo point (`fx`, `fy`) in the viewer.
    pub(crate) fn centre_on(&self, fx: f32, fy: f32) {
        self.anchor.set(Some(ZoomAnchor { fx, fy, at: None }));
    }
}

impl Workspace {
    /// The scale at which the whole photo fits the viewer.
    pub(crate) fn develop_fit_scale(&self) -> Option<f32> {
        let (w, h) = self.develop_image_size()?;
        let (vw, vh) = self.develop_ui.zoom.viewer_size();
        if vw <= 0.0 || vh <= 0.0 {
            return None;
        }
        let ratio = self.develop_ui.zoom.pixel_ratio.get();
        Some((vw * ratio / w).min(vh * ratio / h))
    }

    /// Ctrl + wheel: zooms by `delta_y` (positive zooms in) around
    /// `pointer`.
    pub(crate) fn zoom_develop_by(
        &mut self,
        delta_y: f32,
        pointer: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(fit) = self.develop_fit_scale() else {
            return;
        };
        let zoom = &self.develop_ui.zoom;
        let current = zoom.scale.unwrap_or(fit);
        let next =
            (current * (delta_y * WHEEL_SENSITIVITY).exp()).clamp(fit.min(MAX_ZOOM), MAX_ZOOM);
        if next <= fit * 1.001 {
            if zoom.scale.is_some() {
                self.develop_ui.zoom.scale = None;
                cx.notify();
            }
            return;
        }

        let anchor = self.develop_anchor_at(pointer, fit);
        self.develop_ui.zoom.scale = Some(next);
        self.develop_ui.zoom.anchor.set(Some(anchor));
        cx.notify();
    }

    /// The photo point under `pointer`, pinned there so zooming keeps it
    /// still. Over a panel instead, the middle of what's in view.
    fn develop_anchor_at(&self, pointer: Point<Pixels>, fit: f32) -> ZoomAnchor {
        let zoom = &self.develop_ui.zoom;
        let bounds = zoom.viewer_bounds.get();
        if !bounds.contains(&pointer) {
            let (fx, fy) = zoom
                .visible_region()
                .map(|(l, t, w, h)| (l + w / 2.0, t + h / 2.0))
                .unwrap_or((0.5, 0.5));
            return ZoomAnchor { fx, fy, at: None };
        }
        let (lx, ly) = (
            f32::from(pointer.x - bounds.origin.x),
            f32::from(pointer.y - bounds.origin.y),
        );
        let (vw, vh) = zoom.viewer_size();
        // The photo's on-screen size and scroll offset right now.
        let ((w, h), offset) = match zoom.scale {
            Some(_) => (zoom.zoomed_size.get(), zoom.scroll.offset()),
            None => {
                let (iw, ih) = self.develop_image_size().unwrap_or((1.0, 1.0));
                let ratio = zoom.pixel_ratio.get();
                ((iw * fit / ratio, ih * fit / ratio), Point::default())
            }
        };
        let fx = (lx - f32::from(offset.x) - DevelopZoom::inset(vw, w)) / w.max(1.0);
        let fy = (ly - f32::from(offset.y) - DevelopZoom::inset(vh, h)) / h.max(1.0);
        ZoomAnchor {
            fx: fx.clamp(0.0, 1.0),
            fy: fy.clamp(0.0, 1.0),
            at: Some((lx, ly)),
        }
    }

    /// Space + click on the fitted photo: zoom to 100% on that spot and
    /// keep dragging to pan from there.
    pub(super) fn grab_fitted_develop(&mut self, pointer: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(fit) = self.develop_fit_scale() else {
            return;
        };
        let anchor = self.develop_anchor_at(pointer, fit);
        let zoom = &mut self.develop_ui.zoom;
        zoom.scale = Some(1.0f32.max(fit * 1.01));
        zoom.anchor.set(Some(anchor));
        zoom.pan = Some(pointer);
        cx.notify();
    }

    /// Sets the zoom (`None` fits), keeping the middle of the view where
    /// it is. Used by the footer slider.
    pub(crate) fn set_develop_scale(&mut self, scale: Option<f32>, cx: &mut Context<Self>) {
        let zoom = &mut self.develop_ui.zoom;
        if scale.is_some() {
            let (fx, fy) = zoom
                .visible_region()
                .map(|(l, t, w, h)| (l + w / 2.0, t + h / 2.0))
                .unwrap_or((0.5, 0.5));
            zoom.centre_on(fx, fy);
        }
        zoom.scale = scale;
        cx.notify();
    }

    /// Z: Fit ↔ 100%, centred.
    pub(crate) fn toggle_develop_zoom(&mut self, cx: &mut Context<Self>) {
        let zoom = &mut self.develop_ui.zoom;
        zoom.scale = match zoom.scale {
            Some(_) => None,
            None => {
                zoom.centre_on(0.5, 0.5);
                Some(1.0)
            }
        };
        cx.notify();
    }

    /// Zooms to 100% centred on the photo point (`fx`, `fy`).
    pub(crate) fn zoom_develop_to_actual(&mut self, fx: f32, fy: f32, cx: &mut Context<Self>) {
        self.develop_ui.zoom.scale = Some(1.0);
        self.develop_ui.zoom.centre_on(fx, fy);
        cx.notify();
    }

    pub(crate) fn develop_key_down(
        &mut self,
        event: &KeyDownEvent,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "space" && self.mode == Mode::Develop && self.text_draft.is_none()
        {
            if !self.develop_ui.zoom.space_held {
                self.develop_ui.zoom.space_held = true;
                cx.notify();
            }
            cx.stop_propagation();
        }
    }

    pub(crate) fn develop_key_up(
        &mut self,
        event: &KeyUpEvent,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "space" && self.develop_ui.zoom.space_held {
            self.develop_ui.zoom.space_held = false;
            cx.notify();
        }
    }

    /// Starts panning the zoomed photo from `position`.
    pub(super) fn start_develop_pan(&mut self, position: Point<Pixels>) {
        self.develop_ui.zoom.pan = Some(position);
    }

    /// Follows a pan; `true` while one is in progress.
    pub(super) fn develop_pan_move(
        &mut self,
        position: Point<Pixels>,
        dragging: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(last) = self.develop_ui.zoom.pan else {
            return false;
        };
        if !dragging {
            self.develop_ui.zoom.pan = None;
            cx.notify();
            return true;
        }
        // Move by each step's delta, so a pan that began as the photo was
        // still being zoomed and placed (Space + click when fitted) follows
        // wherever the zoom put it.
        let zoom = &mut self.develop_ui.zoom;
        let offset = zoom.scroll.offset();
        let max = zoom.scroll.max_offset();
        let x = (offset.x + (position.x - last.x)).clamp(-max.width, sp(0.0));
        let y = (offset.y + (position.y - last.y)).clamp(-max.height, sp(0.0));
        zoom.scroll.set_offset(point(x, y));
        zoom.pan = Some(position);
        cx.notify();
        true
    }

    /// The zoom as shown to the user: "Fit" or a percentage.
    pub(super) fn develop_zoom_label(&self) -> String {
        match self.develop_ui.zoom.scale {
            None => "Fit".to_string(),
            Some(scale) => format!("{:.0}%", scale * 100.0),
        }
    }
}
