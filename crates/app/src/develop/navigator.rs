//! The Navigator: a small copy of the photo with a frame around the part
//! the 100% view shows. Clicking or dragging on it moves that view; doing
//! so while fitted zooms to 100% there.

use gpui::{
    canvas, div, img, prelude::*, px, AnyElement, Context, MouseButton, MouseDownEvent,
    ObjectFit, Pixels, Point,
};

use crate::workspace::Workspace;

/// Tallest the Navigator's picture gets, so portrait photos don't push the
/// other tools off screen.
const MAX_HEIGHT: f32 = 200.0;
/// Horizontal room the dock's padding takes from its width.
const DOCK_PADDING: f32 = 28.0;

impl Workspace {
    pub(super) fn navigator_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let (Some(source), Some((w, h))) = (self.develop_image_source(), self.develop_image_size())
        else {
            return vec![div()
                .py_2()
                .text_xs()
                .text_color(theme.text_faint)
                .child("No photo")
                .into_any_element()];
        };

        // Size the picture to the photo's proportions so the frame maps
        // straight onto it.
        let max_width = self.develop_tools_width() - DOCK_PADDING;
        let mut width = max_width;
        let mut height = width * h / w;
        if height > MAX_HEIGHT {
            height = MAX_HEIGHT;
            width = height * w / h;
        }

        let bounds = self.develop_ui.navigator_bounds.clone();
        let frame = self.develop_visible_region().map(|(left, top, fw, fh)| {
            div()
                .absolute()
                .left(px(left * width))
                .top(px(top * height))
                .w(px(fw * width))
                .h(px(fh * height))
                .border_2()
                .border_color(theme.accent)
                .bg(theme.accent.opacity(0.08))
        });

        let picture = div()
            .id("develop-navigator")
            .relative()
            .w(px(width))
            .h(px(height))
            .mx_auto()
            .overflow_hidden()
            .rounded_sm()
            .cursor_crosshair()
            .child(img(source).object_fit(ObjectFit::Fill).size_full())
            .children(frame)
            // Records where the picture was painted, to map clicks onto it.
            .child(
                canvas(
                    move |b, _, _| bounds.set(b),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _w, cx| {
                    this.develop_ui.navigator_dragging = true;
                    this.pan_navigator_to(event.position, cx);
                }),
            );

        let caption = div()
            .pt_1()
            .text_size(px(9.5))
            .text_color(theme.text_faint)
            .text_center()
            .child(if self.zoom_fit {
                "Fit \u{00B7} click to zoom to 100%"
            } else {
                "100% \u{00B7} drag to move"
            });

        vec![div()
            .flex()
            .flex_col()
            .py_2()
            .child(picture)
            .child(caption)
            .into_any_element()]
    }

    /// Centres the 100% view on the photo point under `position`, zooming
    /// in first if the photo is fitted.
    pub(super) fn pan_navigator_to(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let bounds = self.develop_ui.navigator_bounds.get();
        let (bw, bh) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        if bw <= 0.0 || bh <= 0.0 {
            return;
        }
        let fx = (f32::from(position.x - bounds.origin.x) / bw).clamp(0.0, 1.0);
        let fy = (f32::from(position.y - bounds.origin.y) / bh).clamp(0.0, 1.0);
        if self.zoom_fit {
            self.set_develop_zoom_fit(false, cx);
            self.develop_ui.center_on.set(Some((fx, fy)));
        } else {
            self.scroll_develop_to(fx, fy);
        }
        cx.notify();
    }
}
