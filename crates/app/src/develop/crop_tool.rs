//! The Crop tool (R): drag the box on the whole photo.
//!
//! While it is picked the viewer shows the photo uncropped (but with its
//! straightening and perspective applied, since the crop rectangle is
//! measured on that canvas), the outside of the box dimmed. Drag a corner
//! or edge to resize it, the inside to move it, or anywhere outside to draw
//! a new one. An aspect lock from the Crop section is kept. Thirds are
//! drawn inside the box, or the composition guide chosen in the View menu.
//! Drag just outside a corner to turn the photo under the box (straighten);
//! X flips the box between landscape and portrait. Esc, the tool's button,
//! or a double-click inside the box puts it down.

use gpui::{
    canvas, div, fill, point, prelude::*, px, AnyElement, Bounds, Context, MouseButton,
    MouseDownEvent, PathBuilder, Pixels, Point,
};
use rustyedit_core::{drag_crop_rect, CropHandle, MAX_STRAIGHTEN};

use crate::{
    actions::ToolCrop,
    workspace::{Mode, Workspace},
};

/// How close, in screen pixels, the pointer must be to grab a handle.
const HANDLE_REACH: f32 = 11.0;
/// How far a new box must be dragged before it replaces the old one.
const NEW_BOX_THRESHOLD: f32 = 0.004;
/// How far outside a corner (screen pixels) a drag turns the photo rather
/// than drawing a new box.
const ROTATE_REACH: f32 = 46.0;

/// What the pointer is over on the crop overlay.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum CropHit {
    Handle(CropHandle),
    Move,
    /// Just outside a corner: a drag turns the photo.
    Rotate,
    /// Elsewhere on the canvas: a drag draws a new box.
    New,
}

/// A drag in progress.
#[derive(Clone, Copy)]
pub(crate) enum CropDrag {
    /// Resizing, moving or drawing the box: the handle, the box when the
    /// drag started, and where on the canvas (fractions) the pointer was.
    Box {
        handle: CropHandle,
        start: (f32, f32, f32, f32),
        at: (f32, f32),
    },
    /// Turning the photo about the box's middle (screen pixels): where the
    /// pointer's angle started, and the straightening then.
    Rotate {
        centre: (f32, f32),
        from: f32,
        angle: f32,
    },
}

/// The straightening after the pointer, which was at `from` degrees round
/// `centre` when the drag began, has moved to `to` (screen pixels). A turn
/// clockwise on screen is a positive angle.
fn rotated_angle(start_angle: f32, centre: (f32, f32), from: f32, to: (f32, f32)) -> f32 {
    let now = (to.1 - centre.1).atan2(to.0 - centre.0).to_degrees();
    let mut delta = (now - from).rem_euclid(360.0);
    if delta > 180.0 {
        delta -= 360.0;
    }
    (start_angle + delta).clamp(-MAX_STRAIGHTEN, MAX_STRAIGHTEN)
}

/// How far (degrees, turning the photo clockwise) a line drawn from `a`
/// to `b` on screen has to be turned to lie along the nearest axis, the
/// frame's edges: horizontal for a shallow line, vertical for a steep one.
/// Negative when it leans clockwise already (so the photo turns back).
fn alignment_turn(a: (f32, f32), b: (f32, f32)) -> f32 {
    let theta = (b.1 - a.1).atan2(b.0 - a.0).to_degrees();
    // How far clockwise of an axis the line is, -45..45.
    let off = (theta + 45.0).rem_euclid(90.0) - 45.0;
    -off
}

