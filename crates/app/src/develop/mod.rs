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
//! - `auto`: Auto Tone and the white-balance eyedropper
//! - `look`: Match look and New preset from photo
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
mod auto;
mod camera;
mod color_grading;
mod color_mixer;
mod crop;
mod crop_tool;
mod curve_editor;
mod frames;
mod detail;
mod gradients;
mod grid_overlay;
mod history;
mod look;
mod navigator;
pub(crate) use navigator::{navigator_size, navigator_slot};
mod presets;
mod preview;
mod proof_ui;
mod recipe_modal;
mod retouch;
mod sections;
mod snapshots;
mod source;
mod tool_strip;
mod upright;
mod tools;
mod viewer;
mod zoom;

pub(crate) use adjustments::HistoryNote;
use gradients::GradientUi;
pub(crate) use preview::LivePreview;
use retouch::RetouchUi;
use rustyedit_ui::sp;
use zoom::DevelopZoom;

use std::{cell::Cell, rc::Rc};

use gpui::{div, prelude::*, AnyElement, Bounds, Context, Pixels, Point, ScrollHandle, Window};
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
    /// The Presets panel's filter, strength and other view state.
    pub(crate) preset_ui: presets::PresetUi,
    /// The tool strip's View dropdown is open.
    pub(crate) view_menu_open: bool,
    /// Soft-proofing: the profile to simulate and what to show of it.
    pub(crate) proof: proof_ui::ProofUi,
    /// The composition guide over the photo, its mirrored variant (0-3),
    /// and whether its dropdown is open.
    pub(crate) grid_overlay: grid_overlay::GridOverlay,
    pub(crate) grid_orientation: u8,
    pub(crate) grid_menu_open: bool,
    pub(crate) ba_menu_open: bool,
    /// The tone curve channel being edited.
    pub(crate) curve_channel: CurveChannel,
    /// The Tone Curve section shows the parametric sliders instead of the
    /// point editor.
    pub(crate) curve_parametric: bool,
    /// Index of the curve point being dragged.
    pub(crate) curve_drag: Option<usize>,
    /// Where the curve graph was painted, for mapping the pointer onto it.
    pub(crate) curve_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The colour wheel being dragged, and where each was painted.
    pub(crate) wheel_drag: Option<rustyedit_core::ToneZone>,
    pub(crate) wheel_bounds: [Rc<Cell<Bounds<Pixels>>>; 4],
    /// The Color Grading section also shows plain sliders.
    pub(crate) grading_sliders: bool,
    /// The colour range the Color Mixer is editing, its pad's place on
    /// screen, and whether the pad is being dragged.
    pub(crate) mixer_band: rustyedit_core::ColorBand,
    pub(crate) mixer_pad: Rc<Cell<Bounds<Pixels>>>,
    pub(crate) mixer_pad_drag: bool,
    /// The Color Mixer also shows its tabbed sliders.
    pub(crate) mixer_sliders: bool,
    /// The Color Mixer tab shown.
    pub(crate) mixer_prop: MixerProp,
    /// Heal / Clone tool state.
    pub(crate) retouch: RetouchUi,
    /// Linear / Radial gradient tool state.
    pub(crate) gradients: GradientUi,
    /// The Crop tool is picked, and the drag in progress on its box.
    pub(crate) crop_tool: bool,
    pub(crate) crop_drag: Option<crop_tool::CropDrag>,
    /// What the pointer is over on the crop box, and where it is.
    pub(crate) crop_hover: Option<crop_tool::CropHit>,
    pub(crate) crop_pointer: Point<Pixels>,
    /// The curve target picker is picked.
    pub(crate) curve_target: bool,
    /// The white-balance eyedropper is picked.
    pub(crate) wb_picker: bool,
    /// Auto Tone or the eyedropper is measuring the photo.
    pub(crate) auto_busy: bool,
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
            preset_ui: presets::PresetUi::default(),
            view_menu_open: false,
            proof: proof_ui::ProofUi::new(settings.develop.proof_profile.as_ref()),
            grid_overlay: grid_overlay::GridOverlay::Off,
            grid_orientation: 0,
            grid_menu_open: false,
            ba_menu_open: false,
            curve_channel: CurveChannel::default(),
            curve_parametric: false,
            curve_drag: None,
            curve_bounds: Rc::new(Cell::new(Bounds::default())),
            wheel_drag: None,
            wheel_bounds: std::array::from_fn(|_| Rc::new(Cell::new(Bounds::default()))),
            grading_sliders: false,
            mixer_band: rustyedit_core::ColorBand::Red,
            mixer_pad: Rc::new(Cell::new(Bounds::default())),
            mixer_pad_drag: false,
            mixer_sliders: false,
            mixer_prop: MixerProp::default(),
            retouch: RetouchUi::default(),
            gradients: GradientUi::default(),
            crop_tool: false,
            crop_drag: None,
            crop_hover: None,
            crop_pointer: Point::default(),
            curve_target: false,
            wb_picker: false,
            auto_busy: false,
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
            panel_children.extend(self.develop_snapshot_history_sections(cx));
            panel_children.push(panel_group_heading(&self.theme, "Library"));
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
                    .when(stacked, |el| el.child(self.render_navigator(cx)))
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
            .min_h(sp(0.0))
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
            S::Basic => {
                let mut children = vec![self.auto_tone_row(cx)];
                children.extend(self.camera_jpeg_row(cx));
                children.extend(self.adjustment_sliders(&Adjustment::BASIC, cx));
                ("develop-basic", children)
            }
            S::ToneCurve => ("develop-curve", self.curve_section(cx)),
            S::Color => {
                let mut children = vec![self.white_balance_row(cx)];
                children.extend(self.adjustment_sliders(&Adjustment::COLOR, cx));
                ("develop-color", children)
            }
            S::ColorMixer => ("develop-mixer", self.color_mixer_section(cx)),
            S::ColorGrading => ("develop-grading", self.color_grading_section(cx)),
            S::Detail => ("develop-detail", self.detail_section(cx)),
            S::Lens => ("develop-lens", self.lens_section(cx)),
            // Frames has its own rail tool, not a panel section.
            S::Frames => ("develop-frames", Vec::new()),
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
