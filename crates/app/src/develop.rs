use std::sync::Arc;

use gpui::{div, img, prelude::*, px, AnyElement, Context, ObjectFit, Window};
use rustyedit_core::{settings::DockSide, Adjustment, Histogram};
use rustyedit_ui::PanelSide;

use crate::workspace::Workspace;

impl Workspace {
    pub(crate) fn render_develop(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;

        if self.catalog.selected_asset().is_none() {
            return div().flex_1().into_any_element();
        }

        let image_path = self.preview_path.clone().or_else(|| {
            self.catalog
                .selected_asset()
                .map(|a| a.display_source().to_path_buf())
        });

        let viewer = div()
            .id("develop-viewer")
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.background)
            .when(!self.zoom_fit, |el| {
                el.overflow_x_scroll().overflow_y_scroll()
            })
            .child(match image_path {
                Some(path) => img(path)
                    .object_fit(if self.zoom_fit {
                        ObjectFit::Contain
                    } else {
                        ObjectFit::None
                    })
                    .size_full()
                    .into_any_element(),
                None => div()
                    .text_color(theme.text_muted)
                    .child("Loading…")
                    .into_any_element(),
            });

        // The editing panel takes the Inspector's slot in the current
        // layout, so it stays on the side the user expects; layouts with no
        // Inspector (Focus) still need it, so they fall back to the right.
        let side = match self.settings.layout.preset.arrangement().inspector_side() {
            Some(DockSide::Left) => PanelSide::Left,
            _ => PanelSide::Right,
        };
        let panel = rustyedit_ui::side_panel(
            &theme,
            side,
            self.settings.layout.inspector_width as f32,
            &self.develop_panel_scroll,
            vec![
                self.render_main_section(cx),
                self.adjustment_section("Basic", &Adjustment::BASIC, cx),
                self.adjustment_section("Color", &Adjustment::COLOR, cx),
                self.adjustment_section("Detail", &Adjustment::DETAIL, cx),
            ],
        );

        let main = div().flex().flex_row().flex_1().min_h(px(0.0));
        let (dock_side, panel) = match side {
            PanelSide::Left => (DockSide::Left, panel.into_any_element()),
            PanelSide::Right => (DockSide::Right, panel.into_any_element()),
        };
        let docked = self.dock_with_rail(dock_side, Some(panel), cx);
        let main = match side {
            PanelSide::Left => main.children(docked).child(viewer),
            PanelSide::Right => main.child(viewer).children(docked),
        };

        self.with_filmstrip(main.into_any_element(), cx)
    }

    fn adjustment_section(
        &self,
        title: &'static str,
        adjustments: &[Adjustment],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let children = adjustments
            .iter()
            .map(|adj| self.adjustment_slider(*adj, cx).into_any_element())
            .collect();
        rustyedit_ui::section(&theme, title, children).into_any_element()
    }

    fn adjustment_slider(
        &self,
        adjustment: Adjustment,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let value = self
            .catalog
            .selected_asset()
            .map(|asset| adjustment.get(&asset.develop))
            .unwrap_or(0.0);
        let (min, max) = adjustment.range();
        let entity = cx.entity();

        rustyedit_ui::labeled_slider(
            gpui::SharedString::from(format!("adjustment-{:?}", adjustment)),
            adjustment.label(),
            value,
            min,
            max,
            &theme,
            move |new_value, _window, cx| {
                entity.update(cx, |this, cx| {
                    this.apply_adjustment(adjustment, new_value, cx);
                });
            },
        )
    }

    pub(crate) fn apply_adjustment(
        &mut self,
        adjustment: Adjustment,
        value: f32,
        cx: &mut Context<Self>,
    ) {
        let ix = self.catalog.selected;
        if let Some(asset) = self.catalog.assets.get_mut(ix) {
            let state_before = asset.state();
            let before = adjustment.get(&asset.develop);
            adjustment.set(&mut asset.develop, value);
            let after = adjustment.get(&asset.develop);
            let state_after = asset.state();
            self.push_undo(
                adjustment.label(),
                vec![(ix, state_before, state_after)],
                true,
            );
            let asset = &mut self.catalog.assets[ix];
            asset.record_change(
                adjustment.label(),
                format!("{before:+.0}"),
                format!("{after:+.0}"),
            );
        }
        self.mark_catalog_dirty(cx);
        self.request_preview_update(cx);
        cx.notify();
    }

    /// Decodes the currently selected asset's full source file on a
    /// background thread, downscales it to a manageable "preview" size, and
    /// stashes the result so sliders can re-render quickly without
    /// re-reading the file from disk on every change.
    pub(crate) fn load_source_image(&mut self, cx: &mut Context<Self>) {
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        let asset_id = asset.id;
        let path = asset.path.clone();
        let epoch = self.preview_epoch;
        // Longest edge the decoded source is downscaled to before it ever
        // touches the develop pipeline. Lightroom calls this a "smart
        // preview"; it keeps slider drags fast regardless of how big the
        // original is, at the cost of not previewing at 100% detail.
        let max_edge = self.settings.develop.preview_quality.max_edge();

        let decode = cx.background_executor().spawn(async move {
            rustyedit_io::open_image(&path)
                .ok()
                .map(|image| rustyedit_processing::downscale_for_preview(&image, max_edge))
        });

        cx.spawn(async move |this, cx| {
            let decoded = decode.await;
            this.update(cx, |this, cx| {
                if this.preview_epoch != epoch {
                    return;
                }
                if let Some(image) = decoded {
                    this.source_image = Some(Arc::new(image));
                    this.source_image_for = Some(asset_id);
                    this.request_preview_update(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Re-renders the develop pipeline for the current asset's adjustments
    /// and writes the result to a fresh live-preview file. Coalesces rapid
    /// changes (e.g. a slider drag) into a single trailing render instead of
    /// queueing one render per mouse-move event.
    pub(crate) fn request_preview_update(&mut self, cx: &mut Context<Self>) {
        if self.preview_busy {
            self.preview_pending = true;
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
        self.preview_busy = true;
        self.preview_pending = false;

        let render = cx.background_executor().spawn(async move {
            let rendered = rustyedit_processing::apply(&source, &settings);
            let histogram = Histogram::from_rgb8(rendered.as_raw());
            let path = rustyedit_io::next_live_preview_path();
            let _ = rendered.save(&path);
            (path, histogram)
        });

        cx.spawn(async move |this, cx| {
            let (new_path, histogram) = render.await;
            this.update(cx, |this, cx| {
                this.preview_busy = false;

                if this.preview_epoch == epoch {
                    if let Some(stale) = this.previous_preview_path.take() {
                        rustyedit_io::cleanup_live_preview(&stale);
                    }
                    this.previous_preview_path = this.preview_path.take();
                    this.preview_path = Some(new_path);
                    this.develop_histogram = Some((asset_id, Arc::new(histogram)));
                } else {
                    rustyedit_io::cleanup_live_preview(&new_path);
                }

                if this.preview_pending {
                    this.request_preview_update(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
