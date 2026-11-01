//! Help with starting a zine: the library's best photos offered up, and the
//! story that is written once and split across the pages as captions.

use gpui::{div, img, prelude::*, AnyElement, ClickEvent, Context, ObjectFit, SharedString};
use rustyedit_core::{zine_photo_score, Flag, ZinePhoto};
use rustyedit_ui::sp;

use crate::{
    workspace::{TextTarget, Workspace},
    zine_bar::pill,
    zines::plural,
};

/// How many photos are suggested at a time.
const SUGGESTED: usize = 12;

impl Workspace {
    /// The photos of the library that look best and are not in the zine,
    /// best first: by rating, pick flag and sharpness. Rejected photos and
    /// missing files are never suggested.
    pub(crate) fn zine_suggestions(&self, limit: usize) -> Vec<usize> {
        let mut scored: Vec<(usize, f32)> = self
            .catalog
            .assets
            .iter()
            .enumerate()
            .filter(|(_, a)| !a.missing)
            .filter(|(_, a)| {
                !self
                    .zine
                    .photos
                    .iter()
                    .any(|p| p.path == a.path && p.copy_name == a.copy_name)
            })
            .filter_map(|(ix, a)| {
                zine_photo_score(
                    a.rating.value(),
                    a.flag == Flag::Picked,
                    a.flag == Flag::Rejected,
                    a.sharpness,
                )
                .map(|score| (ix, score))
            })
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.into_iter().take(limit).map(|(ix, _)| ix).collect()
    }

    /// Puts library photos into the zine.
    pub(crate) fn add_suggested(&mut self, indices: &[usize], cx: &mut Context<Self>) {
        let photos: Vec<ZinePhoto> = indices
            .iter()
            .filter_map(|&ix| self.catalog.assets.get(ix))
            .map(|a| ZinePhoto {
                path: a.path.clone(),
                copy_name: a.copy_name.clone(),
            })
            .collect();
        let mut added = 0;
        self.update_zine(cx, |z| added = z.add(photos));
        self.status_message = Some(
            format!("Added {added} photo{} to the zine ({} in all)", plural(added), self.zine.photos.len()).into(),
        );
    }

