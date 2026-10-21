//! Moving edits between photos: copy / paste, "Previous" (the photo viewed
//! before this one), and reset. Shared by the panel footers and the
//! context menu.
//!
//! Pasting carries the look (tone, colour, detail, effects, gradients) but
//! keeps each photo's own rotation, flips and retouch spots, which only
//! make sense on the photo they were made for.

use gpui::Context;
use rustyedit_core::DevelopSettings;

use crate::{develop::HistoryNote, workspace::Workspace};

impl Workspace {
    pub(crate) fn copy_edits(&mut self, cx: &mut Context<Self>) {
        self.edit_clipboard = self.catalog.selected_asset().map(|a| a.develop);
        self.status_message = Some("Edits copied.".into());
        cx.notify();
    }

    /// Pastes the copied edits onto every target photo.
    pub(crate) fn paste_edits(&mut self, cx: &mut Context<Self>) {
        let Some(copied) = self.edit_clipboard else {
            return;
        };
        self.apply_look(copied, "paste edits", "Paste edits", "Pasted", cx);
    }

    /// The photo selected before the current one, if it's still around.
    pub(crate) fn previous_photo_edits(&self) -> Option<DevelopSettings> {
        let id = self.previous_photo?;
        if self.catalog.selected_asset().is_some_and(|a| a.id == id) {
            return None;
        }
        self.catalog
            .assets
            .iter()
            .find(|asset| asset.id == id)
            .map(|asset| asset.develop)
    }

    /// "Previous": gives the target photos the previous photo's edits.
    pub(crate) fn apply_previous_edits(&mut self, cx: &mut Context<Self>) {
        let Some(previous) = self.previous_photo_edits() else {
            return;
        };
        self.apply_look(previous, "previous edits", "Previous", "Applied", cx);
    }

    /// Takes every target photo back to unedited.
    pub(crate) fn reset_edits(&mut self, cx: &mut Context<Self>) {
        self.replace_develop(
            self.target_indices(),
            "reset edits",
            HistoryNote::Record {
                title: "Reset edits",
                detail: "Original",
            },
            cx,
            |_| DevelopSettings::default(),
        );
    }

    pub(crate) fn selection_has_edits(&self) -> bool {
        self.target_indices()
            .iter()
            .any(|&ix| self.catalog.assets[ix].develop.is_edited())
    }

    /// Notes the photo being left when the selection moves, for "Previous".
    pub(crate) fn remember_previous_photo(&mut self) {
        let Some(current) = self.catalog.selected_asset().map(|a| a.id) else {
            return;
        };
        if self.current_photo != Some(current) {
            self.previous_photo = self.current_photo;
            self.current_photo = Some(current);
        }
    }

    fn apply_look(
        &mut self,
        look: DevelopSettings,
        undo_label: &str,
        title: &str,
        detail: &str,
        cx: &mut Context<Self>,
    ) {
        self.replace_develop(
            self.target_indices(),
            undo_label,
            HistoryNote::Record { title, detail },
            cx,
            move |own| DevelopSettings {
                spots: own.spots,
                ..own.with_look_of(&look)
            },
        );
    }
}
