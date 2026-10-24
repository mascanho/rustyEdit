//! The size slider at the right of the footer. It controls whatever the
//! current view magnifies:
//!
//! - Library Grid: the card size;
//! - Library Loupe: the photo's zoom, from Fit to 400%;
//! - Develop: the photo's zoom, from Fit to 800%;
//! - Export: the size of the photo tiles.
//!
//! Zoom is mapped logarithmically (each stretch of the slider multiplies
//! the scale by the same amount), so it feels even from Fit to the top.

use rustyedit_ui::sp;
use gpui::{div, prelude::*, AnyElement, ClickEvent, Context};
use rustyedit_core::settings::{GRID_CARD_MAX, GRID_CARD_MIN, GRID_CARD_STEP};
use rustyedit_ui::Icon;

use crate::workspace::{LibraryView, Mode, Workspace};

const LOUPE_MAX_ZOOM: f32 = 4.0;
const DEVELOP_MAX_ZOOM: f32 = 8.0;
/// Below this, the slider snaps to Fit.
const FIT_SNAP: f32 = 0.02;
/// How far the − / + buttons move the slider.
const BUTTON_STEP: f32 = 0.1;
const SLIDER_WIDTH: f32 = 110.0;

#[derive(Clone, Copy, PartialEq)]
enum Target {
    GridCards,
    Loupe,
    Develop,
    ExportTiles,
}

impl Workspace {
    fn footer_zoom_target(&self) -> Option<Target> {
        match (self.mode, self.library_view) {
            (Mode::Library, LibraryView::Grid) => Some(Target::GridCards),
            (Mode::Library, LibraryView::Loupe) => Some(Target::Loupe),
            (Mode::Develop, _) => Some(Target::Develop),
            (Mode::Export, _) => Some(Target::ExportTiles),
            _ => None,
        }
    }

    /// The scale range a zoom target spans: `(fit, max)`.
    fn zoom_range(&self, target: Target) -> Option<(f32, f32)> {
        let (fit, max) = match target {
            Target::GridCards | Target::ExportTiles => return None,
            Target::Loupe => (self.loupe_fit_scale()?, LOUPE_MAX_ZOOM),
            Target::Develop => (self.develop_fit_scale()?, DEVELOP_MAX_ZOOM),
        };
        // A photo smaller than the window already fits above the maximum;
        // give the slider a little room anyway.
        Some((fit, max.max(fit * 2.0)))
    }

    /// The loupe photo's scale when fitted to its area.
    fn loupe_fit_scale(&self) -> Option<f32> {
        let asset = self.catalog.selected_asset()?;
        let bounds = self.loupe_bounds.get().size;
        let (vw, vh) = (f32::from(bounds.width), f32::from(bounds.height));
        if asset.width == 0 || asset.height == 0 || vw <= 0.0 {
            return None;
        }
        let ratio = self.pixel_ratio.get();
        Some((vw * ratio / asset.width as f32).min(vh * ratio / asset.height as f32))
    }

    /// Where the slider sits, `0..=1`.
    fn footer_zoom_fraction(&self, target: Target) -> f32 {
        let scale = match target {
            Target::GridCards => {
                let size = self.settings.library.grid_card_size as f32;
                let (min, max) = (GRID_CARD_MIN as f32, GRID_CARD_MAX as f32);
                return ((size - min) / (max - min)).clamp(0.0, 1.0);
            }
            Target::ExportTiles => {
                let (min, max) = crate::export::tile_size_range();
                return ((self.export_tile_size - min) / (max - min)).clamp(0.0, 1.0);
            }
            Target::Loupe => self.loupe_zoom,
            Target::Develop => self.develop_ui.zoom.scale,
        };
        match (scale, self.zoom_range(target)) {
            (Some(scale), Some((fit, max))) => {
                ((scale / fit).ln() / (max / fit).ln()).clamp(0.0, 1.0)
            }
            _ => 0.0,
        }
    }