impl Workspace {
    /// Mouse released: a line drawn with Align turns the photo until that
    /// line lies along the frame.
    pub(crate) fn finish_crop_ruler(&mut self, cx: &mut Context<Self>) {
        let Some((a, b)) = self.develop_ui.crop_ruler.take() else {
            return;
        };
        cx.notify();
        // Too short a line says nothing about the angle.
        if (b.0 - a.0).hypot(b.1 - a.1) < 24.0 {
            return;
        }
        let turn = alignment_turn(a, b);
        let Some(current) = self.catalog.selected_asset().map(|a| a.develop.crop.angle) else {
            return;
        };
        let target = current + turn;
        if target.abs() > MAX_STRAIGHTEN {
            self.status_message = Some(
                format!("That line is more than {MAX_STRAIGHTEN:.0}\u{b0} off the frame; draw it along something nearly level.").into(),
            );
            return;
        }
        self.develop_ui.crop_align = false;
        self.edit_develop(
            "Align to a line",
            |d| d.crop.angle = target,
            |d| (String::new(), format!("{:+.1}\u{b0}", d.crop.angle)),
            cx,
        );
    }

    pub fn tool_crop(&mut self, _: &ToolCrop, _w: &mut gpui::Window, cx: &mut Context<Self>) {
        if self.mode == Mode::Develop {
            self.toggle_photo_tool(super::tool_strip::PhotoTool::Crop, cx);
        }
    }

