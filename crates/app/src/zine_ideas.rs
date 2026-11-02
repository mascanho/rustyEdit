//! Help with starting a zine: the library's best photos offered up, and the
//! story that is written once and split across the pages as captions.

use gpui::{div, prelude::*, AnyElement, ClickEvent, Context};
use rustyedit_core::{zine_photo_score, Flag, ZinePhoto};
use rustyedit_ui::sp;

use crate::{
    workspace::Workspace,
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
