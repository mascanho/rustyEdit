//! The Develop module: the photo being edited between two columns. The
//! editing panel (histogram and sliders) takes the Inspector's place in the
//! current layout; the tools column (Navigator, Presets, Snapshots,
//! History) takes the Navigator's.
//!
//! - `source`: decoding the photo into a downscaled working copy
//! - `preview`: rendering adjustments into the live, in-memory preview
//! - `viewer`: the photo, fitted or zoomed, with drag-to-pan
//! - `zoom`: Ctrl + wheel zoom around the pointer, Space-drag panning
//! - `adjustments`: the slider sections and every Develop-settings change
//! - `retouch`: Heal and Clone spots, and drawing / dragging them
//! - `gradients`: Linear and Radial masks with local adjustments
//! - `sections`: each editing section's title bar (drag grip, reset
//!   button, on/off switch) and the user's section order
//! - `curve_editor`, `color_mixer`, `color_grading`: the editing panel's
//!   graphical and tabbed sections
//! - `tool_strip`: the pinned row of photo tools and view toggles under
//!   the histogram, and picking a tool
//! - `tools`: where the tools column goes and what it holds
//! - `navigator`, `presets`, `snapshots`, `history`: its sections

mod adjustments;
mod color_grading;
mod color_mixer;
mod crop;
mod curve_editor;
mod gradients;
mod history;
mod navigator;
mod presets;
mod preview;
mod retouch;
mod sections;
mod snapshots;
mod source;
mod tool_strip;
mod tools;
mod viewer;
mod zoom;

pub(crate) use adjustments::HistoryNote;
use gradients::GradientUi;
pub(crate) use preview::LivePreview;
use retouch::RetouchUi;
use zoom::DevelopZoom;

use std::{cell::Cell, rc::Rc};

use gpui::{div, prelude::*, px, AnyElement, Bounds, Context, Pixels, ScrollHandle, Window};
use rustyedit_core::{
    settings::DockSide, Adjustment, CurveChannel, DevelopSection, MixerProp, Presets, Settings,
};
use rustyedit_ui::PanelSide;

use crate::{library::panel_group_heading, workspace::Workspace};

/// View state owned by the Develop module.
pub(crate) struct DevelopUi {
    pub(crate) tools_collapsed: bool,
    pub(crate) tools_scroll: ScrollHandle,
    /// Zoom level and pan position of the photo.
    pub(crate) zoom: DevelopZoom,
    /// Where the Navigator's picture was painted, for turning clicks into
    /// positions on the photo.
    pub(crate) navigator_bounds: Rc<Cell<Bounds<Pixels>>>,
    pub(crate) navigator_dragging: bool,
    /// The user's saved presets (the built-in ones aren't stored).
    pub(crate) presets: Presets,
    /// The tone curve channel being edited.
    pub(crate) curve_channel: CurveChannel,
    /// Index of the curve point being dragged.
    pub(crate) curve_drag: Option<usize>,
    /// Where the curve graph was painted, for mapping the pointer onto it.
    pub(crate) curve_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The Color Mixer tab shown.
    pub(crate) mixer_prop: MixerProp,
    /// Heal / Clone tool state.
    pub(crate) retouch: RetouchUi,
    /// Linear / Radial gradient tool state.
    pub(crate) gradients: GradientUi,
}

impl DevelopUi {
    pub(crate) fn new(settings: &Settings) -> Self {
        Self {
            tools_collapsed: !settings.develop.tools_expanded_on_startup,
            tools_scroll: ScrollHandle::new(),
            zoom: DevelopZoom::default(),
            navigator_bounds: Rc::new(Cell::new(Bounds::default())),
            navigator_dragging: false,
            presets: rustyedit_io::load_presets(),
            curve_channel: CurveChannel::default(),
            curve_drag: None,
            curve_bounds: Rc::new(Cell::new(Bounds::default())),
            mixer_prop: MixerProp::default(),
            retouch: RetouchUi::default(),
            gradients: GradientUi::default(),
        }
    }
}

