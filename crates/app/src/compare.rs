//! The Compare and Survey views, and the loupe zoom.
//!
//! * **Compare** puts two photos side by side. The active pane follows the
//!   arrow keys; the other stays put as a reference. Click a pane to make it
//!   the active one.
//! * **Survey** lays out the multi-selection (or, with none, the photos
//!   that follow the current one) in an auto-sized grid.
//! * **Zoom** magnifies the loupe photo to a pixel scale and lets you pan.

use rustyedit_ui::sp;
use gpui::{
    canvas,
    div, img, point, prelude::*, px, AnyElement, ClickEvent, Context, MouseButton, MouseDownEvent,
    ObjectFit, SharedString, Window,
};

use crate::workspace::{LibraryView, Mode, Workspace};

const SURVEY_MAX: usize = 12;
/// Pixel scales offered for the zoomed loupe (1.0 = one image pixel per
/// screen pixel).
pub(crate) const ZOOM_STEPS: [f32; 3] = [1.0, 2.0, 4.0];

impl Workspace {
    // --- View switching ------------------------------------------------------

    pub(crate) fn set_library_view(&mut self, view: LibraryView, cx: &mut Context<Self>) {
        self.library_view = view;
        self.bottom_menu = None;
        if view != LibraryView::Loupe {
            self.loupe_zoom = None;
        } else {
            // An edited photo shows its edits in the loupe.
            self.refresh_live_preview(cx);
        }
        match view {
            LibraryView::Compare => {
                // Active photo on the left, the next one as its rival.
                let selected = self.catalog.selected;
                let visible = self.catalog.visible_indices();
                let pos = self.catalog.visible_position(selected).unwrap_or(0);
                let rival = visible
                    .get(pos + 1)
                    .or_else(|| pos.checked_sub(1).and_then(|p| visible.get(p)))
                    .copied()
                    .unwrap_or(selected);
                self.compare_pair = (selected, rival);
                self.compare_active_left = true;
                self.request_loupe_for(rival, cx);
            }
            LibraryView::Grid => {}
            _ => {}
        }
        self.on_selection_changed(cx);
    }

    pub fn view_compare(
        &mut self,
        _: &crate::actions::ViewCompare,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode == Mode::Library && !self.catalog.assets.is_empty() {
            self.set_library_view(LibraryView::Compare, cx);
        }
    }

    pub fn view_survey(
        &mut self,
        _: &crate::actions::ViewSurvey,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode == Mode::Library && !self.catalog.assets.is_empty() {
            self.set_library_view(LibraryView::Survey, cx);
        }
    }

    /// Keeps the active Compare pane pointing at the selected photo.
    pub(crate) fn sync_compare_with_selection(&mut self) {
        if self.library_view == LibraryView::Compare {
            if self.compare_active_left {
                self.compare_pair.0 = self.catalog.selected;
            } else {
                self.compare_pair.1 = self.catalog.selected;
            }
        }
    }

    // --- Compare -------------------------------------------------------------

    pub(crate) fn render_compare(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let (left, right) = self.compare_pair;
        div()
            .id("library-compare")
            .flex_1()
            .min_h(sp(0.0))
            .min_w(sp(0.0))
            .overflow_hidden()
            .flex()
            .flex_row()
            .gap_2()
            .p_3()
            .bg(theme.background)
            .child(self.compare_pane(left, true, cx))
            .child(self.compare_pane(right, false, cx))
            .into_any_element()
    }

