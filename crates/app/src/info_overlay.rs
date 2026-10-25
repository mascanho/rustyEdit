//! The `i` metadata overlay on the loupe: exposure headline up top, then
//! File / Camera / Exposure / Image groups read from the photo's EXIF.

use rustyedit_ui::sp;
use std::sync::Arc;

use gpui::{div, prelude::*, px, relative, AnyElement, Context};
use rustyedit_io::{read_metadata, Metadata};

use crate::{
    actions::ToggleInfo,
    workspace::{LibraryView, Mode, Workspace},
};

const OVERLAY_WIDTH: f32 = 300.0;

impl Workspace {
    pub fn toggle_info(
        &mut self,
        _: &ToggleInfo,
        _window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        // The overlay sits over a single photo: the loupe or Develop.
        let on_photo = match self.mode {
            Mode::Develop => true,
            Mode::Library => self.library_view == LibraryView::Loupe,
            _ => false,
        };
        if !on_photo {
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
        self.request_metadata_for(self.catalog.selected, cx);
    }

    /// Reads any photo's metadata on a background thread, once.
    pub(crate) fn request_metadata_for(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(asset) = self.catalog.assets.get(index) else {
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

    /// Opens the metadata popup for the card at `index`.
    pub(crate) fn open_card_info(
        &mut self,
        index: usize,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.card_info = Some((index, position));
        self.context_menu = None;
        self.request_metadata_for(index, cx);
        cx.notify();
    }

    /// The popup shown by a card's ⓘ button: file name, any title and
    /// keywords, then the full metadata. Click anywhere else to dismiss.
    pub(crate) fn render_card_info(
        &self,
        window: &gpui::Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let Some((index, position)) = self.card_info else {
            return div().into_any_element();
        };
        let Some(asset) = self.catalog.assets.get(index) else {
            return div().into_any_element();
        };

        let viewport = window.viewport_size();
        let max_height = (f32::from(viewport.height) - 16.0).min(520.0);
        let x = (f32::from(position.x) + 10.0)
            .min(f32::from(viewport.width) - OVERLAY_WIDTH - 8.0)
            .max(8.0);
        let y = f32::from(position.y)
            .min(f32::from(viewport.height) - max_height - 8.0)
            .max(8.0);

        let mut header = div().flex().flex_col().gap_0p5().child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text)
                .child(asset.display_name()),
        );
        if !asset.title.trim().is_empty() {
            header = header.child(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(asset.title.clone()),
            );
        }
        if !asset.keywords.is_empty() {
            header = header.child(
                div()
                    .text_size(sp(10.5))
                    .text_color(theme.accent)
                    .child(asset.keywords.join(" \u{00B7} ")),
            );
        }

        let body = self.info_body(index, cx);
        div()
            .id("card-info-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _: &gpui::MouseDownEvent, _w, cx| {
                    this.card_info = None;
                    cx.notify();
                }),
            )
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(|this, _: &gpui::MouseDownEvent, _w, cx| {
                    this.card_info = None;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .id("card-info")
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(OVERLAY_WIDTH))
                    .max_h(px(max_height))
                    .occlude()
                    .overflow_y_scroll()
                    .p_2p5()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .rounded_xl()
                    .bg(theme.surface_elevated)
                    .border_1()
                    .border_color(theme.border)
                    .shadow_lg()
                    .child(header)
                    .children(body),
            )
            .into_any_element()
    }

    fn toggle_info_group(&mut self, title: &'static str, cx: &mut Context<Self>) {
        if !self.info_collapsed.remove(title) {
            self.info_collapsed.insert(title);
        }
        cx.notify();
    }

    /// The metadata sections for the photo at `index`: pinned exposure facts
    /// and collapsible groups. Shared by the loupe overlay and the card popup.
    pub(crate) fn info_body(&self, index: usize, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let metadata: Option<&Arc<Metadata>> = self
            .catalog
            .assets
            .get(index)
            .and_then(|asset| self.metadata_cache.get(&asset.id));

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
                // Who took it, with what, and when.
                let dimensions = asset_dimensions(self, index);
                if !metadata.camera.is_empty() || !metadata.lens.is_empty() || !metadata.taken.is_empty() {
                    body.push(
                        div()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_baseline()
                                    .justify_between()
                                    .gap_2()
                                    .child(
                                        div()
                                            .min_w(sp(0.0))
                                            .truncate()
                                            .text_size(sp(13.5))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(theme.text)
                                            .child(if metadata.camera.is_empty() {
                                                "Unknown camera".to_string()
                                            } else {
                                                metadata.camera.clone()
                                            }),
                                    )
                                    .children(dimensions.map(|text| {
                                        div()
                                            .flex_shrink_0()
                                            .text_size(sp(10.0))
                                            .text_color(theme.accent)
                                            .child(text)
                                    })),
                            )
                            .when(!metadata.lens.is_empty(), |el| {
                                el.child(
                                    div()
                                        .truncate()
                                        .text_size(sp(11.0))
                                        .text_color(theme.text_muted)
                                        .child(metadata.lens.clone()),
                                )
                            })
                            .when(!metadata.taken.is_empty(), |el| {
                                el.child(
                                    div()
                                        .text_size(sp(10.0))
                                        .text_color(theme.text_faint)
                                        .child(metadata.taken.clone()),
                                )
                            })
                            .into_any_element(),
                    );
                }
                // The exposure triangle and focal length, as four tiles
                // that are always there (a dash for what the camera didn't
                // record, as with a manual lens).
                body.push(
                    div()
                        .flex()
                        .flex_row()
                        .gap_1p5()
                        .children(["Aperture", "Shutter", "ISO", "Focal"].map(|label| {
                            let value = metadata
                                .headline
                                .iter()
                                .find(|(l, _)| *l == label)
                                .map(|(_, v)| v.clone());
                            let known = value.is_some();
                            div()
                                .flex_1()
                                .min_w(sp(0.0))
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap_0p5()
                                .py_1p5()
                                .rounded_lg()
                                .bg(theme.surface.opacity(0.65))
                                .border_1()
                                .border_color(theme.border_subtle)
                                .child(
                                    div()
                                        .text_size(sp(14.0))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(if known { theme.text } else { theme.text_faint })
                                        .child(value.unwrap_or_else(|| "\u{2014}".to_string())),
                                )
                                .child(
                                    div()
                                        .text_size(sp(8.0))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(theme.accent.opacity(0.9))
                                        .child(label.to_uppercase()),
                                )
                        }))
                        .into_any_element(),
                );
                // The smaller facts, as tags.
                if !metadata.chips.is_empty() {
                    body.push(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap_1()
                            .children(metadata.chips.iter().map(|(label, value)| {
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .h(sp(18.0))
                                    .px_1p5()
                                    .rounded_full()
                                    .bg(theme.surface.opacity(0.65))
                                    .border_1()
                                    .border_color(theme.border_subtle)
                                    .text_size(sp(9.5))
                                    .child(
                                        div()
                                            .text_color(theme.text_faint)
                                            .child(label.to_string()),
                                    )
                                    .child(div().text_color(theme.text).child(value.clone()))
                            }))
                            .into_any_element(),
                    );
                }
                // Everything else is collapsible, open by default.
                for group in &metadata.groups {
                    let title = group.title;
                    let collapsed = self.info_collapsed.contains(title);
                    let mut section = div()
                        .flex()
                        .flex_col()
                        .px_1p5()
                        .rounded_lg()
                        .bg(theme.surface.opacity(0.4))
                        .child(
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
                            .child(rustyedit_ui::icon(
                                if collapsed {
                                    rustyedit_ui::Icon::ChevronRight
                                } else {
                                    rustyedit_ui::Icon::ChevronDown
                                },
                                12.0,
                                theme.text_muted,
                            ))
                            .child(
                                div()
                                    .text_size(sp(9.5))
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
                                    // One line per fact, cut with an ellipsis
                                    // when too long; the tooltip has it all.
                                    div()
                                        .id(gpui::SharedString::from(format!(
                                            "info-row-{title}-{label}"
                                        )))
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .justify_between()
                                        .gap_3()
                                        .w_full()
                                        .min_w(sp(0.0))
                                        .overflow_hidden()
                                        .text_size(sp(10.5))
                                        .tooltip(rustyedit_ui::tip(
                                            format!("{label}: {value}"),
                                            &theme,
                                        ))
                                        .child(
                                            div()
                                                .flex_shrink_0()
                                                .text_color(theme.text_faint)
                                                .child(*label),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w(sp(0.0))
                                                .truncate()
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

        body
    }

    pub(crate) fn render_info_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.info_visible {
            return None;
        }
        let theme = self.theme;
        self.catalog.selected_asset()?;

        let mut body = self.info_body(self.catalog.selected, cx);
        // The photo's histogram, as the camera-and-lens block ends.
        let (histogram, _) = self.current_histogram();
        if histogram.is_some() {
            let strip = rustyedit_ui::histogram_view_with_height(
                &theme,
                rustyedit_ui::HistogramProps {
                    histogram,
                    channel: rustyedit_core::HistogramChannel::Rgb,
                    log_scale: false,
                    hover: None,
                    zone: None,
                    bounds: None,
                },
                46.0,
            )
            .into_any_element();
            body.insert(body.len().min(1), strip);
        }

        Some(
            div()
                .id("info-overlay")
                .absolute()
                .top(sp(12.0))
                .left(sp(12.0))
                .max_h(relative(1.0))
                .w(px(OVERLAY_WIDTH))
                .occlude()
                .overflow_y_scroll()
                .p_2p5()
                .flex()
                .flex_col()
                .gap_2()
                .rounded_xl()
                .bg(theme.background.opacity(0.9))
                .border_1()
                .border_color(theme.border)
                .shadow_lg()
                .children(body)
                .into_any_element(),
        )
    }
}

/// The photo's size as "24 MP · 6000 × 4000", once it's known.
fn asset_dimensions(workspace: &Workspace, index: usize) -> Option<String> {
    let asset = workspace.catalog.assets.get(index)?;
    (asset.width > 0 && asset.height > 0).then(|| {
        format!(
            "{:.0} MP \u{00B7} {} \u{00D7} {}",
            asset.width as f64 * asset.height as f64 / 1e6,
            asset.width,
            asset.height
        )
    })
}
