//! Editable descriptive metadata (title, caption, creator, copyright) in the
//! Inspector. Editing always applies to the whole multi-selection, and each
//! edit is a single undo step. Text is entered with the workspace's shared
//! inline draft system (see `albums.rs`).

use rustyedit_ui::sp;
use gpui::{div, prelude::*, AnyElement, ClickEvent, Context, SharedString, Window};
use rustyedit_core::AssetState;

use crate::workspace::{TextTarget, Workspace};

impl Workspace {
    pub(crate) fn metadata_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(asset) = self.catalog.selected_asset() else {
            return Vec::new();
        };
        let count = self.target_indices().len();

        let rows = [
            (TextTarget::Title, "Title", asset.title.clone()),
            (TextTarget::Caption, "Caption", asset.caption.clone()),
            (TextTarget::Creator, "Creator", asset.creator.clone()),
            (TextTarget::Copyright, "Copyright", asset.copyright.clone()),
        ]
        .into_iter()
        .map(|(target, label, value)| self.metadata_row(target, label, value, cx))
        .collect::<Vec<_>>();

        let keywords = if asset.keywords.is_empty() {
            "—".to_string()
        } else {
            asset.keywords.join(", ")
        };

        let mut children = rows;
        children.push(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w(sp(0.0))
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.text_faint)
                                .child("Keywords"),
                        )
                        .child(div().text_sm().text_color(theme.text).child(keywords)),
                )
                .child(rustyedit_ui::glyph_button(
                    "metadata-edit-keywords",
                    "#",
                    "Edit keywords",
                    theme.text_muted,
                    true,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| {
                        this.open_right_tool(rustyedit_core::RailTool::Keywords, cx);
                    }),
                ))
                .into_any_element(),
        );

        if count > 1 {
            children.push(
                div()
                    .pt_1()
                    .text_size(sp(10.0))
                    .text_color(theme.accent)
                    .child(format!("Changes apply to all {count} selected photos."))
                    .into_any_element(),
            );
        }

        vec![rustyedit_ui::section(&theme, "Metadata", children).into_any_element()]
    }

    fn metadata_row(
        &self,
        target: TextTarget,
        label: &'static str,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let draft = self.draft_for(target);
        let editing = draft.is_some();
        let shown = draft.map(str::to_string).unwrap_or_else(|| value.clone());

        let field = div()
            .id(SharedString::from(format!("metadata-{label}")))
            .flex()
            .flex_row()
            .items_center()
            .min_h(sp(26.0))
            .px_2()
            .when(editing, |el| {
                el.bg(theme.background)
                    .border_1()
                    .border_color(theme.accent)
            })
            .when(!editing, |el| {
                el.cursor_text().hover(|el| el.bg(theme.surface_elevated))
            })
            .child(
                div()
                    .min_w(sp(0.0))
                    .flex_1()
                    .overflow_hidden()
                    .text_sm()
                    .text_color(if shown.is_empty() {
                        theme.text_faint
                    } else {
                        theme.text
                    })
                    .child(if shown.is_empty() {
                        "Add…".to_string()
                    } else {
                        shown
                    }),
            )
            .when(editing, |el| {
                el.child(div().w(sp(1.0)).h(sp(14.0)).ml(sp(1.0)).bg(theme.accent))
            });

        let field = if editing {
            field.into_any_element()
        } else {
            let initial = value.clone();
            field
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.begin_metadata_draft(target, initial.clone(), window, cx)
                }))
                .into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .gap_0p5()
            .child(div().text_xs().text_color(theme.text_faint).child(label))
            .child(field)
            .when(editing, |el| {
                el.child(
                    div()
                        .text_size(sp(9.5))
                        .text_color(theme.text_faint)
                        .child("Enter to apply \u{00B7} Esc to cancel"),
                )
            })
            .into_any_element()
    }

    fn begin_metadata_draft(
        &mut self,
        target: TextTarget,
        initial: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.text_draft = Some((target, initial));
        window.focus(&self.focus_handle);
        cx.notify();
    }

    /// Applies typed text to the metadata field named by `target` on every
    /// selected photo.
    pub(crate) fn commit_metadata_text(
        &mut self,
        target: TextTarget,
        text: String,
        cx: &mut Context<Self>,
    ) {
        let (label, apply): (&'static str, fn(&mut rustyedit_core::Asset, String)) = match target {
            TextTarget::Title => ("Title", |asset, text| asset.title = text),
            TextTarget::Caption => ("Caption", |asset, text| asset.caption = text),
            TextTarget::Creator => ("Creator", |asset, text| asset.creator = text),
            TextTarget::Copyright => ("Copyright", |asset, text| asset.copyright = text),
            _ => return,
        };
        let text = text.trim().to_string();
        let targets = self.target_indices();
        let mut changes: Vec<(usize, AssetState, AssetState)> = Vec::new();
        for &ix in &targets {
            let before = self.catalog.assets[ix].state();
            if let Some(asset) = self.catalog.assets.get_mut(ix) {
                apply(asset, text.clone());
            }
            changes.push((ix, before, self.catalog.assets[ix].state()));
        }
        self.push_undo(label, changes, false);
        self.catalog.refresh();
        self.mark_catalog_dirty(cx);
        self.on_selection_changed(cx);
    }
}
