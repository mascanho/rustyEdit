//! Undo / redo for everything the Library and Develop modules change on a
//! photo (flag, rating, colour label, develop settings). Each entry stores
//! before/after snapshots per affected photo, so one entry can undo a whole
//! multi-photo action.

use std::time::{Duration, Instant};

use gpui::{Context, Window};
use rustyedit_core::{AssetState, DevelopSettings};

use crate::{
    actions::{Redo, Undo},
    workspace::Workspace,
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
                let same_photos = top.changes.len() == changes.len()
                    && top.changes.iter().zip(&changes).all(|(a, b)| a.0 == b.0);
                if top.label == label && same_photos && top.at.elapsed() < COALESCE {
                    for (kept, new) in top.changes.iter_mut().zip(changes) {
                        kept.2 = new.2;
                    }
                    top.at = Instant::now();
                    // Dragged back to where it started: nothing to undo.
                    if top.changes.iter().all(|(_, before, after)| before == after) {
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

    /// A slider drag changes one photo's Develop settings many times a
    /// second under one label. When the newest undo step is such a drag on
    /// photo `ix`, folds the new `develop` into it in place and returns
    /// `true`, without snapshotting the whole photo again for every step.
    /// Returns `false` when a new step has to be recorded instead.
    pub(crate) fn coalesce_develop_undo(
        &mut self,
        label: &str,
        ix: usize,
        develop: DevelopSettings,
    ) -> bool {
        let folded = fold_develop(&mut self.undo_stack, label, ix, develop);
        if folded {
            self.redo_stack.clear();
        }
        folded
    }

    /// Follows photos to their new positions after some were removed from
    /// the catalog (`moved[old] = new`, `None` if removed), dropping the
    /// parts of undo and redo steps that were about removed photos.
    pub(crate) fn remap_undo(&mut self, moved: &[Option<usize>]) {
        for stack in [&mut self.undo_stack, &mut self.redo_stack] {
            for entry in stack.iter_mut() {
                entry.changes = std::mem::take(&mut entry.changes)
                    .into_iter()
                    .filter_map(|(ix, before, after)| {
                        Some((moved.get(ix).copied().flatten()?, before, after))
                    })
                    .collect();
            }
            stack.retain(|entry| !entry.changes.is_empty());
        }
    }

    /// Photo `ix` was just stacked by something that isn't itself undoable
    /// (making a virtual copy). Earlier undo steps for it recorded it as
    /// unstacked; give them its new stack so undoing them doesn't take it
    /// out of the stack.
    pub(crate) fn keep_stack_in_undo(&mut self, ix: usize) {
        let Some(asset) = self.catalog.assets.get(ix) else {
            return;
        };
        let (stack, cover) = (asset.stack_id, asset.stack_cover);
        for entry in self.undo_stack.iter_mut().chain(self.redo_stack.iter_mut()) {
            for (at, before, after) in &mut entry.changes {
                if *at != ix {
                    continue;
                }
                for state in [before, after] {
                    if state.stack_id.is_none() {
                        state.stack_id = stack;
                        state.stack_cover = cover;
                    }
                }
            }
        }
    }

    /// Whether the newest undo step was recorded just now (so a message
    /// about it can offer Undo).
    pub(crate) fn undo_just_recorded(&self) -> bool {
        self.undo_stack
            .last()
            .is_some_and(|entry| entry.at.elapsed() < Duration::from_millis(1500))
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

        // Rotation is baked into the cached thumbnail, so undo/redo of a
        // rotate has to rebuild those previews as well.
        let rotated: Vec<usize> = entry
            .changes
            .iter()
            .filter(|(_, before, after)| {
                before.develop.rotation_degrees != after.develop.rotation_degrees
            })
            .map(|(ix, ..)| *ix)
            .collect();
        if !rotated.is_empty() {
            self.refresh_asset_thumbnails(&rotated, cx);
        }

        // Take the user to what changed so the undo is visible.
        if let Some(&(ix, ..)) = entry.changes.first() {
            if self.catalog.visible_position(ix).is_some() && ix != self.catalog.selected {
                self.select_asset(ix, cx);
            }
        }
        self.refresh_live_preview(cx);
    }
}

/// The folding behind [`Workspace::coalesce_develop_undo`].
fn fold_develop(stack: &mut Vec<UndoEntry>, label: &str, ix: usize, develop: DevelopSettings) -> bool {
    let Some(top) = stack.last_mut() else {
        return false;
    };
    if top.label != label
        || top.at.elapsed() >= COALESCE
        || top.changes.len() != 1
        || top.changes[0].0 != ix
    {
        return false;
    }
    top.changes[0].2.develop = develop;
    top.at = Instant::now();
    // Dragged back to where it started: nothing to undo.
    if top.changes[0].1 == top.changes[0].2 {
        stack.pop();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyedit_core::Asset;

    fn drag_start() -> (Vec<UndoEntry>, AssetState) {
        let mut asset = Asset::new(0, "a.jpg".into());
        let before = asset.state();
        asset.develop.exposure = 5.0;
        let entry = UndoEntry {
            label: "Exposure".into(),
            at: Instant::now(),
            changes: vec![(3, before.clone(), asset.state())],
        };
        (vec![entry], before)
    }

    #[test]
    fn steps_of_a_drag_fold_into_one_entry() {
        let (mut stack, before) = drag_start();
        for v in [10.0, 20.0, 30.0] {
            let develop = DevelopSettings { exposure: v, ..Default::default() };
            assert!(fold_develop(&mut stack, "Exposure", 3, develop));
        }
        assert_eq!(stack.len(), 1);
        assert_eq!(stack[0].changes[0].2.develop.exposure, 30.0);
        assert_eq!(stack[0].changes[0].1, before);
    }

    #[test]
    fn other_labels_photos_and_old_steps_start_a_new_entry() {
        let (mut stack, _) = drag_start();
        let develop = DevelopSettings::default();
        assert!(!fold_develop(&mut stack, "Contrast", 3, develop));
        assert!(!fold_develop(&mut stack, "Exposure", 4, develop));
        stack[0].at = Instant::now() - COALESCE - Duration::from_millis(10);
        assert!(!fold_develop(&mut stack, "Exposure", 3, develop));
        assert!(!fold_develop(&mut Vec::new(), "Exposure", 3, develop));
    }

    #[test]
    fn dragging_back_to_the_start_leaves_nothing_to_undo() {
        let (mut stack, _) = drag_start();
        assert!(fold_develop(&mut stack, "Exposure", 3, DevelopSettings::default()));
        assert!(stack.is_empty());
    }
}
