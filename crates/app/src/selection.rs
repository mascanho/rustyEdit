//! Multi-selection, and the cull actions (flag / rate / label) that apply
//! to it. A selection of several photos always includes the primary photo
//! (`catalog.selected`); with fewer than two, actions hit just the primary.

use gpui::{Context, Modifiers, Window};
use rustyedit_core::{ColorLabel, Flag, Rating};

use crate::{
    actions::{
        ExtendDown, ExtendFirst, ExtendLast, ExtendLeft, ExtendRight, ExtendUp, SelectAll,
        SelectDown, SelectFirst, SelectLast, SelectPageDown, SelectPageUp, SelectUp,
    },
    workspace::{LibraryView, Mode, Workspace},
};

/// Where a keyboard move goes, relative to the primary photo.
#[derive(Clone, Copy)]
pub(crate) enum KeyStep {
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    First,
    Last,
}

impl KeyStep {
    /// Steps that move by grid rows.
    fn is_vertical(self) -> bool {
        matches!(
            self,
            KeyStep::Up | KeyStep::Down | KeyStep::PageUp | KeyStep::PageDown
        )
    }
}

/// One cull operation, applied with toggle semantics: if every target
/// already has the value it's cleared, otherwise it's set on all of them.
#[derive(Clone, Copy)]
pub(crate) enum CullOp {
    Flag(Flag),
    Rating(u8),
    Label(ColorLabel),
}

impl CullOp {
    fn name(&self) -> &'static str {
        match self {
            CullOp::Flag(_) => "flag",
            CullOp::Rating(_) => "rating",
            CullOp::Label(_) => "colour label",
        }
    }
}

impl Workspace {
    /// The photos cull actions apply to, in catalog order.
    pub(crate) fn target_indices(&self) -> Vec<usize> {
        let primary = self.catalog.selected;
        if self.selection.len() > 1 && self.selection.contains(&primary) {
            let mut targets: Vec<usize> = self
                .selection
                .iter()
                .copied()
                .filter(|&ix| self.catalog.visible_position(ix).is_some())
                .collect();
            targets.sort_unstable();
            if targets.len() > 1 {
                return targets;
            }
        }
        vec![primary]
    }

    pub(crate) fn is_multi_selected(&self) -> bool {
        self.target_indices().len() > 1
    }

    /// Whether `ix` is part of a multi-selection.
    pub(crate) fn in_selection(&self, ix: usize) -> bool {
        self.selection.len() > 1 && self.selection.contains(&ix)
    }

    /// Click handling shared by the grid and survey: plain click selects
    /// one photo; ⌘-click toggles it in the selection; Shift-click selects
    /// the range from the anchor.
    pub(crate) fn click_select(&mut self, ix: usize, modifiers: Modifiers, cx: &mut Context<Self>) {
        if modifiers.platform {
            if self.selection.is_empty() {
                self.selection.insert(self.catalog.selected);
            }
            if !self.selection.remove(&ix) {
                self.selection.insert(ix);
                self.catalog.select(ix);
            } else if self.catalog.selected == ix {
                // Deselecting the primary hands it to another selected photo.
                if let Some(&other) = self.selection.iter().min() {
                    self.catalog.select(other);
                }
            }
            self.selection_anchor = Some(ix);
            self.on_selection_changed(cx);
        } else if modifiers.shift {
            self.select_range_to(ix);
            self.on_selection_changed(cx);
        } else {
            self.selection_anchor = Some(ix);
            self.select_asset(ix, cx);
        }
    }

    /// Selects every visible photo between the anchor and `ix`, and makes
    /// `ix` the primary. The anchor stays put so the range can be reshaped.
    fn select_range_to(&mut self, ix: usize) {
        let anchor = self
            .selection_anchor
            .filter(|&a| self.catalog.visible_position(a).is_some())
            .unwrap_or(self.catalog.selected);
        self.selection_anchor = Some(anchor);
        let visible = self.catalog.visible_indices();
        let (a, b) = (
            self.catalog.visible_position(anchor),
            self.catalog.visible_position(ix),
        );
        if let (Some(a), Some(b)) = (a, b) {
            let (lo, hi) = (a.min(b), a.max(b));
            self.selection = visible[lo..=hi].iter().copied().collect();
        }
        self.catalog.select(ix);
    }

    /// Keyboard navigation: moves the primary photo by `step`, either
    /// replacing the selection or (`extend`) growing it from the anchor.
    /// Never wraps. Row-wise steps only mean something in the grid.
    pub(crate) fn keyboard_move(&mut self, step: KeyStep, extend: bool, cx: &mut Context<Self>) {
        let in_library = self.mode == Mode::Library;
        let in_grid = in_library && self.library_view == LibraryView::Grid;
        if (extend && !in_library) || (step.is_vertical() && !in_grid) {
            return;
        }
        let len = self.catalog.visible_indices().len();
        if len == 0 {
            return;
        }
        let pos = self
            .catalog
            .visible_position(self.catalog.selected)
            .unwrap_or(0) as isize;
        let columns = self.grid_columns() as isize;
        let last = len as isize - 1;
        let target = match step {
            KeyStep::Left => pos - 1,
            KeyStep::Right => pos + 1,
            KeyStep::Up => pos - columns,
            KeyStep::Down => pos + columns,
            KeyStep::PageUp => pos - columns * self.grid_page_rows(),
            KeyStep::PageDown => pos + columns * self.grid_page_rows(),
            KeyStep::First => 0,
            KeyStep::Last => last,
        }
        .clamp(0, last);
        let ix = self.catalog.visible_indices()[target as usize];

        if extend {
            self.select_range_to(ix);
        } else {
            self.clear_selection();
            self.selection_anchor = Some(ix);
            self.catalog.select(ix);
        }
        self.on_selection_changed(cx);
    }