    /// X: turns the box between landscape and portrait.
    pub(crate) fn flip_crop_orientation(&mut self, cx: &mut Context<Self>) {
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        let (w, h) = (asset.width.max(1) as f32, asset.height.max(1) as f32);
        self.edit_develop(
            "Crop orientation",
            |d| d.crop = d.crop.with_flipped_orientation(w, h),
            |_| (String::new(), "flipped".to_string()),
            cx,
        );
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
        let theme = self.theme;
        // Where the pointer is just outside a corner (or turning), a turn
        // symbol stands in for the system cursor.
        let turning = matches!(self.develop_ui.crop_drag, Some(CropDrag::Rotate { .. }))
            || (self.develop_ui.crop_drag.is_none() && self.develop_ui.crop_hover == Some(CropHit::Rotate));
        let turn_glyph = turning.then(|| {
            let at = self.develop_ui.crop_pointer;
            div()
                .absolute()
                .left(at.x - gpui::px(11.0))
                .top(at.y - gpui::px(11.0))
                .child(
                    div()
                        .size(gpui::px(22.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(gpui::black().opacity(0.6))
                        .child(rustyedit_ui::icon(rustyedit_ui::Icon::Reset, 14.0, gpui::white())),
                )
        });
        // Align: a level with a crosshair at its centre stands in for the
        // pointer, so the line can be set on the horizon exactly.
        let level_glyph = (self.develop_ui.crop_align
            && (self.develop_ui.crop_hover.is_some() || self.develop_ui.crop_ruler.is_some()))
        .then(|| {
            let at = self.develop_ui.crop_pointer;
            let line = |w: f32, h: f32, x: f32, y: f32, c: gpui::Hsla| {
                div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h)).bg(c)
            };
            let (dark, light, gold) = (gpui::black().opacity(0.6), gpui::white(), gpui::rgb(0xffd23f));
            div()
                .absolute()
                .left(at.x - px(18.0))
                .top(at.y - px(18.0))
                .size(px(36.0))
                // Crosshair, with a gap at the middle.
                .child(line(36.0, 3.0, 0.0, 16.5, dark))
                .child(line(3.0, 36.0, 16.5, 0.0, dark))
                .child(line(14.0, 1.0, 0.0, 17.0, light))
                .child(line(14.0, 1.0, 22.0, 17.0, light))
                .child(line(1.0, 14.0, 17.0, 0.0, light))
                .child(line(1.0, 14.0, 17.0, 22.0, light))
                // The level: a tube with its bubble centred.
                .child(
                    div()
                        .absolute()
                        .left(px(22.0))
                        .top(px(-6.0))
                        .w(px(26.0))
                        .h(px(10.0))
                        .rounded_full()
                        .bg(gpui::black().opacity(0.7))
                        .border_1()
                        .border_color(light)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(div().size(px(5.0)).rounded_full().bg(gold)),
                )
        });
        let (hint_bg, hint_border, hint_text) = (theme.surface.opacity(0.9), theme.border, theme.text);
        let ruler = self.develop_ui.crop_ruler;
        let aligning = self.develop_ui.crop_align;
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
                .cursor(self.crop_cursor())
                .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _w, cx| {
                    // A drag in progress is followed elsewhere.
                    if this.develop_ui.crop_drag.is_none() {
                        this.crop_hover(event.position, cx);
                    }
                }))
                .children(turn_glyph)
                .children(level_glyph)
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
                                // The Align line, with the level it will make.
                                if let Some((a, b)) = ruler {
                                    stroke(window, &[a, b], false, 4.0, gpui::black().opacity(0.5));
                                    stroke(window, &[a, b], false, 2.0, gpui::rgb(0xffd23f).into());
                                    let target = {
                                        let turn = alignment_turn(a, b);
                                        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                                        let (c, s) = (turn.to_radians().cos(), turn.to_radians().sin());
                                        (a.0 + dx * c - dy * s, a.1 + dx * s + dy * c)
                                    };
                                    stroke(window, &[a, target], false, 1.0, gpui::white().opacity(0.8));
                                    for end in [a, b] {
                                        window.paint_quad(fill(rect(end.0 - 3.0, end.1 - 3.0, 6.0, 6.0), gpui::rgb(0xffd23f)));
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
                .child(
                    div()
                        .absolute()
                        .top(gpui::px(8.0))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .px_2()
                                .h(gpui::px(22.0))
                                .flex()
                                .items_center()
                                .rounded_md()
                                .bg(hint_bg)
                                .border_1()
                                .border_color(hint_border)
                                .text_size(gpui::px(10.5))
                                .text_color(hint_text)
                                .child(if aligning {
                                    "Align: drag a line along the horizon or an edge that should be level or upright"
                                } else {
                                    "Drag a corner or edge to resize \u{00B7} just outside a corner to rotate \u{00B7} X flips \u{00B7} double-click to finish"
                                }),
                        ),
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

    /// What the pointer is over: a handle, the inside of the box, just
    /// outside a corner (turns the photo), or the rest of the canvas (draws
    /// a new box).
    fn crop_hit(position: Point<Pixels>, photo: Bounds<Pixels>, rect: (f32, f32, f32, f32)) -> CropHit {
        let (pw, ph) = (f32::from(photo.size.width), f32::from(photo.size.height));
        let (px_, py_) = (f32::from(position.x), f32::from(position.y));
        let screen = |fx: f32, fy: f32| {
            (
                f32::from(photo.origin.x) + fx * pw,
                f32::from(photo.origin.y) + fy * ph,
            )
        };
        let (x, y, w, h) = rect;
        // Corners win over edges when the box is small.
        let handle = CropHandle::EDGES
            .iter()
            .copied()
            .filter(|handle| {
                let (fx, fy) = handle.at();
                let (sx, sy) = screen(x + fx * w, y + fy * h);
                (px_ - sx).abs() <= HANDLE_REACH && (py_ - sy).abs() <= HANDLE_REACH
            })
            .min_by_key(|handle| {
                let (fx, fy) = handle.at();
                (fx == 0.5 || fy == 0.5) as u8
            });
        if let Some(handle) = handle {
            return CropHit::Handle(handle);
        }
        let at = Self::crop_point(position, photo);
        if at.0 >= x && at.0 <= x + w && at.1 >= y && at.1 <= y + h {
            return CropHit::Move;
        }
        let near_corner = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].iter().any(|&(fx, fy)| {
            let (sx, sy) = screen(x + fx * w, y + fy * h);
            (px_ - sx).hypot(py_ - sy) <= ROTATE_REACH
        });
        if near_corner {
            CropHit::Rotate
        } else {
            CropHit::New
        }
    }

    /// Tracks what the pointer is over, so the cursor can say what a drag
    /// would do. Redraws only when that changes (or, while turning, as the
    /// turn glyph follows the pointer).
    fn crop_hover(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some((photo, rect)) = self.crop_space() else {
            return;
        };
        let hit = photo
            .contains(&position)
            .then(|| Self::crop_hit(position, photo, rect));
        // Align's level cursor follows the pointer, so every move redraws.
        let moved = (hit == Some(CropHit::Rotate) || self.develop_ui.crop_align)
            && self.develop_ui.crop_pointer != position;
        self.develop_ui.crop_pointer = position;
        if hit != self.develop_ui.crop_hover || moved {
            self.develop_ui.crop_hover = hit;
            cx.notify();
        }
    }

    /// The cursor for the crop overlay: by what a drag in progress is, or
    /// else by what the pointer is over.
    fn crop_cursor(&self) -> gpui::CursorStyle {
        use gpui::CursorStyle as C;
        if self.develop_ui.crop_align {
            // The level glyph follows the pointer instead (see the
            // overlay); off the photo the plain cursor shows.
            return if self.develop_ui.crop_hover.is_some() || self.develop_ui.crop_ruler.is_some() {
                C::None
            } else {
                C::Crosshair
            };
        }
        let hit = match self.develop_ui.crop_drag {
            Some(CropDrag::Rotate { .. }) => Some(CropHit::Rotate),
            Some(CropDrag::Box { handle: CropHandle::Move, .. }) => {
                return rustyedit_ui::GRABBING_CURSOR;
            }
            Some(CropDrag::Box { handle: CropHandle::New, .. }) => Some(CropHit::New),
            Some(CropDrag::Box { handle, .. }) => Some(CropHit::Handle(handle)),
            None => self.develop_ui.crop_hover,
        };
        match hit {
            Some(CropHit::Handle(CropHandle::TopLeft | CropHandle::BottomRight)) => C::ResizeUpLeftDownRight,
            Some(CropHit::Handle(CropHandle::TopRight | CropHandle::BottomLeft)) => C::ResizeUpRightDownLeft,
            Some(CropHit::Handle(CropHandle::Top | CropHandle::Bottom)) => C::ResizeUpDown,
            Some(CropHit::Handle(CropHandle::Left | CropHandle::Right)) => C::ResizeLeftRight,
            Some(CropHit::Handle(_)) => C::Crosshair,
            Some(CropHit::Move) => rustyedit_ui::GRAB_CURSOR,
            // A turn glyph follows the pointer instead (see the overlay).
            Some(CropHit::Rotate) => C::None,
            Some(CropHit::New) | None => C::Crosshair,
        }
    }

    fn crop_pointer_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some((photo, rect)) = self.crop_space() else {
            return;
        };
        if !photo.contains(&event.position) {
            return;
        }
        if self.develop_ui.crop_align {
            // Align: this press starts a line along what should be level.
            let p = (f32::from(event.position.x), f32::from(event.position.y));
            self.develop_ui.crop_ruler = Some((p, p));
            cx.notify();
            return;
        }
        let at = Self::crop_point(event.position, photo);
        let (pw, ph) = (f32::from(photo.size.width), f32::from(photo.size.height));
        let (x, y, w, h) = rect;
        let handle = match Self::crop_hit(event.position, photo, rect) {
            CropHit::Handle(handle) => handle,
            CropHit::Move => CropHandle::Move,
            CropHit::New => CropHandle::New,
            CropHit::Rotate => {
                // Turn the photo under the box, about the box's middle.
                let centre = (
                    f32::from(photo.origin.x) + (x + w / 2.0) * pw,
                    f32::from(photo.origin.y) + (y + h / 2.0) * ph,
                );
                let from = (f32::from(event.position.y) - centre.1)
                    .atan2(f32::from(event.position.x) - centre.0)
                    .to_degrees();
                let angle = self
                    .catalog
                    .selected_asset()
                    .map_or(0.0, |a| a.develop.crop.angle);
                self.develop_ui.crop_drag = Some(CropDrag::Rotate { centre, from, angle });
                cx.notify();
                return;
            }
        };
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
        self.develop_ui.crop_drag = Some(CropDrag::Box { handle, start, at });
        cx.notify();
    }

    /// Enter: done cropping.
    pub(crate) fn finish_crop(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.crop_in_view() {
            return false;
        }
        self.toggle_photo_tool(super::tool_strip::PhotoTool::Crop, cx);
        true
    }

    /// Follows a drag of the crop box; `true` while one is in progress.
    pub(super) fn crop_drag_move(
        &mut self,
        position: Point<Pixels>,
        dragging: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some((a, _)) = self.develop_ui.crop_ruler {
            if dragging {
                self.develop_ui.crop_pointer = position;
                self.develop_ui.crop_ruler = Some((a, (f32::from(position.x), f32::from(position.y))));
                cx.notify();
            } else {
                self.finish_crop_ruler(cx);
            }
            return true;
        }
        let Some(drag) = self.develop_ui.crop_drag else {
            return false;
        };
        if !dragging {
            self.develop_ui.crop_drag = None;
            cx.notify();
            return true;
        }
        self.develop_ui.crop_pointer = position;
        if let CropDrag::Rotate { centre, from, angle } = drag {
            let turned = rotated_angle(angle, centre, from, (f32::from(position.x), f32::from(position.y)));
            self.edit_develop(
                "Straighten",
                |d| d.crop.angle = turned,
                |d| (String::new(), format!("{:+.1}\u{b0}", d.crop.angle)),
                cx,
            );
            return true;
        }
        let CropDrag::Box { handle, start, at } = drag else {
            return true;
        };
        let Some((photo, _)) = self.crop_space() else {
            return true;
        };
        let Some(asset) = self.catalog.selected_asset() else {
            return true;
        };
        let now = Self::crop_point(position, photo);
        let (du, dv) = (now.0 - at.0, now.1 - at.1);
        // A click outside the box without dragging leaves it be.
        if handle == CropHandle::New && du.abs() < NEW_BOX_THRESHOLD && dv.abs() < NEW_BOX_THRESHOLD {
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
        let rect = drag_crop_rect(start, handle, du, dv, ratio);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragging_clockwise_round_the_box_turns_the_photo_clockwise() {
        let centre = (100.0, 100.0);
        // Start to the right of the centre (0°), move down (90° on screen).
        let angle = rotated_angle(0.0, centre, 0.0, (100.0, 200.0));
        assert!((angle - 45.0).abs() < 1e-3 || angle == MAX_STRAIGHTEN, "{angle}");
        // A small step is a small angle, and adds to where it was.
        let angle = rotated_angle(2.0, centre, 0.0, (200.0, 105.0));
        assert!(angle > 2.0 && angle < 5.0, "{angle}");
        let back = rotated_angle(2.0, centre, 0.0, (200.0, 95.0));
        assert!(back < 2.0 && back > -1.0, "{back}");
    }

    #[test]
    fn the_turn_wraps_the_short_way_and_stops_at_the_limit() {
        let centre = (0.0, 0.0);
        // From just above the left side (179°) to just below it (-179°) is
        // 2° the short way, not 358°.
        let from = 179.0f32;
        let to = ((-179.0f32).to_radians().cos() * 100.0, (-179.0f32).to_radians().sin() * 100.0);
        let angle = rotated_angle(0.0, centre, from, to);
        assert!((angle - 2.0).abs() < 0.01, "{angle}");
        assert_eq!(rotated_angle(40.0, centre, 0.0, (0.0, 100.0)), MAX_STRAIGHTEN);
        assert_eq!(rotated_angle(-40.0, centre, 0.0, (0.0, -100.0)), -MAX_STRAIGHTEN);
    }
}

#[cfg(test)]
mod align_tests {
    use super::alignment_turn;

    #[test]
    fn a_line_is_turned_onto_the_nearest_axis() {
        // Level already.
        assert!(alignment_turn((0.0, 0.0), (100.0, 0.0)).abs() < 1e-3);
        // Sloping down to the right by about 5.7 degrees: turn back.
        let turn = alignment_turn((0.0, 0.0), (100.0, 10.0));
        assert!((turn + 5.71).abs() < 0.05, "{turn}");
        // Drawn right to left, it's the same line.
        let back = alignment_turn((100.0, 10.0), (0.0, 0.0));
        assert!((back - turn).abs() < 1e-3);
        // A leaning upright turns onto the vertical.
        let upright = alignment_turn((0.0, 0.0), (10.0, 100.0));
        assert!(upright > 0.0 && upright < 6.0, "{upright}");
    }
}