impl Workspace {
    pub(crate) fn render_develop(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.catalog.selected_asset().is_none() {
            return div().flex_1().into_any_element();
        }
        // The tools column includes Library's folder tree.
        self.prepare_file_tree();

        let viewer = self.render_develop_viewer(window, cx);
        let panel_side = self.develop_panel_side();
        let tools_side = self.develop_tools_side();

        // A layout that stacks the Navigator with the Inspector stacks the
        // tools above the sliders in the one column.
        let mut panel_children = Vec::new();
        let stacked = tools_side == Some(panel_side);
        if stacked {
            panel_children.push(panel_group_heading(&self.theme, "Tools"));
            panel_children.extend(self.develop_tools_sections(cx));
            panel_children.push(panel_group_heading(&self.theme, "Edit"));
        }
        panel_children.extend(self.develop_edit_sections(cx));

        let panel = rustyedit_ui::side_panel(
            "develop-side-panel",
            &self.theme,
            match panel_side {
                DockSide::Left => PanelSide::Left,
                DockSide::Right => PanelSide::Right,
            },
            self.settings.layout.inspector_width as f32,
            &self.develop_panel_scroll,
            Some(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(self.render_histogram_panel(cx))
                    .child(self.render_tool_strip(cx))
                    // Stacked layouts keep the Navigator in view here.
                    .when(stacked, |el| {
                        el.child(self.render_navigator(navigator::MAX_HEIGHT_SHARED, cx))
                    })
                    .into_any_element(),
            ),
            Some(self.develop_edit_footer(stacked, cx)),
            panel_children,
        )
        .into_any_element();
        let panel = self.dock_with_rail(panel_side, Some(panel), cx);
        let tools = tools_side
            .filter(|side| *side != panel_side)
            .map(|side| (side, self.render_develop_tools_dock(side, cx)));

        // Each side holds at most one of the two columns.
        let mut left = Vec::new();
        let mut right = Vec::new();
        for (side, elements) in tools.into_iter().chain([(panel_side, panel)]) {
            match side {
                DockSide::Left => left.extend(elements),
                DockSide::Right => right.extend(elements),
            }
        }

        let main = div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h(px(0.0))
            .children(left)
            .child(viewer)
            .children(right);
        self.with_filmstrip(main.into_any_element(), cx)
    }

    /// The editing sections (the histogram is pinned above them) in the
    /// user's order, then "Reset panel order" once it's been changed. The
    /// default order follows how a photo is worked on: retouch, tone, then
    /// colour, then detail and effects.
    fn develop_edit_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut sections: Vec<AnyElement> = self
            .develop_section_order()
            .into_iter()
            .map(|section| {
                let (key, children) = self.section_contents(section, cx);
                self.develop_section(section, key, children, cx)
            })
            .collect();
        sections.extend(self.section_order_footer(cx));
        sections
    }

    /// A section's fold-state key and its controls.
    fn section_contents(
        &self,
        section: DevelopSection,
        cx: &mut Context<Self>,
    ) -> (&'static str, Vec<AnyElement>) {
        use DevelopSection as S;
        match section {
            S::Crop => ("develop-crop", self.crop_section(cx)),
            S::Retouch => ("develop-retouch", self.retouch_section(cx)),
            S::Gradients => ("develop-gradients", self.gradients_section(cx)),
            S::Basic => (
                "develop-basic",
                self.adjustment_sliders(&Adjustment::BASIC, cx),
            ),
            S::ToneCurve => ("develop-curve", self.curve_section(cx)),
            S::Color => (
                "develop-color",
                self.adjustment_sliders(&Adjustment::COLOR, cx),
            ),
            S::ColorMixer => ("develop-mixer", self.color_mixer_section(cx)),
            S::ColorGrading => ("develop-grading", self.color_grading_section(cx)),
            S::Detail => (
                "develop-detail",
                self.adjustment_sliders(&Adjustment::DETAIL, cx),
            ),
            S::Effects => (
                "develop-effects",
                self.adjustment_sliders(&Adjustment::EFFECTS, cx),
            ),
        }
    }

    /// The editing panel takes the Inspector's slot in the current layout,
    /// so it stays on the side the user expects; layouts with no Inspector
    /// (Focus) still need it, so they fall back to the right.
    pub(super) fn develop_panel_side(&self) -> DockSide {
        self.settings
            .layout
            .preset
            .arrangement()
            .inspector_side()
            .unwrap_or(DockSide::Right)
    }
}