    /// Whole rows that fit in the grid's viewport, for PageUp / PageDown.
    fn grid_page_rows(&self) -> isize {
        let state = self.grid_scroll.0.borrow();
        let viewport = f32::from(state.base_handle.bounds().size.height);
        let row = state
            .last_item_size
            .map(|size| f32::from(size.item.height))
            .unwrap_or(0.0);
        if row > 0.0 {
            ((viewport / row).floor() as isize).max(1)
        } else {
            1
        }
    }

    pub fn select_up(&mut self, _: &SelectUp, _w: &mut Window, cx: &mut Context<Self>) {
        self.keyboard_move(KeyStep::Up, false, cx);
    }
    pub fn select_down(&mut self, _: &SelectDown, _w: &mut Window, cx: &mut Context<Self>) {
        self.keyboard_move(KeyStep::Down, false, cx);
    }
    pub fn select_first(&mut self, _: &SelectFirst, _w: &mut Window, cx: &mut Context<Self>) {
        self.keyboard_move(KeyStep::First, false, cx);
    }
    pub fn select_last(&mut self, _: &SelectLast, _w: &mut Window, cx: &mut Context<Self>) {
        self.keyboard_move(KeyStep::Last, false, cx);
    }
    pub fn select_page_up(&mut self, _: &SelectPageUp, _w: &mut Window, cx: &mut Context<Self>) {
        self.keyboard_move(KeyStep::PageUp, false, cx);
    }
    pub fn select_page_down(
        &mut self,
        _: &SelectPageDown,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.keyboard_move(KeyStep::PageDown, false, cx);
    }
    pub fn extend_left(&mut self, _: &ExtendLeft, _w: &mut Window, cx: &mut Context<Self>) {
        self.keyboard_move(KeyStep::Left, true, cx);
    }
    pub fn extend_right(&mut self, _: &ExtendRight, _w: &mut Window, cx: &mut Context<Self>) {
        self.keyboard_move(KeyStep::Right, true, cx);
    }
    pub fn extend_up(&mut self, _: &ExtendUp, _w: &mut Window, cx: &mut Context<Self>) {
        self.keyboard_move(KeyStep::Up, true, cx);
    }
    pub fn extend_down(&mut self, _: &ExtendDown, _w: &mut Window, cx: &mut Context<Self>) {
        self.keyboard_move(KeyStep::Down, true, cx);
    }
    pub fn extend_first(&mut self, _: &ExtendFirst, _w: &mut Window, cx: &mut Context<Self>) {
        self.keyboard_move(KeyStep::First, true, cx);
    }
    pub fn extend_last(&mut self, _: &ExtendLast, _w: &mut Window, cx: &mut Context<Self>) {
        self.keyboard_move(KeyStep::Last, true, cx);
    }

    /// Runs a cull action from a control on a grid card. The card's photo
    /// becomes the target first, unless it's already part of the selection
    /// (then the whole selection is acted on, like the keyboard shortcuts).
    pub(crate) fn act_on_card(
        &mut self,
        ix: usize,
        cx: &mut Context<Self>,
        action: impl FnOnce(&mut Self, &mut Context<Self>),
    ) {
        if ix != self.catalog.selected && !self.in_selection(ix) {
            self.selection_anchor = Some(ix);
            self.select_asset(ix, cx);
        }
        action(self, cx);
    }

    pub fn select_all(&mut self, _: &SelectAll, _w: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::Library {
            return;
        }
        self.selection = self.catalog.visible_indices().iter().copied().collect();
        cx.notify();
    }

    pub(crate) fn clear_selection(&mut self) {
        self.selection.clear();
    }

    /// Applies a cull operation to every target photo as one undo step, and
    /// optionally moves on to the next photo.
    pub(crate) fn apply_cull(&mut self, op: CullOp, cx: &mut Context<Self>) {
        if self.catalog.selected_asset().is_none() {
            return;
        }
        let targets = self.target_indices();
        let all_have = targets.iter().all(|&ix| {
            let asset = &self.catalog.assets[ix];
            match op {
                CullOp::Flag(f) => asset.flag == f,
                CullOp::Rating(v) => asset.rating.value() == v,
                CullOp::Label(l) => asset.color_label == l,
            }
        });

        let mut changes = Vec::new();
        for &ix in &targets {
            let before = self.catalog.assets[ix].state();
            match op {
                CullOp::Flag(f) => self
                    .catalog
                    .assign_flag(ix, if all_have { Flag::Unflagged } else { f }),
                CullOp::Rating(v) => self
                    .catalog
                    .assign_rating(ix, Rating::new(if all_have { 0 } else { v })),
                CullOp::Label(l) => self
                    .catalog
                    .assign_color_label(ix, if all_have { ColorLabel::None } else { l }),
            }
            changes.push((ix, before, self.catalog.assets[ix].state()));
        }

        let label = if targets.len() > 1 {
            format!("{} on {} photos", op.name(), targets.len())
        } else {
            op.name().to_string()
        };
        self.push_undo(label, changes, false);
        self.mark_catalog_dirty(cx);

        // Auto-advance after a decision on a single photo (never wraps).
        let decides = matches!(op, CullOp::Flag(_) | CullOp::Rating(_));
        if decides
            && self.settings.library.auto_advance
            && targets.len() == 1
            && self.mode == Mode::Library
            && self.library_view != LibraryView::Grid
        {
            let at = self.catalog.visible_position(self.catalog.selected);
            if let Some(next) = at.and_then(|p| self.catalog.visible_indices().get(p + 1).copied())
            {
                self.select_asset(next, cx);
                return;
            }
        }
        cx.notify();
    }
}
