//! History in the tools column: the selected photo's changes, newest
//! first, where clicking an edit steps the photo back (or forward) to the
//! state right after it. "Original" at the bottom returns it to unedited.

use gpui::{div, prelude::*, AnyElement, ClickEvent, Context};
use rustyedit_core::{DevelopSettings, HistoryEntry};

use super::HistoryNote;
use crate::{
    history_panel::{empty_note, history_row, now_secs, relative_time},
    workspace::Workspace,
};

impl Workspace {
    pub(super) fn develop_history_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(asset) = self.catalog.selected_asset() else {
            return Vec::new();
        };
        let current = asset.develop;

        // The newest edit matching the photo as it is now is where it
        // stands; with none, it stands at Original when unedited.
        let standing = asset
            .history
            .iter()
            .rposition(|entry| entry.develop == Some(current));
        let original = HistoryEntry {
            at: 0,
            label: "Original".to_string(),
            from: String::new(),
            to: "As imported".to_string(),
            develop: Some(DevelopSettings::default()),
        };
        let at_original = standing.is_none() && !current.is_edited();

        let now = now_secs();
        let mut rows: Vec<AnyElement> = asset
            .history
            .iter()
            .enumerate()
            .rev()
            .map(|(ix, entry)| {
                let row = history_row(
                    &theme,
                    entry,
                    relative_time(now, entry.at),
                    standing == Some(ix),
                    false,
                );
                self.step_target(("develop-history", ix), entry.develop, row, cx)
            })
            .collect();
        rows.push(self.step_target(
            ("develop-history", usize::MAX),
            original.develop,
            history_row(&theme, &original, String::new(), at_original, true),
            cx,
        ));

        if asset.history.is_empty() {
            rows.insert(
                0,
                empty_note(&theme, "Edits you make to this photo will appear here."),
            );
        }
        rows
    }

    /// Makes a history row clickable when it carries a Develop state.
    /// Changes without one (flags, ratings) stay as plain, muted rows.
    fn step_target(
        &self,
        id: (&'static str, usize),
        develop: Option<DevelopSettings>,
        row: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let Some(develop) = develop else {
            return div().opacity(0.6).child(row).into_any_element();
        };
        div()
            .id(id)
            .rounded_md()
            .px_1()
            .cursor_pointer()
            .hover(|el| el.bg(theme.surface_elevated))
            .child(row)
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                // Stepping through history doesn't add to it, but it can
                // still be undone.
                this.replace_develop(
                    vec![this.catalog.selected],
                    "history step",
                    HistoryNote::Skip,
                    cx,
                    move |_| develop,
                );
            }))
            .into_any_element()
    }
}
