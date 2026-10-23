//! The History rail tool: a compact log of every change made to the
//! selected photo (flags, ratings, labels and each Develop adjustment), one
//! line each, oldest at the top and the latest change at the bottom, like
//! RawTherapee's History.

use rustyedit_ui::sp;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{div, prelude::*, relative, AnyElement};
use rustyedit_core::HistoryEntry;

use crate::workspace::Workspace;

impl Workspace {
    pub(crate) fn history_sections(&self) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(asset) = self.catalog.selected_asset() else {
            return vec![empty_note(&theme, "No photo selected")];
        };

        let count = asset.history.len();
        let header = div()
            .flex()
            .flex_row()
            .items_baseline()
            .justify_between()
            .gap_2()
            .pt_3()
            .pb_1()
            .child(
                div()
                    .min_w(sp(0.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(sp(11.0))
                    .text_color(theme.text)
                    .child(asset.display_name()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(sp(10.0))
                    .text_color(theme.text_faint)
                    .child(match count {
                        0 => "no changes".to_string(),
                        1 => "1 change".to_string(),
                        n => format!("{n} changes"),
                    }),
            )
            .into_any_element();

        let mut sections = vec![header];
        if asset.history.is_empty() {
            sections.push(empty_note(
                &theme,
                "Flags, ratings, labels and edits will be listed here.",
            ));
            return vec![div().px_3().children(sections).into_any_element()];
        }

        let now = now_secs();
        let last = asset.history.len() - 1;
        sections.push(
            div()
                .flex()
                .flex_col()
                .bg(theme.background.opacity(0.45))
                .border_1()
                .border_color(theme.border_subtle)
                .children(asset.history.iter().enumerate().map(|(i, entry)| {
                    history_row(
                        &theme,
                        i,
                        entry,
                        relative_time(now, entry.at),
                        i == last,
                        false,
                    )
                }))
                .into_any_element(),
        );
        vec![div().px_3().children(sections).into_any_element()]
    }
}

/// One change: a single compact line with its name on the left and its new
/// value on the right. `current` marks where the photo stands; the full
/// before / after and when it happened are in the tooltip. `dimmed` fades
/// changes that come after the current state.
pub(crate) fn history_row(
    theme: &rustyedit_ui::Theme,
    ix: usize,
    entry: &HistoryEntry,
    when: String,
    current: bool,
    dimmed: bool,
) -> gpui::Stateful<gpui::Div> {
    let theme = *theme;
    let detail = if entry.from.is_empty() {
        entry.to.clone()
    } else {
        format!("{} \u{2192} {}", entry.from, entry.to)
    };
    let tip = if when.is_empty() {
        format!("{}: {detail}", entry.label)
    } else {
        format!("{}: {detail} \u{00B7} {when}", entry.label)
    };
    div()
        .id(("history-row", ix))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_2()
        .h(sp(20.0))
        .px_2()
        .text_size(sp(11.0))
        .when(current, |el| {
            el.bg(theme.accent_muted).text_color(theme.text)
        })
        .when(!current, |el| el.text_color(theme.text_muted))
        .when(dimmed, |el| el.opacity(0.5))
        .tooltip(rustyedit_ui::tip(tip, &theme))
        .child(
            div()
                .flex_1()
                .min_w(sp(0.0))
                .overflow_hidden()
                .whitespace_nowrap()
                .child(entry.label.clone()),
        )
        .child(
            div()
                .max_w(relative(0.5))
                .flex_shrink_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_size(sp(10.0))
                .text_color(if current {
                    theme.text_muted
                } else {
                    theme.text_faint
                })
                .child(entry.to.clone()),
        )
}

pub(crate) fn empty_note(theme: &rustyedit_ui::Theme, text: &'static str) -> AnyElement {
    div()
        .py_3()
        .text_xs()
        .text_color(theme.text_faint)
        .child(text)
        .into_any_element()
}

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(crate) fn relative_time(now: u64, at: u64) -> String {
    let secs = now.saturating_sub(at);
    match secs {
        0..=4 => "just now".into(),
        5..=59 => format!("{secs}s ago"),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86_399 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86_400),
    }
}
