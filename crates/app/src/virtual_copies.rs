//! Virtual copies: extra catalog entries for the same file, each with its
//! own edits, flags and metadata (a colour and a black-and-white version,
//! say) without duplicating the original. Copies are stacked with their
//! original and named "Copy 1", "Copy 2", ...
//!
//! Removing a copy only drops that entry; it never touches the file or the
//! other versions. Removing the original removes the file from the
//! catalog, copies included.

use std::collections::HashSet;

use gpui::{Context, Window};

use crate::{
    actions::CreateVirtualCopy,
    workspace::{Mode, Workspace},
};

impl Workspace {
    pub fn create_virtual_copy(
        &mut self,
        _: &CreateVirtualCopy,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.create_virtual_copies(cx);
    }

    /// Makes a virtual copy of each target photo and selects the (last)
    /// new copy. New copies go at the end of the catalog list, so nothing
    /// that refers to photos by position (undo, the selection) shifts.
    pub(crate) fn create_virtual_copies(&mut self, cx: &mut Context<Self>) {
        if self.is_importing || self.mode == Mode::Export {
            return;
        }
        let targets: Vec<usize> = self
            .target_indices()
            .into_iter()
            .filter(|&ix| self.catalog.assets.get(ix).is_some_and(|a| !a.missing))
            .collect();
        let mut created = Vec::new();
        for ix in targets {
            if let Some(copy) = self.catalog.add_virtual_copy(ix) {
                created.push(copy);
                let master = self.catalog.master_of(ix);
                self.keep_stack_in_undo(master);
            }
        }
        let Some(&last) = created.last() else {
            return;
        };
        self.facets = crate::filters_panel::Facets::build(&self.catalog);
        self.mark_catalog_dirty(cx);
        self.select_asset(last, cx);
        self.status_message = Some(match created.len() {
            1 => format!(
                "Created {}.",
                self.catalog.assets[last].display_name()
            ),
            n => format!("Created {n} virtual copies."),
        }
        .into());
        cx.notify();
    }

    /// Takes the virtual copies among `indices` out of the catalog (the
    /// file and every other version stay). Returns how many went.
    pub(crate) fn remove_virtual_copies(
        &mut self,
        indices: &[usize],
        cx: &mut Context<Self>,
    ) -> usize {
        let ids: HashSet<u64> = indices
            .iter()
            .filter_map(|&ix| self.catalog.assets.get(ix))
            .filter(|a| a.is_virtual_copy())
            .map(|a| a.id)
            .collect();
        if ids.is_empty() {
            return 0;
        }
        self.finish_removal_where(|asset| ids.contains(&asset.id), cx)
    }
}
