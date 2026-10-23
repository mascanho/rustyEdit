//! Pop-up notifications ("toasts") in the window's corner, and importing by
//! dropping folders on the window.
//!
//! Toasts: every message the app puts in the status bar also pops up for a
//! few seconds, unless it's progress ("Scanning…"), so confirmations and
//! problems can't be missed. A message about something just recorded in
//! the undo history gets an Undo button. Problems stay up longer. Off in
//! Settings ▸ Appearance ▸ Pop-up notifications.
//!
//! Drops: folders dropped on the window are added to the catalog and
//! scanned, like Import; dropped photos bring their folder in and are
//! selected.

use std::{path::PathBuf, time::Duration};

use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context, SharedString};
use rustyedit_ui::Theme;

use crate::{actions::Undo, workspace::Workspace};

/// Most toasts on screen at once; older ones make way.
const MAX_TOASTS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToastKind {
    Success,
    Info,
    Problem,
}

impl ToastKind {
    /// What a status message is, from its wording.
    fn of(text: &str) -> Self {
        let lower = text.to_lowercase();
        let problem = ["couldn't", "can't", "cannot", "failed", "error", "unable"];
        let info = ["no ", "nothing", "select ", "wait ", "turn on", "smart culling is off", "click "];
        if problem.iter().any(|w| lower.starts_with(w) || lower.contains(&format!(" {w}"))) {
            ToastKind::Problem
        } else if info.iter().any(|w| lower.starts_with(w)) {
            ToastKind::Info
        } else {
            ToastKind::Success
        }
    }

    fn duration(&self, undo: bool) -> Duration {
        match (self, undo) {
            (ToastKind::Problem, _) => Duration::from_secs(7),
            (_, true) => Duration::from_secs(6),
            (ToastKind::Info, _) => Duration::from_secs(4),
            (ToastKind::Success, _) => Duration::from_millis(3500),
        }
    }
}

pub(crate) struct Toast {
    id: u64,
    text: SharedString,
    kind: ToastKind,
    undo: bool,
}

/// Toasts on screen and what's been shown.
#[derive(Default)]
pub(crate) struct Toasts {
    items: Vec<Toast>,
    next_id: u64,
    /// The status message last turned into a toast (or skipped).
    last_status: Option<SharedString>,
}

impl Toasts {
    /// Starts out having "seen" `status`, so the message the app opens
    /// with doesn't pop up.
    pub(crate) fn starting_with(status: Option<SharedString>) -> Self {
        Self {
            last_status: status,
            ..Default::default()
        }
    }
}

/// Progress messages ("Scanning…") stay in the status bar only.
fn is_progress(text: &str) -> bool {
    let text = text.trim_end();
    text.ends_with('\u{2026}') || text.ends_with("...")
}

