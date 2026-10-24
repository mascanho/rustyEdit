//! The Crop tool (R): drag the box on the whole photo.
//!
//! While it is picked the viewer shows the photo uncropped (but with its
//! straightening and perspective applied, since the crop rectangle is
//! measured on that canvas), the outside of the box dimmed. Drag a corner
//! or edge to resize it, the inside to move it, or anywhere outside to draw
//! a new one. An aspect lock from the Crop section is kept. Thirds are
//! drawn inside the box, or the composition guide chosen in the View menu.
//! Esc, the tool's button, or a double-click inside the box puts it down.

use gpui::{
    canvas, div, fill, point, prelude::*, px, AnyElement, Bounds, Context, MouseButton,
    MouseDownEvent, PathBuilder, Pixels, Point,
};
use rustyedit_core::{drag_crop_rect, CropHandle};

use crate::{
    actions::ToolCrop,
    workspace::{Mode, Workspace},
};

/// How close, in screen pixels, the pointer must be to grab a handle.
const HANDLE_REACH: f32 = 11.0;
/// How far a new box must be dragged before it replaces the old one.
const NEW_BOX_THRESHOLD: f32 = 0.004;

/// A drag in progress.
#[derive(Clone, Copy)]
pub(crate) struct CropDrag {
    handle: CropHandle,
    /// The box when the drag started, and where on the canvas (fractions)
    /// the pointer was.
    start: (f32, f32, f32, f32),
    at: (f32, f32),
}

impl Workspace {
    pub fn tool_crop(&mut self, _: &ToolCrop, _w: &mut gpui::Window, cx: &mut Context<Self>) {
        if self.mode == Mode::Develop {
            self.toggle_photo_tool(super::tool_strip::PhotoTool::Crop, cx);
        }
    }

    /// Whether the viewer should show the whole canvas for cropping.
    pub(crate) fn crop_in_view(&self) -> bool {
        self.mode == Mode::Develop
            && self.develop_ui.crop_tool
            && self.catalog.selected_asset().is_some()
    }

    /// The photo's place on screen and the box in it (fractions), once the
    /// viewer is showing the whole canvas.
    fn crop_space(&self) -> Option<(Bounds<Pixels>, (f32, f32, f32, f32))> {
        if !self.crop_in_view() || !self.live_preview.shown_crop_full() {
            return None;
        }
        let asset = self.catalog.selected_asset()?;
        let rect = self.develop_ui.zoom.photo_rect(self.develop_image_size()?)?;
        Some((rect, asset.develop.crop.rect))
    }

    pub(super) fn render_crop_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.develop_ui.zoom.space_held {
            return None;
        }
        let (photo, (cx0, cy0, cw, ch)) = self.crop_space()?;
        let clip = self.develop_ui.zoom.viewer_bounds.get();
        let thirds = self.develop_ui.grid_overlay == super::grid_overlay::GridOverlay::Off;
        let (pw, ph) = (f32::from(photo.size.width), f32::from(photo.size.height));
        // The box on screen.
        let left = f32::from(photo.origin.x) + cx0 * pw;
        let top = f32::from(photo.origin.y) + cy0 * ph;
        let (width, height) = (cw * pw, ch * ph);

