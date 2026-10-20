//! The Compare and Survey views, and the loupe zoom.
//!
//! * **Compare** puts two photos side by side. The active pane follows the
//!   arrow keys; the other stays put as a reference. Click a pane to make it
//!   the active one.
//! * **Survey** lays out the multi-selection (or, with none, the photos
//!   that follow the current one) in an auto-sized grid.
//! * **Zoom** magnifies the loupe photo to a pixel scale and lets you pan.

use gpui::{
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
            .min_h(px(0.0))
            .min_w(px(0.0))
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
            .min_w(px(0.0))
            .min_h(px(0.0))
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
                div().flex_1().min_h(px(0.0)).p_2().child(
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
                    .h(px(26.0))
                    .px_3()
                    .border_t_1()
                    .border_color(theme.border_subtle)
                    .text_xs()
                    .text_color(if active { theme.text } else { theme.text_muted })
                    .child(asset.file_name())
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
    fn survey_indices(&self) -> Vec<usize> {
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
                    .min_h(px(0.0))
                    .gap_2()
                    .children(tiles)
                    .into_any_element()
            })
            .collect();

        div()
            .id("library-survey")
            .flex_1()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .overflow_hidden()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .bg(theme.background)
            .children(rows)
            .into_any_element()
    }

    fn survey_tile(&self, ix: usize, removable: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let asset = &self.catalog.assets[ix];
        let active = ix == self.catalog.selected;

        div()
            .id(("survey-tile", ix))
            .relative()
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
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
                div().size_full().p_1().child(
                    img(asset.display_source().to_path_buf())
                        .object_fit(ObjectFit::Contain)
                        .size_full(),
                ),
            )
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
                    .h(px(20.0))
                    .bg(theme.background.opacity(0.7))
                    .text_size(px(10.0))
                    .text_color(theme.text_muted)
                    .child(asset.file_name())
                    .child(rustyedit_ui::flag_badge(asset.flag, &theme)),
            )
            .when(removable, |el| {
                el.child(div().absolute().top(px(4.0)).right(px(4.0)).child(
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
        if self.loupe_center_pending.get() && bounds.size.width > px(0.0) {
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
                .cursor_grab()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, _w, _cx| {
                        this.loupe_drag = Some((event.position, this.loupe_scroll.offset()));
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
