//! History in the tools column: the selected photo's changes, "Original"
//! at the top and the latest at the bottom. Hovering a step previews the
//! photo as it looked there; clicking steps back (or forward) to it.

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

    /// Makes a history row clickable when it carries a Develop state, and
    /// shows the photo as it looked there while the pointer is over it
    /// (Settings ▸ Develop ▸ Presets ▸ "Preview on hover" also covers
    /// this). Changes without a state (flags, ratings) stay as plain,
    /// muted rows.
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
        let ix = id.1;
        div()
            .id(id)
            .cursor_pointer()
            .hover(|el| el.bg(theme.surface_elevated))
            .child(row)
            .on_hover(cx.listener(move |this, hovered: &bool, _w, cx| {
                if *hovered {
                    this.preview_history(ix, develop, cx);
                } else {
                    this.end_history_preview(ix, cx);
                }
            }))
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                // Stepping through history doesn't add to it, but it can
                // still be undone.
                this.history_hover = None;
                this.preview_override = None;
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

    /// Shows the selected photo as it looked right after history entry
    /// `ix` (`usize::MAX` for Original), without changing it.
    fn preview_history(&mut self, ix: usize, develop: DevelopSettings, cx: &mut Context<Self>) {
        if !self.settings.develop.preset_hover_preview {
            return;
        }
        self.history_hover = Some(ix);
        self.preview_override = Some(develop);
        self.refresh_live_preview(cx);
        cx.notify();
    }

    /// The pointer left history entry `ix`'s row: back to the photo's own
    /// current look (unless another step is already being previewed).
    fn end_history_preview(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.history_hover != Some(ix) {
            return;
        }
        self.history_hover = None;
        if self.preview_override.take().is_some() {
            self.refresh_live_preview(cx);
            cx.notify();
        }
    }
}