        Some(
            div()
                .id("crop-overlay")
                .absolute()
                .inset_0()
                .cursor_crosshair()
                .child(
                    canvas(
                        |_, _, _| {},
                        move |_, _, window, _| {
                            let rect = |x: f32, y: f32, w: f32, h: f32| Bounds {
                                origin: point(px(x), px(y)),
                                size: gpui::size(px(w.max(0.0)), px(h.max(0.0))),
                            };
                            let (px0, py0) = (f32::from(photo.origin.x), f32::from(photo.origin.y));
                            window.with_content_mask(Some(gpui::ContentMask { bounds: clip }), |window| {
                                // Dim what's cropped away.
                                let dim = gpui::black().opacity(0.58);
                                window.paint_quad(fill(rect(px0, py0, pw, top - py0), dim));
                                window.paint_quad(fill(rect(px0, top + height, pw, py0 + ph - top - height), dim));
                                window.paint_quad(fill(rect(px0, top, left - px0, height), dim));
                                window.paint_quad(fill(rect(left + width, top, px0 + pw - left - width, height), dim));
                                let stroke = |window: &mut gpui::Window, points: &[(f32, f32)], closed: bool, w: f32, c: gpui::Hsla| {
                                    let pts: Vec<Point<Pixels>> =
                                        points.iter().map(|&(x, y)| point(px(x), px(y))).collect();
                                    let mut path = PathBuilder::stroke(px(w));
                                    path.add_polygon(&pts, closed);
                                    if let Ok(path) = path.build() {
                                        window.paint_path(path, c);
                                    }
                                };
                                let outline = [
                                    (left, top),
                                    (left + width, top),
                                    (left + width, top + height),
                                    (left, top + height),
                                ];
                                stroke(window, &outline, true, 3.0, gpui::black().opacity(0.5));
                                stroke(window, &outline, true, 1.5, gpui::white());
                                if thirds {
                                    for i in 1..3 {
                                        let f = i as f32 / 3.0;
                                        for c in [(gpui::black().opacity(0.35), 3.0), (gpui::white().opacity(0.55), 1.0)] {
                                            stroke(window, &[(left + width * f, top), (left + width * f, top + height)], false, c.1, c.0);
                                            stroke(window, &[(left, top + height * f), (left + width, top + height * f)], false, c.1, c.0);
                                        }
                                    }
                                }
                                // The handles.
                                for handle in CropHandle::EDGES {
                                    let (fx, fy) = handle.at();
                                    let (x, y) = (left + fx * width, top + fy * height);
                                    let corner = fx != 0.5 && fy != 0.5;
                                    let size = if corner { 9.0 } else { 7.0 };
                                    window.paint_quad(fill(
                                        rect(x - size / 2.0 - 1.0, y - size / 2.0 - 1.0, size + 2.0, size + 2.0),
                                        gpui::black().opacity(0.6),
                                    ));
                                    window.paint_quad(fill(
                                        rect(x - size / 2.0, y - size / 2.0, size, size),
                                        gpui::white(),
                                    ));
                                }
                            });
                        },
                    )
                    .absolute()
                    .size_full(),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        this.crop_pointer_down(event, cx);
                    }),
                )
                .into_any_element(),
        )
    }

    /// The photo-space point (fractions of the canvas) under `position`.
    fn crop_point(position: Point<Pixels>, photo: Bounds<Pixels>) -> (f32, f32) {
        let x = f32::from(position.x - photo.origin.x) / f32::from(photo.size.width).max(1.0);
        let y = f32::from(position.y - photo.origin.y) / f32::from(photo.size.height).max(1.0);
        (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0))
    }

    fn crop_pointer_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some((photo, rect)) = self.crop_space() else {
            return;
        };
        if !photo.contains(&event.position) {
            return;
        }
        let at = Self::crop_point(event.position, photo);
        let (pw, ph) = (f32::from(photo.size.width), f32::from(photo.size.height));
        let near = |hx: f32, hy: f32| {
            let sx = f32::from(photo.origin.x) + hx * pw;
            let sy = f32::from(photo.origin.y) + hy * ph;
            (f32::from(event.position.x) - sx).abs() <= HANDLE_REACH
                && (f32::from(event.position.y) - sy).abs() <= HANDLE_REACH
        };
        // Corners win over edges when the box is small.
        let (x, y, w, h) = rect;
        let handle = CropHandle::EDGES
            .iter()
            .copied()
            .filter(|handle| {
                let (fx, fy) = handle.at();
                near(x + fx * w, y + fy * h)
            })
            .min_by_key(|handle| {
                let (fx, fy) = handle.at();
                // Corners first.
                (fx == 0.5 || fy == 0.5) as u8
            })
            .unwrap_or_else(|| {
                if at.0 >= x && at.0 <= x + w && at.1 >= y && at.1 <= y + h {
                    CropHandle::Move
                } else {
                    CropHandle::New
                }
            });
        if handle == CropHandle::Move && event.click_count >= 2 {
            // Done: put the tool down.
            self.toggle_photo_tool(super::tool_strip::PhotoTool::Crop, cx);
            return;
        }
        let start = if handle == CropHandle::New {
            (at.0, at.1, 0.0, 0.0)
        } else {
            rect
        };
        self.develop_ui.crop_drag = Some(CropDrag { handle, start, at });
        cx.notify();
    }

    /// Follows a drag of the crop box; `true` while one is in progress.
    pub(super) fn crop_drag_move(
        &mut self,
        position: Point<Pixels>,
        dragging: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.develop_ui.crop_drag else {
            return false;
        };
        if !dragging {
            self.develop_ui.crop_drag = None;
            cx.notify();
            return true;
        }
        let Some((photo, _)) = self.crop_space() else {
            return true;
        };
        let Some(asset) = self.catalog.selected_asset() else {
            return true;
        };
        let now = Self::crop_point(position, photo);
        let (du, dv) = (now.0 - drag.at.0, now.1 - drag.at.1);
        // A click outside the box without dragging leaves it be.
        if drag.handle == CropHandle::New && du.abs() < NEW_BOX_THRESHOLD && dv.abs() < NEW_BOX_THRESHOLD {
            return true;
        }
        let (w, h) = (asset.width.max(1) as f32, asset.height.max(1) as f32);
        // The locked shape, as the box's width over its height in canvas
        // fractions.
        let ratio = asset
            .develop
            .crop
            .aspect
            .ratio(w, h)
            .map(|shape| shape / (w / h));
        let rect = drag_crop_rect(drag.start, drag.handle, du, dv, ratio);
        self.edit_develop(
            "Crop",
            |d| d.crop.rect = rect,
            |d| {
                let (_, _, rw, rh) = d.crop.rect;
                (String::new(), format!("{:.0} \u{d7} {:.0} px", rw * w, rh * h))
            },
            cx,
        );
        true
    }
}
