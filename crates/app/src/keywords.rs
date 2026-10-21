//! The Keywords rail tool: apply keywords to the selection (with
//! autocomplete), remove them, browse every keyword in the catalog as a
//! filter facet, and apply whole keyword sets in one click.

use std::collections::HashMap;

use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context, SharedString};
use rustyedit_core::AssetState;

use crate::workspace::{TextTarget, Workspace};

impl Workspace {
    pub(crate) fn keywords_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let editing = self.draft_for(TextTarget::Keyword);
        let draft = editing.unwrap_or("").to_string();
        let Some(asset) = self.catalog.selected_asset() else {
            return vec![div()
                .pt_3()
                .text_xs()
                .text_color(theme.text_faint)
                .child("Select a photo to add keywords.")
                .into_any_element()];
        };
        let current = asset.keywords.clone();
        let target_count = self.target_indices().len();

        // Distinct keywords and their counts, most used first.
        let mut counts: HashMap<String, usize> = HashMap::new();
        let mut display: HashMap<String, String> = HashMap::new();
        for asset in &self.catalog.assets {
            for keyword in &asset.keywords {
                let key = keyword.to_lowercase();
                *counts.entry(key.clone()).or_default() += 1;
                display.entry(key).or_insert_with(|| keyword.clone());
            }
        }
        let mut all: Vec<(String, usize)> = counts
            .into_iter()
            .map(|(key, count)| (display.remove(&key).unwrap_or(key), count))
            .collect();
        all.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then(a.0.to_lowercase().cmp(&b.0.to_lowercase()))
        });

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
                        .child("KEYWORDS"),
                )
                .when(target_count > 1, |el| {
                    el.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.accent)
                            .child(format!("{target_count} selected")),
                    )
                })
                .into_any_element(),
        );

        // Entry field.
        let field = div()
            .id("keyword-field")
            .flex()
            .flex_row()
            .items_center()
            .h(px(28.0))
            .px_2()
            .bg(theme.background)
            .border_1()
            .border_color(if editing.is_some() {
                theme.accent
            } else {
                theme.border_subtle
            })
            .text_sm()
            .cursor_text()
            .child(
                div()
                    .text_color(if draft.is_empty() {
                        theme.text_faint
                    } else {
                        theme.text
                    })
                    .child(if draft.is_empty() && editing.is_none() {
                        "Add a keyword…".to_string()
                    } else {
                        draft.clone()
                    }),
            )
            .when(editing.is_some(), |el| {
                el.child(div().w(px(1.0)).h(px(14.0)).ml(px(1.0)).bg(theme.accent))
            })
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.begin_text_draft(TextTarget::Keyword, window, cx)
            }))
            .into_any_element();
        out.push(field);

        // Autocomplete: existing keywords containing the typed text.
        if !draft.trim().is_empty() {
            let needle = draft.trim().to_lowercase();
            let mut suggestions: Vec<&str> = all
                .iter()
                .map(|(keyword, _)| keyword.as_str())
                .filter(|keyword| keyword.to_lowercase().contains(&needle))
                .take(8)
                .collect();
            if suggestions.is_empty() {
                suggestions.push(draft.trim());
            }
            let rows: Vec<AnyElement> = suggestions
                .into_iter()
                .map(|keyword| {
                    let keyword = keyword.to_string();
                    div()
                        .id(SharedString::from(format!("keyword-suggest-{keyword}")))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .h(px(22.0))
                        .px_2()
                        .cursor_pointer()
                        .text_size(px(11.5))
                        .text_color(theme.text_muted)
                        .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                        .child(div().text_color(theme.accent).child("#"))
                        .child(keyword.clone())
                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            this.apply_keywords(vec![keyword.clone()], cx)
                        }))
                        .into_any_element()
                })
                .collect();
            out.push(
                div()
                    .flex()
                    .flex_col()
                    .pt_1()
                    .children(rows)
                    .into_any_element(),
            );
        }

        out.push(
            div()
                .pt_2()
                .text_size(px(10.0))
                .text_color(theme.text_faint)
                .child(if target_count > 1 {
                    "Keywords apply to every selected photo."
                } else {
                    "Enter to add \u{00B7} separate several with commas."
                })
                .into_any_element(),
        );

        // Keywords on the selected photo (with removal).
        out.push(
            div()
                .pt_3()
                .pb_1()
                .text_size(px(10.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(theme.text_faint)
                .child("ON THIS PHOTO")
                .into_any_element(),
        );
        if current.is_empty() {
            out.push(
                div()
                    .pb_1()
                    .text_xs()
                    .text_color(theme.text_faint)
                    .child("No keywords yet.")
                    .into_any_element(),
            );
        } else {
            let chips = current
                .iter()
                .map(|keyword| {
                    let keyword = keyword.clone();
                    let remove = keyword.clone();
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .h(px(22.0))
                        .pl_2()
                        .pr_1()
                        .rounded_full()
                        .bg(theme.accent_muted)
                        .text_size(px(10.5))
                        .text_color(theme.text)
                        .child(keyword)
                        .child(
                            div()
                                .id(SharedString::from(format!("keyword-remove-{remove}")))
                                .px(px(2.0))
                                .rounded_full()
                                .cursor_pointer()
                                .text_color(theme.text_faint)
                                .hover(|el| el.text_color(theme.danger))
                                .child("\u{2715}")
                                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                    this.remove_keywords(vec![remove.clone()], cx)
                                })),
                        )
                        .into_any_element()
                })
                .collect::<Vec<_>>();
            out.push(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_1()
                    .pb_1()
                    .children(chips)
                    .into_any_element(),
            );
        }

        // Keyword sets.
        if !self.catalog_def.keyword_sets.is_empty() {
            out.push(
                div()
                    .pt_3()
                    .pb_1()
                    .text_size(px(10.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.text_faint)
                    .child("KEYWORD SETS")
                    .into_any_element(),
            );
            for (set_ix, set) in self.catalog_def.keyword_sets.iter().enumerate() {
                let keywords = set.keywords.clone();
                let label = if keywords.is_empty() {
                    "—".to_string()
                } else {
                    keywords.join(" \u{00B7} ")
                };
                out.push(
                    div()
                        .id(SharedString::from(format!("keyword-set-{set_ix}")))
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .px_2()
                        .py_1()
                        .cursor_pointer()
                        .hover(|el| el.bg(theme.surface_elevated))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_between()
                                .text_size(px(11.5))
                                .text_color(theme.text)
                                .child(set.name.clone())
                                .child(
                                    div()
                                        .text_size(px(9.5))
                                        .text_color(theme.accent)
                                        .child("APPLY"),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(theme.text_faint)
                                .child(label),
                        )
                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            this.apply_keywords(keywords.clone(), cx)
                        }))
                        .into_any_element(),
                );
            }
        }

        // Browse every keyword; clicking one filters the library by it.
        if !all.is_empty() {
            let active = self.catalog.filters().keyword.clone();
            out.push(
                div()
                    .pt_3()
                    .pb_1()
                    .text_size(px(10.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.text_faint)
                    .child("ALL KEYWORDS")
                    .into_any_element(),
            );
            for (keyword, count) in all {
                let value = keyword.clone();
                let is_active = active.as_deref() == Some(keyword.as_str());
                let filter_value = keyword.clone();
                let apply_value = keyword.clone();
                out.push(
                    div()
                        .id(SharedString::from(format!("keyword-all-{keyword}")))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .h(px(24.0))
                        .px_2()
                        .cursor_pointer()
                        .text_size(px(11.5))
                        .when(is_active, |el| {
                            el.bg(theme.accent_muted).text_color(theme.text)
                        })
                        .when(!is_active, |el| {
                            el.text_color(theme.text_muted)
                                .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                        })
                        .child(div().text_color(theme.accent).child("#"))
                        .child(div().flex_1().min_w(px(0.0)).overflow_hidden().child(value))
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(theme.text_faint)
                                .child(count.to_string()),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("keyword-add-{keyword}")))
                                .px(px(3.0))
                                .text_color(theme.text_faint)
                                .hover(|el| el.text_color(theme.accent))
                                .child("+")
                                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                    cx.stop_propagation();
                                    this.apply_keywords(vec![apply_value.clone()], cx)
                                })),
                        )
                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            let value = filter_value.clone();
                            this.update_filters(
                                move |filters| {
                                    if filters.keyword.as_deref() == Some(value.as_str()) {
                                        filters.keyword = None;
                                    } else {
                                        filters.keyword = Some(value);
                                    }
                                },
                                cx,
                            );
                        }))
                        .into_any_element(),
                );
            }
        }

        out
    }

    /// Adds `keywords` to every selected photo as one undo step.
    pub(crate) fn apply_keywords(&mut self, keywords: Vec<String>, cx: &mut Context<Self>) {
        let keywords: Vec<String> = keywords
            .into_iter()
            .flat_map(|word| {
                word.split(',')
                    .map(|part| part.trim().to_string())
                    .collect::<Vec<_>>()
            })
            .filter(|word| !word.is_empty())
            .collect();
        if keywords.is_empty() || self.catalog.selected_asset().is_none() {
            return;
        }
        if matches!(self.text_draft, Some((TextTarget::Keyword, _))) {
            self.text_draft = None;
        }
        let targets = self.target_indices();
        let mut changes: Vec<(usize, AssetState, AssetState)> = Vec::new();
        for &ix in &targets {
            let before = self.catalog.assets[ix].state();
            if let Some(asset) = self.catalog.assets.get_mut(ix) {
                for keyword in &keywords {
                    asset.add_keyword(keyword);
                }
            }
            changes.push((ix, before, self.catalog.assets[ix].state()));
        }
        self.push_undo("Keywords", changes, false);
        self.catalog.refresh();
        self.mark_catalog_dirty(cx);
        self.on_selection_changed(cx);
    }

    /// Removes `keywords` from every selected photo as one undo step.
    pub(crate) fn remove_keywords(&mut self, keywords: Vec<String>, cx: &mut Context<Self>) {
        let targets = self.target_indices();
        let mut changes: Vec<(usize, AssetState, AssetState)> = Vec::new();
        for &ix in &targets {
            let before = self.catalog.assets[ix].state();
            if let Some(asset) = self.catalog.assets.get_mut(ix) {
                for keyword in &keywords {
                    asset.remove_keyword(keyword);
                }
            }
            changes.push((ix, before, self.catalog.assets[ix].state()));
        }
        self.push_undo("Keywords", changes, false);
        self.catalog.refresh();
        self.mark_catalog_dirty(cx);
        self.on_selection_changed(cx);
    }
}
