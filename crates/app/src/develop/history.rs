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

        // Oldest at the top ("Original"), the latest change at the bottom,
        // with whatever lies after the current state faded: stepping back
        // keeps the later changes there to step forward to.
        let now = now_secs();
        let mut rows: Vec<AnyElement> = Vec::new();
        rows.push(self.step_target(
            ("develop-history", usize::MAX),
            original.develop,
            history_row(
                &theme,
                usize::MAX,
                &original,
                String::new(),
                at_original,
                false,
            ),
            cx,
        ));
        rows.extend(asset.history.iter().enumerate().map(|(ix, entry)| {
            let row = history_row(
                &theme,
                ix,
                entry,
                relative_time(now, entry.at),
                standing == Some(ix),
                match standing {
                    Some(current) => ix > current,
                    None => at_original,
                },
            );
            self.step_target(("develop-history", ix), entry.develop, row, cx)
        }));

        if asset.history.is_empty() {
            return vec![empty_note(&theme, "Edits will be listed here.")];
        }
        vec![div()
            .flex()
            .flex_col()
            .bg(theme.background.opacity(0.45))
            .border_1()
            .border_color(theme.border_subtle)
            .children(rows)
            .into_any_element()]
    }

    /// Makes a history row clickable when it carries a Develop state.
    /// Changes without one (flags, ratings) stay as plain, muted rows.
    fn step_target(
        &self,
        id: (&'static str, usize),
        develop: Option<DevelopSettings>,
        row: gpui::Stateful<gpui::Div>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let Some(develop) = develop else {
            // Flags, ratings and labels have no edit state to step to.
            return row.opacity(0.6).into_any_element();
        };
        div()
            .id(id)
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
