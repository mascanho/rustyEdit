//! Ctrl + mouse wheel zooms the photos in every module: the loupe steps
//! through its zoom levels, the grid resizes its cards, and Develop toggles
//! between fit and 100%.
//!
//! The wheel is intercepted in the capture phase, before any scroll
//! container sees it, so zooming never scrolls the list underneath.

use gpui::{
    canvas, prelude::*, px, AnyElement, Context, DispatchPhase, ScrollWheelEvent, Window,
};
use rustyedit_core::settings::{GRID_CARD_MAX, GRID_CARD_MIN, GRID_CARD_STEP};

use crate::{
    actions::{GridLarger, GridSmaller},
    compare::ZOOM_STEPS,
    workspace::{LibraryView, Mode, Workspace},
};

/// Wheel travel, in pixels, that makes one zoom step. Trackpads report many
/// small deltas; this keeps them from racing through every level.
const PIXELS_PER_STEP: f32 = 40.0;

impl Workspace {
    /// An invisible element whose paint hooks the window's wheel events.
    /// Place it once in the root.
    pub(crate) fn wheel_zoom_hook(&self, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let entity = entity.clone();
                window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture || !event.modifiers.control {
                        return;
                    }
                    let delta = event.delta.pixel_delta(window.line_height());
                    let handled =
                        entity.update(cx, |this, cx| this.wheel_zoom(f32::from(delta.y), cx));
                    if handled {
                        cx.stop_propagation();
                    }
                });
            },
        )
        .absolute()
        .size(px(0.0))
        .into_any_element()
    }

    /// Grows (positive) or shrinks the grid cards by `steps` size steps.
    fn step_grid_card_size(&mut self, steps: i32, cx: &mut Context<Self>) {
        let size = (self.settings.library.grid_card_size as i32 + steps * GRID_CARD_STEP as i32)
            .clamp(GRID_CARD_MIN as i32, GRID_CARD_MAX as i32) as u32;
        self.update_settings(|s| s.library.grid_card_size = size, cx);
    }

    /// Ctrl + / Ctrl − in the grid: one card-size step, like a wheel notch.
    fn keyboard_grid_zoom(&mut self, steps: i32, cx: &mut Context<Self>) {
        if self.settings_open
            || self.mode != Mode::Library
            || self.library_view != LibraryView::Grid
        {
            return;
        }
        self.step_grid_card_size(steps, cx);
    }

    pub fn grid_larger(&mut self, _: &GridLarger, _w: &mut Window, cx: &mut Context<Self>) {
        self.keyboard_grid_zoom(1, cx);
    }

    pub fn grid_smaller(&mut self, _: &GridSmaller, _w: &mut Window, cx: &mut Context<Self>) {
        self.keyboard_grid_zoom(-1, cx);
    }

    /// Applies a wheel movement (positive = away from the user = zoom in).
    /// Returns whether this screen zooms, i.e. whether the event was used.
    fn wheel_zoom(&mut self, delta_y: f32, cx: &mut Context<Self>) -> bool {
        if self.settings_open || self.catalog_menu_open {
            return false;
        }
        let zoomable = match self.mode {
            Mode::Library => matches!(self.library_view, LibraryView::Loupe | LibraryView::Grid),
            Mode::Develop => true,
            Mode::Export => false,
        };
        if !zoomable {
            return false;
        }

        // Accumulate small deltas; a change of direction starts over.
        if self.wheel_accum.signum() != delta_y.signum() {
            self.wheel_accum = 0.0;
        }
        self.wheel_accum += delta_y;
        let steps = (self.wheel_accum / PIXELS_PER_STEP).trunc() as i32;
        if steps == 0 {
            return true;
        }
        self.wheel_accum -= steps as f32 * PIXELS_PER_STEP;

        match (self.mode, self.library_view) {
            (Mode::Library, LibraryView::Loupe) => {
                // Fit, then each pixel scale in turn.
                let levels: Vec<Option<f32>> = std::iter::once(None)
                    .chain(ZOOM_STEPS.iter().copied().map(Some))
                    .collect();
                let current = levels
                    .iter()
                    .position(|l| *l == self.loupe_zoom)
                    .unwrap_or(0);
                let next = (current as i32 + steps).clamp(0, levels.len() as i32 - 1) as usize;
                if next != current {
                    self.set_loupe_zoom(levels[next], cx);
                }
            }
            (Mode::Library, LibraryView::Grid) => self.step_grid_card_size(steps, cx),
            (Mode::Develop, _) => {
                self.set_develop_zoom_fit(steps < 0, cx);
            }
            _ => {}
        }
        true
    }
}