impl Workspace {
    /// Turns a new status message into a toast. Called while rendering, so
    /// every way a message is set is covered.
    pub(crate) fn sync_toasts(&mut self, cx: &mut Context<Self>) {
        if self.status_message == self.toasts.last_status {
            return;
        }
        self.toasts.last_status = self.status_message.clone();
        if !self.settings.appearance.notifications {
            return;
        }
        let Some(text) = self.status_message.clone() else {
            return;
        };
        if text.trim().is_empty() || is_progress(&text) {
            return;
        }
        let kind = ToastKind::of(&text);
        let undo = kind != ToastKind::Problem && self.undo_just_recorded();
        let id = self.toasts.next_id;
        self.toasts.next_id += 1;
        self.toasts.items.push(Toast {
            id,
            text,
            kind,
            undo,
        });
        if self.toasts.items.len() > MAX_TOASTS {
            self.toasts.items.remove(0);
        }
        let duration = kind.duration(undo);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(duration).await;
            this.update(cx, |this, cx| this.dismiss_toast(id, cx)).ok();
        })
        .detach();
    }

    fn dismiss_toast(&mut self, id: u64, cx: &mut Context<Self>) {
        let before = self.toasts.items.len();
        self.toasts.items.retain(|t| t.id != id);
        if self.toasts.items.len() != before {
            cx.notify();
        }
    }

    /// The toasts, stacked in the bottom-right corner above the status bar.
    pub(crate) fn render_toasts(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.toasts.items.is_empty() {
            return None;
        }
        let theme = self.theme;
        let cards: Vec<AnyElement> = self
            .toasts
            .items
            .iter()
            .map(|toast| toast_card(toast, &theme, cx))
            .collect();
        Some(
            div()
                .absolute()
                .right(px(16.0))
                .bottom(px(rustyedit_ui::STATUS_BAR_HEIGHT + 12.0))
                .flex()
                .flex_col()
                .items_end()
                .gap_2()
                .children(cards)
                .into_any_element(),
        )
    }

    // --- Dropping folders -----------------------------------------------------

    /// Imports what was dropped on the window: folders as they are, photos
    /// by their folder (selecting the first).
    pub(crate) fn import_dropped(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if self.is_importing {
            self.status_message = Some("Wait for the current import to finish.".into());
            cx.notify();
            return;
        }
        let mut folders: Vec<PathBuf> = Vec::new();
        let mut select = None;
        for path in paths {
            let folder = if path.is_dir() {
                path
            } else if path.is_file() {
                select.get_or_insert_with(|| path.clone());
                match path.parent() {
                    Some(parent) => parent.to_path_buf(),
                    None => continue,
                }
            } else {
                continue;
            };
            if !folders.contains(&folder) {
                folders.push(folder);
            }
        }
        if folders.is_empty() {
            self.status_message = Some("Drop a folder (or photos) to import it.".into());
            cx.notify();
            return;
        }
        let mut added = false;
        for folder in &folders {
            added |= self.catalog_def.add_folder(folder);
        }
        if added {
            self.save_catalog_def();
        }
        // Like Import: go to the first new photo, or the dropped one.
        self.import_append = select.is_none();
        self.open_catalog(select, cx);
    }
}

fn toast_card(toast: &Toast, theme: &Theme, cx: &mut Context<Workspace>) -> AnyElement {
    let theme = *theme;
    let id = toast.id;
    let colour = match toast.kind {
        ToastKind::Success => theme.pick,
        ToastKind::Info => theme.accent,
        ToastKind::Problem => theme.danger,
    };
    let glyph = match toast.kind {
        ToastKind::Success => "\u{2713}",
        ToastKind::Info => "i",
        ToastKind::Problem => "!",
    };
    div()
        .id(SharedString::from(format!("toast-{id}")))
        // Clicks on a toast are its own.
        .occlude()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .max_w(px(420.0))
        .min_h(px(36.0))
        .pl_2()
        .pr_1()
        .py_1p5()
        .rounded_lg()
        .bg(theme.surface_elevated)
        .border_1()
        .border_color(theme.border)
        .shadow_lg()
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .size(px(18.0))
                .rounded_full()
                .bg(colour.opacity(0.18))
                .text_size(px(10.5))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(colour)
                .child(glyph),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(12.0))
                .text_color(theme.text)
                .child(toast.text.clone()),
        )
        .when(toast.undo, |el| {
            el.child(
                div()
                    .id(SharedString::from(format!("toast-undo-{id}")))
                    .flex_shrink_0()
                    .px_2()
                    .py_0p5()
                    .rounded_md()
                    .text_size(px(11.5))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.accent)
                    .cursor_pointer()
                    .hover(|el| el.bg(theme.accent_muted))
                    .child("Undo")
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.dismiss_toast(id, cx);
                        this.undo(&Undo, window, cx);
                    })),
            )
        })
        .child(
            div()
                .id(SharedString::from(format!("toast-close-{id}")))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .size(px(20.0))
                .rounded_md()
                .text_size(px(10.0))
                .text_color(theme.text_faint)
                .cursor_pointer()
                .hover(|el| el.bg(theme.background).text_color(theme.text))
                .child("\u{2715}")
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| this.dismiss_toast(id, cx))),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_sorted_by_wording() {
        assert_eq!(ToastKind::of("Couldn't save catalog: disk full"), ToastKind::Problem);
        assert_eq!(ToastKind::of("Export failed for 2 photos"), ToastKind::Problem);
        assert_eq!(ToastKind::of("No soft photos found."), ToastKind::Info);
        assert_eq!(ToastKind::of("Synced 3 sections to 5 photos."), ToastKind::Success);
        assert!(is_progress("Scanning My Catalog\u{2026}"));
        assert!(!is_progress("Edits copied."));
    }
}
