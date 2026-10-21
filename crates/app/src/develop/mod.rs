//! The Develop module: the photo being edited between two columns. The
//! editing panel (histogram and sliders) takes the Inspector's place in the
//! current layout; the tools column (Navigator, Presets, Snapshots,
//! History) takes the Navigator's.
//!
//! - `source`: decoding the photo into a downscaled working copy
//! - `preview`: rendering adjustments into the live, in-memory preview
//! - `viewer`: the photo, fitted or at 100% with drag-to-pan
//! - `adjustments`: the slider sections and every Develop-settings change
//! - `curve_editor`, `color_mixer`, `color_grading`: the editing panel's
//!   graphical and tabbed sections
//! - `tools`: where the tools column goes and what it holds
//! - `navigator`, `presets`, `snapshots`, `history`: its sections

mod adjustments;
mod color_grading;
mod color_mixer;
mod curve_editor;
mod history;
mod navigator;
mod presets;
mod preview;
mod snapshots;
mod source;
mod tools;
mod viewer;

pub(crate) use adjustments::HistoryNote;
pub(crate) use preview::LivePreview;

use std::{cell::Cell, rc::Rc};

use gpui::{div, prelude::*, px, AnyElement, Bounds, Context, Pixels, Point, ScrollHandle, Window};
use rustyedit_core::{settings::DockSide, Adjustment, CurveChannel, MixerProp, Presets, Settings};
use rustyedit_ui::PanelSide;

use crate::{library::panel_group_heading, workspace::Workspace};

/// View state owned by the Develop module.
pub(crate) struct DevelopUi {
    pub(crate) tools_collapsed: bool,
    pub(crate) tools_scroll: ScrollHandle,
    /// Scroll position of the photo at 100%.
    pub(crate) viewer_scroll: ScrollHandle,
    /// Where to centre the 100% view (as fractions of the photo) once its
    /// viewport has been laid out.
    pub(crate) center_on: Cell<Option<(f32, f32)>>,
    /// Size of the photo at 100% in logical pixels, from the last render.
    pub(crate) zoomed_size: Cell<(f32, f32)>,
    /// A drag on the photo at 100%: pointer and scroll offset at its start.
    pub(crate) pan_drag: Option<(Point<Pixels>, Point<Pixels>)>,
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
}

impl DevelopUi {
    pub(crate) fn new(settings: &Settings) -> Self {
        Self {
            tools_collapsed: !settings.develop.tools_expanded_on_startup,
            tools_scroll: ScrollHandle::new(),
            viewer_scroll: ScrollHandle::new(),
            center_on: Cell::new(None),
            zoomed_size: Cell::new((0.0, 0.0)),
            pan_drag: None,
            navigator_bounds: Rc::new(Cell::new(Bounds::default())),
            navigator_dragging: false,
            presets: rustyedit_io::load_presets(),
            curve_channel: CurveChannel::default(),
            curve_drag: None,
            curve_bounds: Rc::new(Cell::new(Bounds::default())),
            mixer_prop: MixerProp::default(),
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
        if tools_side == Some(panel_side) {
            panel_children.push(panel_group_heading(&self.theme, "Tools"));
            panel_children.extend(self.develop_tools_sections(cx));
            panel_children.push(panel_group_heading(&self.theme, "Edit"));
        }
        panel_children.extend(self.develop_edit_sections(cx));

        let panel = rustyedit_ui::side_panel(
            &self.theme,
            match panel_side {
                DockSide::Left => PanelSide::Left,
                DockSide::Right => PanelSide::Right,
            },
            self.settings.layout.inspector_width as f32,
            &self.develop_panel_scroll,
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

    /// Histogram and the editing sections, broadly in the order a photo
    /// is worked on: tone, then colour, then detail and effects. Each can
    /// be collapsed.
    fn develop_edit_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        vec![
            self.render_main_section(cx),
            self.adjustment_section("develop-basic", "Basic", &Adjustment::BASIC, cx),
            self.disclosure("develop-curve", "Tone Curve", self.curve_section(cx), cx),
            self.adjustment_section("develop-color", "Color", &Adjustment::COLOR, cx),
            self.disclosure("develop-mixer", "Color Mixer", self.color_mixer_section(cx), cx),
            self.disclosure(
                "develop-grading",
                "Color Grading",
                self.color_grading_section(cx),
                cx,
            ),
            self.adjustment_section("develop-detail", "Detail", &Adjustment::DETAIL, cx),
            self.adjustment_section("develop-effects", "Effects", &Adjustment::EFFECTS, cx),
        ]
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
