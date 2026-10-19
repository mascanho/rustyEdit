//! The `i` metadata overlay on the loupe: exposure headline up top, then
//! File / Camera / Exposure / Image groups read from the photo's EXIF.

use std::sync::Arc;

use gpui::{div, prelude::*, px, relative, AnyElement, Context};
use rustyedit_io::{read_metadata, Metadata};

use crate::{
    actions::ToggleInfo,
    workspace::{LibraryView, Mode, Workspace},
};

const OVERLAY_WIDTH: f32 = 264.0;

impl Workspace {
    pub fn toggle_info(
        &mut self,
        _: &ToggleInfo,
        _window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        // The overlay belongs to the loupe; ignore the key elsewhere.
        if self.mode != Mode::Library || self.library_view != LibraryView::Loupe {
            return;
        }
        self.info_visible = !self.info_visible;
        if self.info_visible {
            self.request_metadata(cx);
        }
        cx.notify();
    }

    /// Reads the selected photo's metadata on a background thread, once.
    pub(crate) fn request_metadata(&mut self, cx: &mut Context<Self>) {
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        if self.metadata_cache.contains_key(&asset.id) || !self.metadata_loading.insert(asset.id) {
            return;
        }
        let (id, path, dims) = (asset.id, asset.path.clone(), (asset.width, asset.height));
        let job = cx
            .background_executor()
            .spawn(async move { read_metadata(&path, Some(dims)) });
        cx.spawn(async move |this, cx| {
            let metadata = job.await;
            this.update(cx, |this, cx| {
                this.metadata_loading.remove(&id);
                this.metadata_cache.insert(id, Arc::new(metadata));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn toggle_info_group(&mut self, title: &'static str, cx: &mut Context<Self>) {
        if !self.info_collapsed.remove(title) {
            self.info_collapsed.insert(title);
        }
        cx.notify();
    }

    pub(crate) fn render_info_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.info_visible {
            return None;
        }
        let theme = self.theme;
        let asset = self.catalog.selected_asset()?;
        let metadata: Option<&Arc<Metadata>> = self.metadata_cache.get(&asset.id);

        let mut body: Vec<AnyElement> = Vec::new();
        match metadata {
            None => body.push(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child("Reading metadata\u{2026}")
                    .into_any_element(),
            ),
            Some(metadata) => {
                // The exposure facts stay pinned open: bare values with
                // hairline dividers, no boxes.
                if !metadata.headline.is_empty() {
                    body.push(
                        div()
                            .flex()
                            .flex_row()
                            .pb_2()
                            .border_b_1()
                            .border_color(theme.border_subtle)
                            .children(metadata.headline.iter().enumerate().map(
                                |(i, (label, value))| {
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .gap_0p5()
                                        .when(i > 0, |el| {
                                            el.border_l_1().border_color(theme.border_subtle)
                                        })
                                        .child(
                                            div()
                                                .text_size(px(12.5))
                                                .text_color(theme.text)
                                                .child(value.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(8.0))
                                                .text_color(theme.text_faint)
                                                .child(label.to_uppercase()),
                                        )
                                },
                            ))
                            .into_any_element(),
                    );
                }
                // Everything else is collapsible, open by default.
                for group in &metadata.groups {
                    let title = group.title;
                    let collapsed = self.info_collapsed.contains(title);
                    let mut section = div().flex().flex_col().child(
                        div()
                            .id(gpui::SharedString::from(format!("info-group-{title}")))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .py_1()
                            .cursor_pointer()
                            .text_color(theme.text_faint)
                            .hover(|el| el.text_color(theme.text_muted))
                            .child(div().w(px(10.0)).text_size(px(10.0)).child(if collapsed {
                                "\u{25B8}"
                            } else {
                                "\u{25BE}"
                            }))
                            .child(
                                div()
                                    .text_size(px(9.5))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(title.to_uppercase()),
                            )
                            .on_click(cx.listener(move |this, _, _w, cx| {
                                this.toggle_info_group(title, cx)
                            })),
                    );
                    if !collapsed {
                        section = section.child(
                            div().flex().flex_col().gap_0p5().pl_3().pb_1().children(
                                group.rows.iter().map(|(label, value)| {
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_start()
                                        .justify_between()
                                        .gap_3()
                                        .text_size(px(10.5))
                                        .child(
                                            div()
                                                .flex_shrink_0()
                                                .text_color(theme.text_faint)
                                                .child(*label),
                                        )
                                        .child(
                                            div()
                                                .min_w(px(0.0))
                                                .text_right()
                                                .text_color(theme.text_muted)
                                                .child(value.clone()),
                                        )
                                }),
                            ),
                        );
                    }
                    body.push(section.into_any_element());
                }
            }
        }

        Some(
            div()
                .id("info-overlay")
                .absolute()
                .top(px(12.0))
                .left(px(12.0))
                .max_h(relative(1.0))
                .w(px(OVERLAY_WIDTH))
                .occlude()
                .overflow_y_scroll()
                .p_2p5()
                .flex()
                .flex_col()
                .gap_2()
                .rounded_xl()
                .bg(theme.background.opacity(0.82))
                .border_1()
                .border_color(theme.border_subtle)
                .shadow_lg()
                .children(body)
                .into_any_element(),
        )
    }
}
