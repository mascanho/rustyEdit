//! The History rail tool: a newest-first log of every change made to the
//! selected photo (flags, ratings, labels and each Develop adjustment).

use rustyedit_ui::sp;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{div, prelude::*, AnyElement};
use rustyedit_core::HistoryEntry;

use crate::workspace::Workspace;

impl Workspace {
    pub(crate) fn history_sections(&self) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(asset) = self.catalog.selected_asset() else {
            return vec![empty_note(&theme, "No photo selected")];
        };

        let header = div()
            .flex()
            .flex_col()
            .gap_0p5()
            .py_3()
            .border_b_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .text_sm()
                    .text_color(theme.text)
                    .overflow_hidden()
                    .child(asset.display_name()),
            )
            .child(
                div()
                    .text_size(sp(10.0))
                    .text_color(theme.text_faint)
                    .child(match asset.history.len() {
                        0 => "No changes".to_string(),
                        1 => "1 change".to_string(),
                        n => format!("{n} changes"),
                    }),
            )
            .into_any_element();

        let mut sections = vec![header];
        if asset.history.is_empty() {
            sections.push(empty_note(
                &theme,
                "Flags, ratings, labels and edits you make to this photo will appear here.",
            ));
            return sections;
        }

        let now = now_secs();
        let last = asset.history.len() - 1;
        sections.extend(asset.history.iter().rev().enumerate().map(|(i, entry)| {
            history_row(
                &theme,
                entry,
                relative_time(now, entry.at),
                i == 0,
                i == last,
            )
        }));
        sections
    }
}

/// One change on the timeline. `highlight` lights its dot (the latest
/// change here; the photo's current state in Develop).
pub(crate) fn history_row(
    theme: &rustyedit_ui::Theme,
    entry: &HistoryEntry,
    when: String,
    highlight: bool,
    is_last: bool,
) -> AnyElement {
    let theme = *theme;
    // A timeline: a dot per change joined by a faint line.
    let rail = div()
        .flex()
        .flex_col()
        .items_center()
        .w(sp(12.0))
        .flex_shrink_0()
        .pt(sp(4.0))
        .child(
            div()
                .size(sp(9.0))
                .rounded_full()
                .bg(if highlight { theme.accent } else { theme.border })
                .when(highlight, |el| {
                    el.border_2().border_color(theme.accent.opacity(0.35))
                }),
        )
        .when(!is_last, |el| {
            el.child(div().flex_1().w(sp(1.0)).mt_1().bg(theme.border_subtle))
        });

    div()
        .flex()
        .flex_row()
        .gap_2()
        .pt_2()
        .child(rail)
        .child(
            div()
                .flex_1()
                .min_w(sp(0.0))
                .flex()
                .flex_col()
                .gap_0p5()
                .pb_2()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .items_baseline()
                        .child(
                            div()
                                .text_size(sp(11.5))
                                .text_color(theme.text)
                                .child(entry.label.clone()),
                        )
                        .child(
                            div()
                                .text_size(sp(9.5))
                                .text_color(theme.text_faint)
                                .child(when),
                        ),
                )
                .child(
                    div()
                        .text_size(sp(10.5))
                        .text_color(theme.text_muted)
                        .child(if entry.from.is_empty() {
                            entry.to.clone()
                        } else {
                            format!("{} \u{2192} {}", entry.from, entry.to)
                        }),
                ),
        )
        .into_any_element()
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
