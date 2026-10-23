//! The photo in Develop: fitted to the viewer, or zoomed (see `zoom`) in a
//! scrollable view that pans by dragging, with Space held or not.

use rustyedit_ui::sp;
use gpui::{
    canvas, div, img, prelude::*, px, AnyElement, ClickEvent, Context, ImageSource, MouseButton,
    MouseDownEvent, MouseMoveEvent, ObjectFit, Window,
};

use crate::workspace::Workspace;

/// Space around the photo in the Develop viewer.
const PHOTO_PADDING: f32 = 20.0;

impl Workspace {
    pub(super) fn render_develop_viewer(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let zoom = &self.develop_ui.zoom;
        zoom.pixel_ratio.set(window.scale_factor());

        let comparison = self.ba_replacement(zoom.scale.is_none());
        let content = match (self.develop_image_source(), zoom.scale) {
            _ if comparison.is_some() => comparison.unwrap(),
            (None, _) => div()
                .text_color(theme.text_muted)
                .child("Loading…")
                .into_any_element(),
            (Some(source), None) => img(source)
                .object_fit(ObjectFit::Contain)
                .size_full()
                .into_any_element(),
            (Some(source), Some(scale)) => self.render_zoomed_develop(source, scale, cx),
        };

        let bounds = zoom.viewer_bounds.clone();
        let grab_fitted = zoom.space_held && zoom.scale.is_none();
        div()
            .id("develop-viewer")
            .relative()
            .when(grab_fitted, |el| {
                el.cursor_grab().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, _w, cx| {
                        this.grab_fitted_develop(event.position, cx);
                    }),
                )
            })
            .on_mouse_down(
                MouseButton::Right,
                Self::right_click_listener(cx.entity(), self.catalog.selected),
            )
            .flex_1()
            .min_w(sp(0.0))
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .bg(self.viewer_bg())
            // The photo area, inset so the picture has breathing room. Its
            // bounds are what zooming and the retouch / gradient overlays
            // measure against, so everything photo-related lives inside it.
            .child(
                div()
                    .absolute()
                    .inset(px(PHOTO_PADDING))
                    .flex()
                    .items_center()
                    .justify_center()
                    // Records the photo area's bounds, for zooming around the
                    // pointer.
                    .child(
                        canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                            .absolute()
                            .size_full(),
                    )
                    .child(content)
                    .children(self.render_grid_overlay())
                    .children(self.render_retouch_overlay(cx))
                    .children(self.render_gradient_overlay(cx))
                    .children(self.render_white_balance_overlay(cx))
                    .children(self.develop_split_overlay(cx)),
            )
            .child(self.render_zoom_badge(cx))
            .children(self.ba_pill(true, cx))
            .children(self.render_info_overlay(cx))
            .into_any_element()
    }

    /// The split comparison over the Develop photo.
    fn develop_split_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let zoom = &self.develop_ui.zoom;
        let (w, h) = self.develop_image_size()?;
        let photo = zoom.photo_rect((w, h))?;
        self.ba_split_overlay(photo, zoom.viewer_bounds.get(), cx)
    }

    /// The composition guide, when one is chosen.
    fn render_grid_overlay(&self) -> Option<AnyElement> {
        let ui = &self.develop_ui;
        if ui.grid_overlay == super::grid_overlay::GridOverlay::Off {
            return None;
        }
        let (w, h) = self.develop_image_size()?;
        let photo = ui.zoom.photo_rect((w, h))?;
        let clip = ui.zoom.viewer_bounds.get();
        let lines = ui.grid_overlay.lines(w / h.max(1.0), ui.grid_orientation);
        Some(super::grid_overlay::overlay_canvas(
            lines,
            photo,
            clip,
            gpui::white().opacity(0.8),
        ))
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

    fn render_zoomed_develop(
        &self,
        source: ImageSource,
        scale: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let zoom = &self.develop_ui.zoom;
        let Some((w, h)) = self.develop_image_size() else {
            return div().into_any_element();
        };
        // Logical pixels: at 100% a Retina display shows one photo pixel per
        // physical pixel.
        let ratio = zoom.pixel_ratio.get();
        let (w, h) = (w * scale / ratio, h * scale / ratio);
        zoom.zoomed_size.set((w, h));
        zoom.apply_anchor();

        let panning = zoom.pan.is_some();
        let viewer = zoom.viewer_bounds.get().size;
        let (viewer_w, viewer_h) = (f32::from(viewer.width), f32::from(viewer.height));
        div()
            .id("develop-zoom")
            .size_full()
            .overflow_scroll()
            .track_scroll(&zoom.scroll)
            .when(panning, |el| el.cursor_grabbing())
            .when(!panning, |el| el.cursor_grab())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _w, cx| {
                    // With a retouch tool picked, clicks place spots; only
                    // Space + drag pans.
                    let zoom = &this.develop_ui.zoom;
                    if this.photo_tool_active() && !zoom.space_held {
                        return;
                    }
                    this.start_develop_pan(event.position);
                    cx.notify();
                }),
            )
            .child(
                div()
                    // As wide and tall as the photo (or the viewer, if
                    // larger), so a centred photo can be scrolled to its
                    // left and top edges too, not only its right and bottom.
                    .min_w(px(w.max(viewer_w)))
                    .min_h(px(h.max(viewer_h)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(img(source).w(px(w)).h(px(h)).flex_shrink_0()),
            )
            .into_any_element()
    }

    /// The zoom level in the viewer's corner; clicking it toggles Fit and
    /// 100%.
    fn render_zoom_badge(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let hint = if self.develop_ui.zoom.scale.is_some() {
            "Ctrl + scroll to zoom \u{00B7} Space + drag to move \u{00B7} click for Fit"
        } else {
            "Ctrl + scroll to zoom \u{00B7} click for 100%"
        };
        div()
            .id("develop-zoom-badge")
            .absolute()
            .left(sp(10.0))
            .bottom(sp(10.0))
            .h(sp(22.0))
            .px_2()
            .flex()
            .items_center()
            .rounded_md()
            .bg(theme.surface.opacity(0.85))
            .border_1()
            .border_color(theme.border)
            .text_size(sp(11.0))
            .text_color(theme.text)
            .cursor_pointer()
            .hover(|el| el.bg(theme.surface_elevated))
            .child(self.develop_zoom_label())
            .tooltip(rustyedit_ui::tip(hint, &theme))
            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                this.toggle_develop_zoom(cx);
            }))
            .into_any_element()
    }

    /// The part of the photo in view; `None` when fitted.
    pub(super) fn develop_visible_region(&self) -> Option<(f32, f32, f32, f32)> {
        self.develop_ui.zoom.visible_region()
    }

    /// Follows a drag on the curve graph, the Navigator or the zoomed
    /// photo. Returns whether one was in progress (and so the event is
    /// used).
    pub(crate) fn develop_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.curve_drag_move(event.position, event.dragging(), cx)
            || self.retouch_drag_move(event.position, event.dragging(), cx)
            || self.gradient_drag_move(event.position, event.dragging(), cx)
        {
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
        self.develop_pan_move(event.position, event.dragging(), cx)
    }

    pub(crate) fn end_develop_drag(&mut self) {
        self.develop_ui.zoom.pan = None;
        self.develop_ui.navigator_dragging = false;
        self.develop_ui.curve_drag = None;
        self.end_retouch_drag();
        self.end_gradient_drag();
    }

    /// Whether a tool that works by clicking the photo (Heal / Clone, a
    /// gradient, the eyedropper) is picked, so plain clicks don't pan.
    fn photo_tool_active(&self) -> bool {
        self.active_photo_tool().is_some()
    }
}
