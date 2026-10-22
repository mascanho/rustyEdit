//! The fixed footers along the bottom of the side panels, Lightroom-style:
//!
//! - Library's Navigator dock: Import… / Export…
//! - Library's Inspector dock: Copy Edits / Paste Edits / Sync…
//! - Develop's tools column: Copy Edits / Paste Edits / Sync…
//! - Develop's editing panel: Previous / Reset
//!
//! A dock holding several panels stacks their rows.

use gpui::{AnyElement, ClickEvent, Context};
use rustyedit_core::settings::PanelKind;

use crate::{actions::GoExport, workspace::Workspace};

impl Workspace {
    /// The footer of a Library dock holding `panels`.
    pub(crate) fn library_dock_footer(
        &self,
        panels: &[PanelKind],
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let rows: Vec<_> = panels
            .iter()
            .map(|panel| match panel {
                PanelKind::Navigator => self.import_export_row(cx),
                PanelKind::Inspector => self.copy_paste_row(cx),
            })
            .collect();
        (!rows.is_empty()).then(|| rustyedit_ui::panel_footer(&self.theme, rows))
    }

    /// The footer of Develop's tools column.
    pub(crate) fn develop_tools_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        rustyedit_ui::panel_footer(&self.theme, vec![self.copy_paste_row(cx)])
    }

    /// The footer of Develop's editing panel; with the tools stacked in the
    /// same column, their Copy / Paste row too.
    pub(crate) fn develop_edit_footer(&self, stacked: bool, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = Vec::new();
        if stacked {
            rows.push(self.copy_paste_row(cx));
        }
        rows.push(self.previous_reset_row(cx));
        rustyedit_ui::panel_footer(&self.theme, rows)
    }

    fn import_export_row(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let has_photos = !self.catalog.assets.is_empty();
        vec![
            rustyedit_ui::footer_button(
                "footer-import",
                "Import\u{2026}",
                "Add a folder of photos (\u{2318}O)",
                !self.is_importing,
                &theme,
                cx.listener(|this, _: &ClickEvent, _w, cx| this.start_import(cx)),
            ),
            rustyedit_ui::footer_button(
                "footer-export",
                "Export\u{2026}",
                "Export the selected photos (E)",
                has_photos,
                &theme,
                cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.go_export(&GoExport, window, cx)
                }),
            ),
        ]
    }

    fn copy_paste_row(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let has_photo = self.catalog.selected_asset().is_some();
        let targets = self.target_indices().len();
        vec![
            rustyedit_ui::footer_button(
                "footer-copy-edits",
                "Copy Edits",
                "Copy this photo's edits",
                has_photo,
                &theme,
                cx.listener(|this, _: &ClickEvent, _w, cx| this.copy_edits(cx)),
            ),
            rustyedit_ui::footer_button(
                "footer-paste-edits",
                if targets > 1 {
                    format!("Paste to {targets}")
                } else {
                    "Paste Edits".to_string()
                },
                "Paste the copied edits (keeps each photo's rotation and spots)",
                has_photo && self.edit_clipboard.is_some(),
                &theme,
                cx.listener(|this, _: &ClickEvent, _w, cx| this.paste_edits(cx)),
            ),
            rustyedit_ui::footer_button(
                "footer-sync",
                "Sync\u{2026}",
                "Copy chosen settings from the active photo to the other selected photos (\u{21E7}\u{2318}S)",
                self.can_sync(),
                &theme,
                cx.listener(|this, _: &ClickEvent, _w, cx| this.open_sync_dialog(cx)),
            ),
        ]
    }

    fn previous_reset_row(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        vec![
            rustyedit_ui::footer_button(
                "footer-previous",
                "Previous",
                "Apply the edits of the photo you viewed before this one",
                self.previous_photo_edits().is_some(),
                &theme,
                cx.listener(|this, _: &ClickEvent, _w, cx| this.apply_previous_edits(cx)),
            ),
            rustyedit_ui::footer_button(
                "footer-reset",
                "Reset",
                "Take this photo back to unedited",
                self.selection_has_edits(),
                &theme,
                cx.listener(|this, _: &ClickEvent, _w, cx| this.reset_edits(cx)),
            ),
        ]
    }
}
