//! The Duplicates rail tool: exact duplicates (sampled file content hash) and
//! visually similar photos (dHash distance). Hashing happens during import;
//! pairing runs in the background the first time the panel is opened.

use gpui::{div, img, prelude::*, px, AnyElement, ClickEvent, Context, ObjectFit, SharedString};
use rustyedit_core::{find_duplicates_in, DuplicateGroup, DuplicateKind, PhotoHash};

use crate::{
    actions::FindDuplicates,
    workspace::{Mode, Workspace},
};

impl Workspace {
    pub fn find_duplicates(
        &mut self,
        _: &FindDuplicates,
        _w: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode != Mode::Library {
            return;
        }
        self.open_left_tool(rustyedit_core::RailTool::Duplicates, cx);
        self.ensure_duplicates(cx);
    }

    /// Starts the (background) duplicate scan if it hasn't run for this
    /// catalog yet.
    pub(crate) fn ensure_duplicates(&mut self, cx: &mut Context<Self>) {
        if self.duplicate_groups.is_some() || self.duplicates_loading {
            return;
        }
        if self.catalog.assets.is_empty() {
            self.duplicate_groups = Some(Vec::new());
            return;
        }
        self.duplicates_loading = true;
        let hashes: Vec<PhotoHash> = self
            .catalog
            .assets
            .iter()
            .enumerate()
            // A virtual copy is the same file on purpose, not a duplicate.
            .filter(|(_, asset)| !asset.is_virtual_copy())
            .map(|(index, asset)| PhotoHash {
                index,
                width: asset.width,
                height: asset.height,
                content_hash: asset.content_hash,
                phash: asset.phash,
            })
            .collect();
        let job = cx
            .background_executor()
            .spawn(async move { find_duplicates_in(&hashes) });
        cx.spawn(async move |this, cx| {
            let groups = job.await;
            this.update(cx, |this, cx| {
                this.duplicates_loading = false;
                this.duplicate_groups = Some(groups);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn duplicates_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let mut out: Vec<AnyElement> = Vec::new();

        out.push(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .pt_3()
                .pb_1()
                .child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.accent)
                        .child("DUPLICATES"),
                )
                .child(rustyedit_ui::chrome_button(
                    "duplicates-rescan",
                    "Rescan",
                    false,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| {
                        this.duplicate_groups = None;
                        this.ensure_duplicates(cx);
                    }),
                ))
                .into_any_element(),
        );

        if self.duplicates_loading {
            return vec![
                out.remove(0),
                div()
                    .py_2()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child("Comparing photos…")
                    .into_any_element(),
            ];
        }

        let Some(groups) = &self.duplicate_groups else {
            out.push(
                div()
                    .py_2()
                    .text_xs()
                    .text_color(theme.text_faint)
                    .child("Open this panel to scan the catalog.")
                    .into_any_element(),
            );
            return out;
        };

        if groups.is_empty() {
            out.push(
                div()
                    .py_2()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child("No duplicates or similar photos found.")
                    .into_any_element(),
            );
            return out;
        }

        let exact = groups
            .iter()
            .filter(|group| group.kind == DuplicateKind::Exact)
            .count();
        let similar = groups.len() - exact;
        out.push(
            div()
                .pb_2()
                .text_size(px(10.5))
                .text_color(theme.text_faint)
                .child(format!(
                    "{exact} exact group{} \u{00B7} {similar} similar group{}",
                    if exact == 1 { "" } else { "s" },
                    if similar == 1 { "" } else { "s" }
                ))
                .into_any_element(),
        );

        for (group_ix, group) in groups.iter().enumerate() {
            out.push(self.duplicate_group_row(group_ix, group, cx));
        }
        out
    }

    fn duplicate_group_row(
        &self,
        group_ix: usize,
        group: &DuplicateGroup,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let (title, color) = match group.kind {
            DuplicateKind::Exact => ("EXACT DUPLICATES", theme.reject),
            DuplicateKind::Similar => ("SIMILAR PHOTOS", theme.accent),
        };

        let previews = group
            .indices
            .iter()
            .take(4)
            .filter_map(|&ix| {
                let asset = self.catalog.assets.get(ix)?;
                let source = asset
                    .thumbnail_path
                    .clone()
                    .unwrap_or_else(|| asset.path.clone());
                Some(
                    div()
                        .w(px(52.0))
                        .h(px(40.0))
                        .overflow_hidden()
                        .bg(theme.background)
                        .child(img(source).object_fit(ObjectFit::Contain).size_full())
                        .into_any_element(),
                )
            })
            .collect::<Vec<_>>();

        let select_indices = group.indices.clone();
        let count = group.indices.len();

        div()
            .id(SharedString::from(format!("duplicate-group-{group_ix}")))
            .flex()
            .flex_col()
            .gap_1()
            .pb_2()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(9.5))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(color)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_faint)
                            .child(format!("{count} photos")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .children(previews)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_row()
                            .justify_end()
                            .child(rustyedit_ui::chrome_button(
                                SharedString::from(format!("duplicate-select-{group_ix}")),
                                "Select",
                                false,
                                &theme,
                                cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                    this.select_indices(&select_indices, cx)
                                }),
                            )),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn select_indices(&mut self, indices: &[usize], cx: &mut Context<Self>) {
        let visible: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&ix| self.catalog.visible_position(ix).is_some())
            .collect();
        if visible.is_empty() {
            return;
        }
        self.selection = visible.iter().copied().collect();
        self.selection_anchor = visible.first().copied();
        self.catalog.select(visible[0]);
        self.on_selection_changed(cx);
    }
}