    fn set_footer_zoom_fraction(&mut self, target: Target, t: f32, cx: &mut Context<Self>) {
        let t = t.clamp(0.0, 1.0);
        if target == Target::GridCards {
            let (min, max) = (GRID_CARD_MIN as f32, GRID_CARD_MAX as f32);
            let step = GRID_CARD_STEP as f32 / 2.0;
            let size = ((min + t * (max - min)) / step).round() * step;
            let size = (size as u32).clamp(GRID_CARD_MIN, GRID_CARD_MAX);
            if size != self.settings.library.grid_card_size {
                self.update_settings(|s| s.library.grid_card_size = size, cx);
            }
            return;
        }
        if target == Target::ExportTiles {
            let (min, max) = crate::export::tile_size_range();
            self.export_tile_size = (min + t * (max - min)).round().clamp(min, max);
            cx.notify();
            return;
        }
        let scale = match self.zoom_range(target) {
            Some((fit, max)) if t > FIT_SNAP => Some(fit * (max / fit).powf(t)),
            _ => None,
        };
        match target {
            Target::Loupe => self.set_loupe_zoom(scale, cx),
            Target::Develop => self.set_develop_scale(scale, cx),
            Target::GridCards | Target::ExportTiles => {}
        }
    }

    /// − / +: moves the slider by `delta`.
    fn step_footer_zoom(&mut self, target: Target, delta: f32, cx: &mut Context<Self>) {
        let at = self.footer_zoom_fraction(target);
        self.set_footer_zoom_fraction(target, at + delta, cx);
    }

    /// What the slider is at, in words: "180 px", "Fit", "250%".
    fn footer_zoom_label(&self, target: Target) -> String {
        let scale = match target {
            Target::GridCards => return format!("{} px", self.settings.library.grid_card_size),
            Target::ExportTiles => return format!("{} px", self.export_tile_size as u32),
            Target::Loupe => self.loupe_zoom,
            Target::Develop => self.develop_ui.zoom.scale,
        };
        match scale {
            None => "Fit".to_string(),
            Some(scale) => format!("{:.0}%", scale * 100.0),
        }
    }

    /// The slider with − / + buttons and its readout, or `None` where
    /// nothing zooms (Compare, Survey, Export).
    pub(crate) fn render_footer_zoom(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let target = self.footer_zoom_target()?;
        let theme = self.theme;
        let fraction = self.footer_zoom_fraction(target);
        let tooltip = match target {
            Target::GridCards | Target::ExportTiles => "Thumbnail size",
            Target::Loupe | Target::Develop => "Zoom",
        };
        let zoom_out = cx.listener(move |this, _: &ClickEvent, _w, cx| {
            this.step_footer_zoom(target, -BUTTON_STEP, cx)
        });
        let zoom_in = cx.listener(move |this, _: &ClickEvent, _w, cx| {
            this.step_footer_zoom(target, BUTTON_STEP, cx)
        });
        let button = |id: &'static str, icon: Icon| {
            div()
                .id(id)
                .flex()
                .items_center()
                .justify_center()
                .size(sp(18.0))
                .rounded_sm()
                .cursor_pointer()
                .hover(|el| el.bg(theme.surface_elevated))
                .child(rustyedit_ui::icon(icon, 11.0, theme.text_muted))
        };
        let entity = cx.entity();

        Some(
            div()
                .id("footer-zoom")
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .tooltip(rustyedit_ui::tip(tooltip, &theme))
                .child(button("footer-zoom-out", Icon::Minus).on_click(zoom_out))
                .child(rustyedit_ui::compact_slider(
                    "footer-zoom-slider",
                    fraction,
                    SLIDER_WIDTH,
                    &theme,
                    move |t, _w, cx| {
                        entity.update(cx, |this, cx| this.set_footer_zoom_fraction(target, t, cx));
                    },
                ))
                .child(button("footer-zoom-in", Icon::Plus).on_click(zoom_in))
                .child(
                    div()
                        .w(sp(44.0))
                        .text_color(theme.text)
                        .child(self.footer_zoom_label(target)),
                )
                .into_any_element(),
        )
    }
}
