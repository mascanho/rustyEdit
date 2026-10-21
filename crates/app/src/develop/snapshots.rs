//! Snapshots in the tools column: named, saved states of the selected
//! photo's Develop settings, to keep alternative edits and return to them.

use gpui::{AnyElement, ClickEvent, Context};
use rustyedit_core::{next_snapshot_name, Snapshot};

use super::{
    tools::{add_row, list_row, name_field},
    HistoryNote,
};
use crate::{
    history_panel::{now_secs, relative_time},
    workspace::{TextTarget, Workspace},
};

impl Workspace {
    pub(super) fn snapshots_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(asset) = self.catalog.selected_asset() else {
            return Vec::new();
        };

        let mut rows = vec![match self.draft_for(TextTarget::SnapshotName) {
            Some(draft) => name_field(
                &theme,
                draft,
                &format!("{}, then \u{23CE}", next_snapshot_name(&asset.snapshots)),
            ),
            None => add_row(
                "develop-snapshot-take",
                &theme,
                "Take snapshot",
                cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.begin_text_draft(TextTarget::SnapshotName, window, cx);
                }),
            ),
        }];

        let now = now_secs();
        // Newest first, like History.
        for (ix, snapshot) in asset.snapshots.iter().enumerate().rev() {
            let snapshot = snapshot.clone();
            rows.push(list_row(
                ("develop-snapshot", ix),
                &theme,
                snapshot.name.clone(),
                Some(relative_time(now, snapshot.at)),
                snapshot.develop == asset.develop,
                cx.listener(move |this, _: &ClickEvent, _w, cx| this.apply_snapshot(&snapshot, cx)),
                Some(Box::new(cx.listener(
                    move |this, _: &ClickEvent, _w, cx| {
                        this.delete_snapshot(ix, cx);
                    },
                ))),
            ));
        }
        rows
    }

    /// Saves the selected photo's current Develop settings as a snapshot
    /// called `name` (numbered if blank).
    pub(crate) fn take_snapshot(&mut self, name: String, cx: &mut Context<Self>) {
        let ix = self.catalog.selected;
        let Some(asset) = self.catalog.assets.get_mut(ix) else {
            return;
        };
        let name = match name.trim() {
            "" => next_snapshot_name(&asset.snapshots),
            trimmed => trimmed.to_string(),
        };
        asset
            .snapshots
            .push(Snapshot::new(name.clone(), asset.develop));
        self.status_message = Some(format!("Took snapshot \u{201C}{name}\u{201D}").into());
        self.mark_catalog_dirty(cx);
        cx.notify();
    }

    fn apply_snapshot(&mut self, snapshot: &Snapshot, cx: &mut Context<Self>) {
        let develop = snapshot.develop;
        self.replace_develop(
            vec![self.catalog.selected],
            &format!("snapshot \u{201C}{}\u{201D}", snapshot.name),
            HistoryNote::Record {
                title: "Snapshot",
                detail: &snapshot.name,
            },
            cx,
            move |_| develop,
        );
    }

    fn delete_snapshot(&mut self, index: usize, cx: &mut Context<Self>) {
        let ix = self.catalog.selected;
        let Some(asset) = self.catalog.assets.get_mut(ix) else {
            return;
        };
        if index < asset.snapshots.len() {
            let removed = asset.snapshots.remove(index);
            self.status_message =
                Some(format!("Deleted snapshot \u{201C}{}\u{201D}", removed.name).into());
            self.mark_catalog_dirty(cx);
            cx.notify();
        }
    }
}
