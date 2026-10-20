//! Undo / redo for everything the Library and Develop modules change on a
//! photo (flag, rating, colour label, develop settings). Each entry stores
//! before/after snapshots per affected photo, so one entry can undo a whole
//! multi-photo action.

use std::time::{Duration, Instant};

use gpui::{Context, Window};
use rustyedit_core::AssetState;

use crate::{
    actions::{Redo, Undo},
    workspace::{Mode, Workspace},
};

const MAX_ENTRIES: usize = 200;
/// Edits to the same photo under one label inside this window (a slider
/// drag) merge into a single undo step.
const COALESCE: Duration = Duration::from_millis(1500);

pub(crate) struct UndoEntry {
    label: String,
    at: Instant,
    /// `(asset index, state before, state after)`.
    changes: Vec<(usize, AssetState, AssetState)>,
}

impl Workspace {
    /// Records a completed change. Changes that left every photo as it was
    /// are dropped.
    pub(crate) fn push_undo(
        &mut self,
        label: impl Into<String>,
        changes: Vec<(usize, AssetState, AssetState)>,
        coalesce: bool,
    ) {
        let changes: Vec<_> = changes.into_iter().filter(|(_, b, a)| b != a).collect();
        if changes.is_empty() {
            return;
        }
        let label = label.into();
        self.redo_stack.clear();

        if coalesce {
            if let Some(top) = self.undo_stack.last_mut() {
                if top.label == label
                    && top.changes.len() == 1
                    && changes.len() == 1
                    && top.changes[0].0 == changes[0].0
                    && top.at.elapsed() < COALESCE
                {
                    top.changes[0].2 = changes[0].2;
                    top.at = Instant::now();
                    // Dragged back to where it started: nothing to undo.
                    if top.changes[0].1 == top.changes[0].2 {
                        self.undo_stack.pop();
                    }
                    return;
                }
            }
        }
        self.undo_stack.push(UndoEntry {
            label,
            at: Instant::now(),
            changes,
        });
        if self.undo_stack.len() > MAX_ENTRIES {
            self.undo_stack.remove(0);
        }
    }

    pub fn undo(&mut self, _: &Undo, _w: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.undo_stack.pop() else {
            self.status_message = Some("Nothing to undo.".into());
            cx.notify();
            return;
        };
        self.apply_history_entry(&entry, true, cx);
        self.status_message = Some(format!("Undid {}", entry.label).into());
        self.redo_stack.push(entry);
        cx.notify();
    }

    pub fn redo(&mut self, _: &Redo, _w: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.redo_stack.pop() else {
            self.status_message = Some("Nothing to redo.".into());
            cx.notify();
            return;
        };
        self.apply_history_entry(&entry, false, cx);
        self.status_message = Some(format!("Redid {}", entry.label).into());
        self.undo_stack.push(entry);
        cx.notify();
    }

    fn apply_history_entry(&mut self, entry: &UndoEntry, backwards: bool, cx: &mut Context<Self>) {
        for (ix, before, after) in &entry.changes {
            let state = if backwards { before } else { after };
            self.catalog.restore_state(*ix, state);
        }
        self.mark_catalog_dirty(cx);

        // Take the user to what changed so the undo is visible.
        if let Some(&(ix, ..)) = entry.changes.first() {
            if self.catalog.visible_position(ix).is_some() && ix != self.catalog.selected {
                self.select_asset(ix, cx);
            }
        }
        if self.mode == Mode::Develop {
            self.request_preview_update(cx);
        }
    }
}