    fn compare_pane(&self, ix: usize, is_left: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let Some(asset) = self.catalog.assets.get(ix) else {
            return div().flex_1().into_any_element();
        };
        let active = self.compare_active_left == is_left;

        div()
            .id(SharedString::from(format!("compare-pane-{is_left}")))
            .flex_1()
            .min_w(sp(0.0))
            .min_h(sp(0.0))
            .flex()
            .flex_col()
            .rounded_lg()
            .overflow_hidden()
            .bg(theme.surface)
            .border_2()
            .border_color(if active {
                theme.accent
            } else {
                theme.border_subtle
            })
            .cursor_pointer()
            .child(
                div().flex_1().min_h(sp(0.0)).p_2().child(
                    img(self.loupe_source(asset))
                        .object_fit(ObjectFit::Contain)
                        .size_full(),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .flex_shrink_0()
                    .h(sp(26.0))
                    .px_3()
                    .border_t_1()
                    .border_color(theme.border_subtle)
                    .text_xs()
                    .text_color(if active { theme.text } else { theme.text_muted })
                    .child(asset.display_name())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(rustyedit_ui::flag_badge(asset.flag, &theme))
                            .child(rustyedit_ui::rating_stars(
                                SharedString::from(format!("compare-stars-{is_left}")),
                                asset.rating,
                                &theme,
                                11.0,
                                None::<fn(u8, &mut Window, &mut gpui::App)>,
                            )),
                    ),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _w, cx| {
                    this.compare_active_left = is_left;
                    this.open_context_menu(ix, event.position, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                Self::right_click_listener(cx.entity(), ix),
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, _w, cx| {
                this.compare_active_left = is_left;
                this.catalog.select(ix);
                this.on_selection_changed(cx);
                if event.click_count() >= 2 {
                    this.set_library_view(LibraryView::Loupe, cx);
                }
            }))
            .into_any_element()
    }

    // --- Survey --------------------------------------------------------------

    /// The photos the survey shows.
    pub(crate) fn survey_indices(&self) -> Vec<usize> {
        let targets = self.target_indices();
        if targets.len() > 1 {
            return targets.into_iter().take(SURVEY_MAX).collect();
        }
        let visible = self.catalog.visible_indices();
        let pos = self
            .catalog
            .visible_position(self.catalog.selected)
            .unwrap_or(0);
        visible.iter().skip(pos).take(6).copied().collect()
    }

    pub(crate) fn render_survey(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let indices = self.survey_indices();
        let n = indices.len().max(1);
        let cols = (n as f32).sqrt().ceil() as usize;
        let multi = self.is_multi_selected();

        let rows: Vec<AnyElement> = indices
            .chunks(cols)
            .map(|chunk| {
                let mut tiles: Vec<AnyElement> = chunk
                    .iter()
                    .map(|&ix| self.survey_tile(ix, multi, cx))
                    .collect();
                // Pad the last row so tiles keep one size.
                while tiles.len() < cols {
                    tiles.push(div().flex_1().into_any_element());
                }
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h(sp(0.0))
                    .gap_2()
                    .children(tiles)
                    .into_any_element()
            })
            .collect();

        let zoom = self.survey_zoom;
        let controls = {
            let button = |id: &'static str, label: &'static str, factor: f32, cx: &mut Context<Self>| {
                div()
                    .id(id)
                    .flex()
                    .items_center()
                    .justify_center()
                    .min_w(sp(22.0))
                    .h(sp(20.0))
                    .px_1()
                    .rounded_sm()
                    .cursor_pointer()
                    .text_size(sp(11.0))
                    .text_color(theme.text)
                    .hover(|el| el.bg(theme.surface_elevated))
                    .child(label)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        if factor == 0.0 {
                            this.survey_zoom = 1.0;
                            this.survey_centre = (0.5, 0.5);
                            cx.notify();
                        } else {
                            this.zoom_survey(factor, cx);
                        }
                    }))
            };
            div()
                .absolute()
                .top(sp(8.0))
                .right(sp(12.0))
                .flex()
                .flex_row()
                .items_center()
                .gap_0p5()
                .px_1()
                .rounded_md()
                .bg(theme.surface.opacity(0.92))
                .border_1()
                .border_color(theme.border)
                .child(
                    div()
                        .px_1()
                        .text_size(sp(10.0))
                        .text_color(theme.text_muted)
                        .child(if zoom <= 1.001 {
                            "Linked zoom: scroll to zoom".to_string()
                        } else {
                            format!("Linked zoom {:.0}%", zoom * 100.0)
                        }),
                )
                .child(button("survey-zoom-out", "\u{2212}", 1.0 / 1.4, cx))
                .child(button("survey-zoom-in", "+", 1.4, cx))
                .child(button("survey-zoom-fit", "Fit", 0.0, cx))
        };
        div()
            .id("library-survey")
            .relative()
            .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _w, cx| {
                let dy = f32::from(event.delta.pixel_delta(sp(16.0)).y);
                if dy != 0.0 {
                    cx.stop_propagation();
                    // Scrolling up zooms in, 12% a notch.
                    this.zoom_survey(1.12f32.powf((dy / 16.0).clamp(-4.0, 4.0)), cx);
                }
            }))
            .flex_1()
            .min_h(sp(0.0))
            .min_w(sp(0.0))
            .overflow_hidden()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .bg(theme.background)
            .children(rows)
            .child(controls)
            .into_any_element()
    }

    /// A survey tile's picture: the photo fitted, or, once zoomed, every
    /// tile enlarged about the same point so the same spot of each photo
    /// can be compared. Dragging pans them together.
    fn survey_picture(&self, asset: &rustyedit_core::Asset, cx: &mut Context<Self>) -> AnyElement {
        let zoom = self.survey_zoom;
        let size_cell = self.survey_tile_size.clone();
        let measure = canvas(
            move |b, _, _| size_cell.set((f32::from(b.size.width), f32::from(b.size.height))),
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        if zoom <= 1.001 {
            return div()
                .relative()
                .size_full()
                .p_1()
                .child(measure)
                .child(
                    img(self
                        .display_thumbnail(asset)
                        .unwrap_or_else(|| asset.display_source().to_path_buf()))
                    .object_fit(ObjectFit::Contain)
                    .size_full(),
                )
                .into_any_element();
        }
        // Past 2x the thumbnail is too coarse: use the large preview.
        let source = if zoom >= 2.0 {
            self.loupe_source(asset)
        } else {
            self.display_thumbnail(asset)
                .unwrap_or_else(|| asset.display_source().to_path_buf())
        };
        let (cx0, cy0) = self.survey_centre;
        div()
            .id(("survey-zoomed", asset.id as usize))
            .relative()
            .size_full()
            .overflow_hidden()
            .cursor(rustyedit_ui::GRAB_CURSOR)
            .child(measure)
            .child(
                img(source)
                    .object_fit(ObjectFit::Contain)
                    .absolute()
                    .w(gpui::relative(zoom))
                    .h(gpui::relative(zoom))
                    .left(gpui::relative(0.5 - cx0 * zoom))
                    .top(gpui::relative(0.5 - cy0 * zoom)),
            )
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, _w, _cx| {
                    this.survey_pan = Some((event.position, this.survey_centre));
                }),
            )
            .into_any_element()
    }

    /// Zooms every survey tile by `factor` (back to fitted at 1).
    pub(crate) fn zoom_survey(&mut self, factor: f32, cx: &mut Context<Self>) {
        self.survey_zoom = (self.survey_zoom * factor).clamp(1.0, 8.0);
        if self.survey_zoom <= 1.001 {
            self.survey_zoom = 1.0;
            self.survey_centre = (0.5, 0.5);
        }
        cx.notify();
    }

    /// Follows a pan of the survey tiles; `true` while one is in progress.
    pub(crate) fn survey_drag_move(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        dragging: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((from, centre)) = self.survey_pan else {
            return false;
        };
        if !dragging {
            self.survey_pan = None;
            return true;
        }
        let (tw, th) = self.survey_tile_size.get();
        if tw > 0.0 && th > 0.0 {
            let (dx, dy) = (f32::from(position.x - from.x), f32::from(position.y - from.y));
            // Dragging moves the picture with the pointer, so the point at
            // the middle moves the other way.
            self.survey_centre = (
                (centre.0 - dx / (tw * self.survey_zoom)).clamp(0.0, 1.0),
                (centre.1 - dy / (th * self.survey_zoom)).clamp(0.0, 1.0),
            );
            cx.notify();
        }
        true
    }

    fn survey_tile(&self, ix: usize, removable: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let asset = &self.catalog.assets[ix];
        let active = ix == self.catalog.selected;

        div()
            .id(("survey-tile", ix))
            .relative()
            .flex_1()
            .min_w(sp(0.0))
            .min_h(sp(0.0))
            .rounded_lg()
            .overflow_hidden()
            .bg(theme.surface)
            .border_2()
            .border_color(if active {
                theme.accent
            } else {
                theme.border_subtle
            })
            .cursor_pointer()
            .child(self.survey_picture(asset, cx))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .h(sp(20.0))
                    .bg(theme.background.opacity(0.7))
                    .text_size(sp(10.0))
                    .text_color(theme.text_muted)
                    .child(asset.display_name())
                    .child(rustyedit_ui::flag_badge(asset.flag, &theme)),
            )
            .when(removable, |el| {
                el.child(div().absolute().top(sp(4.0)).right(sp(4.0)).child(
                    rustyedit_ui::glyph_button(
                        SharedString::from(format!("survey-remove-{ix}")),
                        "\u{2715}",
                        "Remove from survey",
                        theme.text,
                        true,
                        &theme,
                        cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            this.selection.remove(&ix);
                            if this.catalog.selected == ix {
                                if let Some(&other) = this.selection.iter().min() {
                                    this.catalog.select(other);
                                }
                            }
                            this.on_selection_changed(cx);
                        }),
                    ),
                ))
            })
            .on_click(cx.listener(move |this, event: &ClickEvent, _w, cx| {
                this.catalog.select(ix);
                this.on_selection_changed(cx);
                if event.click_count() >= 2 {
                    this.selection.clear();
                    this.set_library_view(LibraryView::Loupe, cx);
                }
            }))
            .into_any_element()
    }

    // --- Loupe zoom ----------------------------------------------------------

    /// Sets the loupe magnification (`None` = fit the window) and centres
    /// the view. Zooming also asks for a full-resolution copy when the
    /// original can't be shown directly.
    pub(crate) fn set_loupe_zoom(&mut self, zoom: Option<f32>, cx: &mut Context<Self>) {
        self.loupe_zoom = zoom;
        self.loupe_center_pending.set(true);
        if zoom.is_some() {
            self.request_full_preview(cx);
        }
        cx.notify();
    }

    /// `z`: toggle between fit and 100%.
    pub(crate) fn toggle_loupe_zoom(&mut self, cx: &mut Context<Self>) {
        let next = if self.loupe_zoom.is_some() {
            None
        } else {
            Some(1.0)
        };
        self.set_loupe_zoom(next, cx);
    }

    /// The loupe photo magnified to `scale`, in a pannable viewport.
    pub(crate) fn render_zoomed_loupe(
        &self,
        scale: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let asset = self.catalog.selected_asset()?;
        if asset.width == 0 || asset.height == 0 {
            return None;
        }
        // Logical pixels, so a Retina display still shows 1 image pixel
        // per physical pixel at 100%.
        let factor = scale / window.scale_factor();
        let (w, h) = (asset.width as f32 * factor, asset.height as f32 * factor);
        let source = self
            .loupe_full_paths
            .get(&asset.id)
            .cloned()
            .unwrap_or_else(|| self.loupe_source(asset));

        // Centre the photo the first frame after zooming, once the
        // viewport's size is known.
        let bounds = self.loupe_scroll.bounds();
        if self.loupe_center_pending.get() && bounds.size.width > sp(0.0) {
            let off_x = ((w - f32::from(bounds.size.width)) / 2.0).max(0.0);
            let off_y = ((h - f32::from(bounds.size.height)) / 2.0).max(0.0);
            self.loupe_scroll.set_offset(point(px(-off_x), px(-off_y)));
            self.loupe_center_pending.set(false);
        }

        Some(
            div()
                .id("loupe-zoom")
                .size_full()
                .overflow_scroll()
                .track_scroll(&self.loupe_scroll)
                .when(self.loupe_drag.is_some(), |el| {
                    el.cursor(rustyedit_ui::GRABBING_CURSOR)
                })
                .when(self.loupe_drag.is_none(), |el| el.cursor(rustyedit_ui::GRAB_CURSOR))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, _w, cx| {
                        this.loupe_drag = Some((event.position, this.loupe_scroll.offset()));
                        // Redraw so the grabbing hand shows straight away.
                        cx.notify();
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
                .into_any_element(),
        )
    }
}
