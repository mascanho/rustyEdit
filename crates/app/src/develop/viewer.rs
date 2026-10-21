//! The photo in Develop: fitted to the window, or at 100% in a scrollable
//! view that pans by dragging (or from the Navigator).

use gpui::{
    div, img, point, prelude::*, px, AnyElement, Context, ImageSource, MouseButton,
    MouseDownEvent, MouseMoveEvent, ObjectFit, Window,
};

use crate::workspace::Workspace;

impl Workspace {
    pub(super) fn render_develop_viewer(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let content = match (self.develop_image_source(), self.zoom_fit) {
            (None, _) => div()
                .text_color(theme.text_muted)
                .child("Loading…")
                .into_any_element(),
            (Some(source), true) => img(source)
                .object_fit(ObjectFit::Contain)
                .size_full()
                .into_any_element(),
            (Some(source), false) => self.render_zoomed_develop(source, window, cx),
        };

        div()
            .id("develop-viewer")
            .on_mouse_down(
                MouseButton::Right,
                Self::right_click_listener(cx.entity(), self.catalog.selected),
            )
            .flex_1()
            .min_w(px(0.0))
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.background)
            .child(content)
            .into_any_element()
    }

    /// The live preview once one has been rendered, and the photo's own
    /// file until then.
    pub(super) fn develop_image_source(&self) -> Option<ImageSource> {
        match self.live_preview.image() {
            Some(rendered) => Some(rendered.into()),
            None => self
                .catalog
                .selected_asset()
                .map(|a| a.display_source().to_path_buf().into()),
        }
    }

    /// The photo's size in image pixels as Develop shows it: the rendered
    /// preview's, or (before the first render) the original's, capped to
    /// the working-preview size it will be rendered at.
    pub(super) fn develop_image_size(&self) -> Option<(f32, f32)> {
        if let Some(size) = self.live_preview.size() {
            return Some(size);
        }
        let asset = self.catalog.selected_asset()?;
        if asset.width == 0 || asset.height == 0 {
            return None;
        }
        let (w, h) = (asset.width as f32, asset.height as f32);
        let max_edge = self.settings.develop.preview_quality.max_edge() as f32;
        let scale = (max_edge / w.max(h)).min(1.0);
        Some((w * scale, h * scale))
    }

    /// Switches between fit and 100%. Going to 100% centres the photo.
    pub(crate) fn set_develop_zoom_fit(&mut self, fit: bool, cx: &mut Context<Self>) {
        if self.zoom_fit == fit {
            return;
        }
        self.zoom_fit = fit;
        if !fit {
            self.develop_ui.center_on.set(Some((0.5, 0.5)));
        }
        cx.notify();
    }

    fn render_zoomed_develop(
        &self,
        source: ImageSource,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((w, h)) = self.develop_image_size() else {
            return div().into_any_element();
        };
        // Logical pixels, so a Retina display still shows 1 image pixel
        // per physical pixel at 100%.
        let (w, h) = (w / window.scale_factor(), h / window.scale_factor());
        self.develop_ui.zoomed_size.set((w, h));

        // Centre where asked once the viewport's size is known.
        if let Some((fx, fy)) = self.develop_ui.center_on.get() {
            if self.develop_ui.viewer_scroll.bounds().size.width > px(0.0) {
                self.scroll_develop_to(fx, fy);
                self.develop_ui.center_on.set(None);
            }
        }

        div()
            .id("develop-zoom")
            .size_full()
            .overflow_scroll()
            .track_scroll(&self.develop_ui.viewer_scroll)
            .cursor_grab()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _w, _cx| {
                    let offset = this.develop_ui.viewer_scroll.offset();
                    this.develop_ui.pan_drag = Some((event.position, offset));
                }),
            )
            .child(
                div()
                    .min_w_full()
                    .min_h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(img(source).w(px(w)).h(px(h)).flex_shrink_0()),
            )
            .into_any_element()
    }

    /// Scrolls the 100% view so the photo point at (`fx`, `fy`), as
    /// fractions of its width and height, sits in the middle of the view.
    pub(super) fn scroll_develop_to(&self, fx: f32, fy: f32) {
        let scroll = &self.develop_ui.viewer_scroll;
        let (w, h) = self.develop_ui.zoomed_size.get();
        let view = scroll.bounds().size;
        let (vw, vh) = (f32::from(view.width), f32::from(view.height));
        let x = (fx * w - vw / 2.0).clamp(0.0, (w - vw).max(0.0));
        let y = (fy * h - vh / 2.0).clamp(0.0, (h - vh).max(0.0));
        scroll.set_offset(point(px(-x), px(-y)));
    }

    /// The part of the photo the 100% view shows, as fractions
    /// `(left, top, width, height)`; `None` when fitted.
    pub(super) fn develop_visible_region(&self) -> Option<(f32, f32, f32, f32)> {
        if self.zoom_fit {
            return None;
        }
        let (w, h) = self.develop_ui.zoomed_size.get();
        let scroll = &self.develop_ui.viewer_scroll;
        let view = scroll.bounds().size;
        let (vw, vh) = (f32::from(view.width), f32::from(view.height));
        if w <= 0.0 || h <= 0.0 || vw <= 0.0 {
            return None;
        }
        let offset = scroll.offset();
        let (fw, fh) = ((vw / w).min(1.0), (vh / h).min(1.0));
        let left = (-f32::from(offset.x) / w).clamp(0.0, 1.0 - fw);
        let top = (-f32::from(offset.y) / h).clamp(0.0, 1.0 - fh);
        Some((left, top, fw, fh))
    }

    /// Follows a drag on the 100% photo or the Navigator. Returns whether
    /// one was in progress (and so the event is used).
    pub(crate) fn develop_drag_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) -> bool {
        if self.curve_drag_move(event.position, event.dragging(), cx) {
            return true;
        }
        if self.develop_ui.navigator_dragging {
            if event.dragging() {
                self.pan_navigator_to(event.position, cx);
            } else {
                self.develop_ui.navigator_dragging = false;
            }
            return true;
        }
        let Some((start, start_offset)) = self.develop_ui.pan_drag else {
            return false;
        };
        if !event.dragging() {
            self.develop_ui.pan_drag = None;
            return true;
        }
        let scroll = &self.develop_ui.viewer_scroll;
        let max = scroll.max_offset();
        let x = (start_offset.x + (event.position.x - start.x)).clamp(-max.width, px(0.0));
        let y = (start_offset.y + (event.position.y - start.y)).clamp(-max.height, px(0.0));
        scroll.set_offset(point(x, y));
        cx.notify();
        true
    }

    pub(crate) fn end_develop_drag(&mut self) {
        self.develop_ui.pan_drag = None;
        self.develop_ui.navigator_dragging = false;
        self.develop_ui.curve_drag = None;
    }
}
