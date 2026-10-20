//! Stacks: photos grouped as one burst/variant set, shown collapsed as a
//! single representative with a count badge. Stacks can be made by hand from
//! the selection or automatically by capture time. Collapsing is handled by
//! the catalog's visible index (see `rustyedit_core::Catalog`), so every view
//! and the filmstrip obey it automatically.

use gpui::{Context, Window};
use rustyedit_core::{AssetState, Flag};

use crate::{
    actions::{AutoStack, StackSelected, ToggleStack, Unstack},
    workspace::Workspace,
};

/// Photos captured within this many seconds of each other are auto-stacked.
const AUTO_STACK_WINDOW_SECS: i64 = 1;

impl Workspace {
    /// Groups the current multi-selection into one stack.
    pub fn stack_selected(&mut self, _: &StackSelected, _w: &mut Window, cx: &mut Context<Self>) {
        let targets = self.target_indices();
        if targets.len() < 2 {
            self.status_message = Some("Select at least two photos to stack.".into());
            cx.notify();
            return;
        }
        let id = self.catalog.max_stack_id() + 1;
        let cover = self.best_cover(&targets);
        let mut changes: Vec<(usize, AssetState, AssetState)> = Vec::new();
        for &ix in &targets {
            let before = self.catalog.assets[ix].state();
            if let Some(asset) = self.catalog.assets.get_mut(ix) {
                asset.stack_id = Some(id);
                asset.stack_cover = ix == cover;
            }
            changes.push((ix, before, self.catalog.assets[ix].state()));
        }
        self.push_undo("Stack photos", changes, false);
        self.catalog.set_stack_collapsed(id, false);
        self.catalog.refresh();
        self.mark_catalog_dirty(cx);
        self.status_message = Some(format!("Stacked {} photos.", targets.len()).into());
        self.on_selection_changed(cx);
    }

    /// Groups the visible photos into stacks wherever their capture times are
    /// within a second of each other.
    pub fn auto_stack(&mut self, _: &AutoStack, _w: &mut Window, cx: &mut Context<Self>) {
        let mut candidates: Vec<usize> = self
            .catalog
            .visible_indices()
            .iter()
            .copied()
            .filter(|&ix| {
                let asset = &self.catalog.assets[ix];
                asset.meta.captured > 0 && asset.stack_id.is_none()
            })
            .collect();
        if candidates.len() < 2 {
            self.status_message = Some("Not enough unstacked photos with capture times.".into());
            cx.notify();
            return;
        }
        candidates.sort_unstable_by_key(|&ix| (self.catalog.assets[ix].meta.captured, ix));

        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut current: Vec<usize> = Vec::new();
        for ix in candidates {
            let captured = self.catalog.assets[ix].meta.captured;
            let contiguous = current.last().is_some_and(|&last| {
                captured - self.catalog.assets[last].meta.captured <= AUTO_STACK_WINDOW_SECS
            });
            if current.is_empty() || contiguous {
                current.push(ix);
            } else {
                if current.len() > 1 {
                    groups.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
                current.push(ix);
            }
        }
        if current.len() > 1 {
            groups.push(current);
        }

        if groups.is_empty() {
            self.status_message = Some("No bursts found in this view.".into());
            cx.notify();
            return;
        }

        let mut next_id = self.catalog.max_stack_id() + 1;
        let mut changes: Vec<(usize, AssetState, AssetState)> = Vec::new();
        let mut stacked = 0;
        for group in groups {
            let id = next_id;
            next_id += 1;
            let cover = self.best_cover(&group);
            for &ix in &group {
                let before = self.catalog.assets[ix].state();
                if let Some(asset) = self.catalog.assets.get_mut(ix) {
                    asset.stack_id = Some(id);
                    asset.stack_cover = ix == cover;
                }
                changes.push((ix, before, self.catalog.assets[ix].state()));
                stacked += 1;
            }
        }
        self.push_undo("Auto-stack", changes, false);
        self.catalog.refresh();
        self.mark_catalog_dirty(cx);
        self.status_message = Some(format!("Auto-stacked {stacked} photos.").into());
        self.on_selection_changed(cx);
    }

    /// Removes every stack touched by the selection, restoring its members.
    pub fn unstack_selected(&mut self, _: &Unstack, _w: &mut Window, cx: &mut Context<Self>) {
        let mut ids: Vec<u64> = self
            .target_indices()
            .into_iter()
            .filter_map(|ix| self.catalog.assets[ix].stack_id)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() {
            self.status_message = Some("The selection is not stacked.".into());
            cx.notify();
            return;
        }
        let members: Vec<usize> = ids
            .iter()
            .flat_map(|&id| self.catalog.stack_members(id))
            .collect();
        let mut changes: Vec<(usize, AssetState, AssetState)> = Vec::new();
        for ix in members {
            let before = self.catalog.assets[ix].state();
            if let Some(asset) = self.catalog.assets.get_mut(ix) {
                asset.stack_id = None;
                asset.stack_cover = false;
            }
            changes.push((ix, before, self.catalog.assets[ix].state()));
        }
        self.push_undo("Unstack", changes, false);
        self.catalog.refresh();
        self.mark_catalog_dirty(cx);
        self.status_message = Some("Stack removed.".into());
        self.on_selection_changed(cx);
    }

    /// Collapses or expands the selected photo's stack.
    pub fn toggle_stack(&mut self, _: &ToggleStack, _w: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self
            .catalog
            .selected_asset()
            .and_then(|asset| asset.stack_id)
        else {
            self.status_message = Some("This photo is not in a stack.".into());
            cx.notify();
            return;
        };
        self.catalog.toggle_stack_collapsed(id);
        self.on_selection_changed(cx);
    }

    /// The stack member that best represents the set: highest rating, then a
    /// pick, then the earliest in catalog order.
    fn best_cover(&self, members: &[usize]) -> usize {
        let mut best = members[0];
        for &ix in &members[1..] {
            let candidate = &self.catalog.assets[ix];
            let current = &self.catalog.assets[best];
            let better = candidate.rating.value() > current.rating.value()
                || (candidate.rating.value() == current.rating.value()
                    && candidate.flag == Flag::Picked
                    && current.flag != Flag::Picked);
            if better {
                best = ix;
            }
        }
        best
    }
}