    /// Photos worth putting in, as rows with an Add button each.
    pub(crate) fn suggestions_card(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let ideas = self.zine_suggestions(SUGGESTED);
        if ideas.is_empty() {
            return vec![div()
                .text_size(sp(10.5))
                .text_color(theme.text_faint)
                .child("Rate or flag photos in the Library and the best of them show up here.")
                .into_any_element()];
        }
        let mut out: Vec<AnyElement> = vec![div()
            .text_size(sp(10.5))
            .text_color(theme.text_faint)
            .child("The highest rated, picked and sharpest photos not in this zine.")
            .into_any_element()];
        for &ix in &ideas {
            let asset = &self.catalog.assets[ix];
            let stars = "\u{2605}".repeat(asset.rating.value() as usize);
            let name = asset.display_name();
            let thumb = self.display_thumbnail(asset);
            out.push(
                div()
                    .id(SharedString::from(format!("zine-idea-{ix}")))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .p_1()
                    .rounded_lg()
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(sp(52.0))
                            .h(sp(40.0))
                            .rounded_md()
                            .overflow_hidden()
                            .bg(gpui::black())
                            .child(match thumb {
                                Some(path) => img(path).object_fit(ObjectFit::Contain).size_full().into_any_element(),
                                None => div().size_full().into_any_element(),
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(sp(0.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(sp(11.0))
                                    .text_color(theme.text)
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_size(sp(10.0))
                                    .text_color(theme.star)
                                    .child(if asset.flag == Flag::Picked {
                                        format!("{stars} \u{2691} picked")
                                    } else if stars.is_empty() {
                                        "unrated".to_string()
                                    } else {
                                        stars
                                    }),
                            ),
                    )
                    .child(
                        pill(&theme, SharedString::from(format!("zine-idea-add-{ix}")), "Add", "Put this photo in the zine", false, true)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| this.add_suggested(&[ix], cx))),
                    )
                    .into_any_element(),
            );
        }
        let all = ideas.clone();
        out.push(
            pill(
                &theme,
                "zine-idea-add-all".into(),
                &format!("Add all {}", ideas.len()),
                "Put every suggested photo in the zine",
                false,
                true,
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| this.add_suggested(&all, cx)))
            .into_any_element(),
        );
        out
    }

    /// The button on an empty zine's page that fills it with the best
    /// photos.
    pub(crate) fn start_with_best(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = self.theme;
        let ideas = self.zine_suggestions(SUGGESTED);
        if ideas.is_empty() {
            return None;
        }
        let n = ideas.len();
        Some(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_size(sp(12.0))
                        .text_color(theme.text_muted)
                        .child("Or begin with the best of your library:"),
                )
                .child(
                    div()
                        .id("zine-start-best")
                        .flex()
                        .items_center()
                        .h(sp(32.0))
                        .px_4()
                        .rounded_md()
                        .cursor_pointer()
                        .bg(theme.accent)
                        .text_color(gpui::white())
                        .text_size(sp(12.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .shadow_md()
                        .hover(|el| el.opacity(0.92))
                        .child(format!("Start with my {n} best photo{}", plural(n)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            let ideas = this.zine_suggestions(SUGGESTED);
                            this.add_suggested(&ideas, cx);
                        })),
                )
                .into_any_element(),
        )
    }

    // --- The story ---------------------------------------------------------------------

    /// Writing the story, and spreading it over the pages.
    pub(crate) fn story_card(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let typing = self.draft_for(TextTarget::ZineStory);
        let story = self.zine.story.clone();
        let words = story.split_whitespace().count();
        let field = div()
            .id("zine-story-field")
            .flex()
            .flex_col()
            .min_h(sp(72.0))
            .max_h(sp(160.0))
            .overflow_hidden()
            .px_2()
            .py_1p5()
            .rounded_md()
            .bg(theme.background)
            .border_1()
            .text_size(sp(11.5))
            .map(|el| match typing {
                Some(draft) => el
                    .border_color(theme.accent)
                    .text_color(theme.text)
                    .child(format!("{draft}\u{258F}")),
                None => {
                    let current = story.clone();
                    el.border_color(theme.border)
                        .text_color(if story.is_empty() { theme.text_faint } else { theme.text })
                        .cursor_text()
                        .hover(|el| el.border_color(theme.accent))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            this.text_draft = Some((TextTarget::ZineStory, current.clone()));
                            cx.notify();
                        }))
                        .child(if story.is_empty() {
                            "Write the story of the zine here. Click to type; Shift+Enter starts a new line, Enter keeps it."
                                .to_string()
                        } else {
                            story.clone()
                        })
                }
            });
        let pages_with_photos = self
            .zine
            .manual
            .pages
            .iter()
            .filter(|p| !p.is_sheet() && (p.slots.iter().any(Option::is_some) || p.items.iter().any(|i| i.photo.is_some())))
            .count();
        vec![
            div()
                .text_size(sp(10.5))
                .text_color(theme.text_faint)
                .child("Write it once. It is split by paragraph or sentence and set as a caption under the photos of each page.")
                .into_any_element(),
            field.into_any_element(),
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_1()
                .child(
                    pill(
                        &theme,
                        "zine-story-split".into(),
                        "Split across the pages",
                        "Caption each page that has photos with its share of the story",
                        false,
                        words > 0 && !self.zine.photos.is_empty(),
                    )
                    .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.split_story_now(cx))),
                )
                .child(
                    pill(
                        &theme,
                        "zine-story-clear".into(),
                        "Remove the captions",
                        "Take the story's captions off the pages",
                        false,
                        pages_with_photos > 0,
                    )
                    .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                        this.update_zine(cx, |z| z.clear_story_captions());
                    })),
                )
                .into_any_element(),
            div()
                .text_size(sp(10.0))
                .text_color(theme.text_faint)
                .child(format!("{words} word{}", plural(words)))
                .into_any_element(),
        ]
    }

    /// Enter in the story field.
    pub(crate) fn commit_zine_story(&mut self, text: String, cx: &mut Context<Self>) {
        self.update_zine(cx, |z| z.story = text);
    }

    /// Splits the story across the pages, as one undo step.
    pub(crate) fn split_story_now(&mut self, cx: &mut Context<Self>) {
        let mut done = 0;
        self.zine_ui.clear_selection();
        self.update_zine(cx, |z| done = z.apply_story());
        self.status_message = Some(
            if done == 0 {
                "No pages with photos to put the story on yet".to_string()
            } else {
                format!("Put the story on {done} page{}", plural(done))
            }
            .into(),
        );
    }
}
