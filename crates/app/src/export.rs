//! The Export module: choose which photos, format, size and destination,
//! then render each photo through its saved develop settings at full
//! resolution and write it out.

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use gpui::{
    div, prelude::*, px, relative, AnyElement, App, ClickEvent, Context, SharedString, Window,
};
use image::{codecs::jpeg::JpegEncoder, DynamicImage, ExtendedColorType, ImageEncoder};
use rustyedit_core::{Asset, ExportFormat, ExportOptions, ExportScope, ExportSize, Flag};

use crate::workspace::Workspace;

impl Workspace {
    /// The photos the current scope selects, cloned for the worker thread.
    fn export_assets(&self) -> Vec<Asset> {
        let assets = &self.catalog.assets;
        match self.export_options.scope {
            ExportScope::Picked => assets
                .iter()
                .filter(|a| a.flag == Flag::Picked)
                .cloned()
                .collect(),
            ExportScope::All => assets.clone(),
            ExportScope::Selected => self
                .target_indices()
                .into_iter()
                .filter_map(|ix| self.catalog.assets.get(ix).cloned())
                .collect(),
        }
    }

    fn export_count(&self, scope: ExportScope) -> usize {
        match scope {
            ExportScope::Picked => self.catalog.counts().picked,
            ExportScope::All => self.catalog.counts().total,
            ExportScope::Selected if self.catalog.selected_asset().is_some() => {
                self.target_indices().len()
            }
            ExportScope::Selected => 0,
        }
    }

    fn update_export(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut ExportOptions)) {
        change(&mut self.export_options);
        self.export_message = None;
        cx.notify();
    }

    fn choose_export_destination(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let folder = cx
                .background_executor()
                .spawn(async { rfd::FileDialog::new().pick_folder() })
                .await;
            if let Some(folder) = folder {
                this.update(cx, |this, cx| {
                    this.update_export(cx, |o| o.destination = Some(folder));
                })
                .ok();
            }
        })
        .detach();
    }

    fn start_export(&mut self, cx: &mut Context<Self>) {
        if self.export_progress.is_some() {
            return;
        }
        let Some(destination) = self.export_options.destination.clone() else {
            self.export_message = Some("Choose a destination folder first.".into());
            cx.notify();
            return;
        };
        let assets = self.export_assets();
        if assets.is_empty() {
            self.export_message = Some("Nothing to export for this selection.".into());
            cx.notify();
            return;
        }

        let options = self.export_options.clone();
        let done = Arc::new(AtomicUsize::new(0));
        self.export_progress = Some((done.clone(), assets.len()));
        self.export_message = None;
        cx.notify();

        let total = assets.len();
        let job = cx.background_executor().spawn(async move {
            let mut failed = 0usize;
            for asset in &assets {
                if export_one(asset, &options, &destination).is_err() {
                    failed += 1;
                }
                done.fetch_add(1, Ordering::Relaxed);
            }
            failed
        });

        cx.spawn(async move |this, cx| {
            // Repaint a few times a second so the progress bar moves.
            let ticker = cx.spawn({
                let this = this.clone();
                async move |cx| {
                    while this.update(cx, |_, cx| cx.notify()).is_ok() {
                        cx.background_executor()
                            .timer(Duration::from_millis(100))
                            .await;
                    }
                }
            });
            let failed = job.await;
            drop(ticker);
            this.update(cx, |this, cx| {
                this.export_progress = None;
                this.export_message = Some(if failed == 0 {
                    format!(
                        "Exported {total} photo{}.",
                        if total == 1 { "" } else { "s" }
                    )
                    .into()
                } else {
                    format!("Exported {} of {total}; {failed} failed.", total - failed).into()
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // --- UI ------------------------------------------------------------------

    pub(crate) fn render_export(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let options = self.export_options.clone();
        let count = self.export_count(options.scope);

        let choice = |id: String,
                      label: String,
                      selected: bool,
                      on_click: Box<dyn Fn(&mut Self, &mut Context<Self>)>| {
            rustyedit_ui::chrome_button(
                SharedString::from(id),
                label,
                selected,
                &theme,
                cx.listener(move |this, _: &ClickEvent, _w, cx| on_click(this, cx)),
            )
            .into_any_element()
        };
        let row = |children: Vec<AnyElement>| {
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_1()
                .children(children)
                .into_any_element()
        };

        let scope_row = row(ExportScope::ALL
            .iter()
            .map(|&scope| {
                choice(
                    format!("export-scope-{}", scope.label()),
                    format!("{} ({})", scope.label(), self.export_count(scope)),
                    options.scope == scope,
                    Box::new(move |this, cx| this.update_export(cx, |o| o.scope = scope)),
                )
            })
            .collect());

        let format_row = row(ExportFormat::ALL
            .iter()
            .map(|&format| {
                choice(
                    format!("export-format-{}", format.label()),
                    format.label().into(),
                    options.format == format,
                    Box::new(move |this, cx| this.update_export(cx, |o| o.format = format)),
                )
            })
            .collect());

        let size_row = row(ExportSize::PRESETS
            .iter()
            .map(|&size| {
                choice(
                    format!("export-size-{}", size.label()),
                    size.label(),
                    options.size == size,
                    Box::new(move |this, cx| this.update_export(cx, |o| o.size = size)),
                )
            })
            .collect());

        let mut format_children = vec![format_row];
        if options.format == ExportFormat::Jpeg {
            let entity = cx.entity();
            format_children.push(labelled(
                &theme,
                "Quality",
                rustyedit_ui::stepper(
                    "export-quality",
                    options.quality as i64,
                    1,
                    100,
                    5,
                    "",
                    &theme,
                    move |value, _w: &mut Window, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.update_export(cx, |o| o.quality = value.clamp(1, 100) as u8)
                        });
                    },
                )
                .into_any_element(),
            ));
        }

        let destination = options
            .destination
            .as_deref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "No folder chosen".into());
        let destination_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .px_3()
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .rounded_md()
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .text_sm()
                    .text_color(if options.destination.is_some() {
                        theme.text
                    } else {
                        theme.text_faint
                    })
                    .child(destination),
            )
            .child(rustyedit_ui::chrome_button(
                "export-choose",
                "Choose…",
                false,
                &theme,
                cx.listener(|this, _: &ClickEvent, _w, cx| this.choose_export_destination(cx)),
            ))
            .into_any_element();

        // While an export runs, the centre column shows its progress.
        let progress: Option<AnyElement> = match &self.export_progress {
            Some((done, total)) => {
                let done = done.load(Ordering::Relaxed).min(*total);
                let fraction = if *total == 0 {
                    0.0
                } else {
                    done as f32 / *total as f32
                };
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.text)
                                .child(format!("Exporting {done} of {total}…")),
                        )
                        .child(
                            div()
                                .w_full()
                                .h(px(6.0))
                                .rounded_full()
                                .bg(theme.border)
                                .child(
                                    div()
                                        .h_full()
                                        .w(relative(fraction))
                                        .rounded_full()
                                        .bg(theme.accent),
                                ),
                        )
                        .into_any_element(),
                )
            }
            None => None,
        };

        // The settings live in the left panel, in the same titled sections
        // the other modules use.
        let sections: Vec<AnyElement> = vec![
            rustyedit_ui::section(&theme, "Photos", vec![scope_row]).into_any_element(),
            rustyedit_ui::section(&theme, "Format", format_children).into_any_element(),
            rustyedit_ui::section(&theme, "Size", vec![size_row]).into_any_element(),
            rustyedit_ui::section(&theme, "Destination", vec![destination_row]).into_any_element(),
        ];

        // The Export action sits in the panel's footer, like the other
        // modules' footers.
        let footer = rustyedit_ui::panel_footer(
            &theme,
            vec![vec![div()
                .id("export-run")
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .h(px(26.0))
                .rounded_md()
                .text_size(px(11.5))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .when(count > 0 && options.destination.is_some(), |el| {
                    el.bg(theme.accent)
                        .text_color(theme.background)
                        .cursor_pointer()
                        .hover(|el| el.opacity(0.9))
                        .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.start_export(cx)))
                })
                .when(!(count > 0 && options.destination.is_some()), |el| {
                    el.bg(theme.surface_elevated).text_color(theme.text_faint)
                })
                .child(format!(
                    "Export {count} photo{}",
                    if count == 1 { "" } else { "s" }
                ))
                .into_any_element()]],
        );

        let panel = rustyedit_ui::side_panel(
            "export-side-panel",
            &theme,
            rustyedit_ui::PanelSide::Left,
            self.settings.layout.inspector_width as f32,
            &self.export_panel_scroll,
            None,
            Some(footer),
            sections,
        )
        .into_any_element();

        // The centre column: what is going to be exported, and how it is
        // going.
        let mut summary: Vec<AnyElement> = vec![
            summary_row(
                &theme,
                "Photos",
                format!("{count} of {}", self.catalog.assets.len()),
            ),
            summary_row(&theme, "Format", options.format.label().to_string()),
            summary_row(&theme, "Size", options.size.label().to_string()),
            summary_row(
                &theme,
                "Destination",
                options
                    .destination
                    .as_deref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "No folder chosen".into()),
            ),
        ];
        if let Some(message) = &self.export_message {
            summary.push(
                div()
                    .text_sm()
                    .text_color(theme.text_muted)
                    .child(message.clone())
                    .into_any_element(),
            );
        }
        if let Some(progress) = progress {
            summary.push(progress);
        }

        let center = div()
            .id("export-page")
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .overflow_y_scroll()
            .bg(theme.background)
            .items_center()
            .px_4()
            .py_8()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .w_full()
                    .max_w(px(560.0))
                    .child(div().text_xl().text_color(theme.text).child("Export"))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .p_4()
                            .rounded_xl()
                            .bg(theme.surface)
                            .border_1()
                            .border_color(theme.border_subtle)
                            .children(summary),
                    ),
            )
            .into_any_element();

        let main = div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h(px(0.0))
            .child(panel)
            .child(center);
        self.with_filmstrip(main.into_any_element(), cx)
    }
}

/// One "label on the left, value on the right" line of the export summary.
fn summary_row(theme: &rustyedit_ui::Theme, label: &str, value: String) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_3()
        .child(
            div()
                .text_sm()
                .text_color(theme.text_muted)
                .child(label.to_string()),
        )
        .child(
            div()
                .min_w(px(0.0))
                .overflow_hidden()
                .text_sm()
                .text_color(theme.text)
                .child(value),
        )
        .into_any_element()
}

fn labelled(theme: &rustyedit_ui::Theme, label: &'static str, control: AnyElement) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .child(div().text_sm().text_color(theme.text_muted).child(label))
        .child(control)
        .into_any_element()
}

/// Renders one photo through its develop settings and writes it into
/// `destination`, never overwriting an existing file.
fn export_one(asset: &Asset, options: &ExportOptions, destination: &Path) -> anyhow::Result<()> {
    let source = rustyedit_io::open_image(&asset.path)?;
    let rendered = DynamicImage::ImageRgb8(rustyedit_processing::apply(&source, &asset.develop));
    let rendered = match options.size.0 {
        Some(edge) if rendered.width().max(rendered.height()) > edge => {
            rendered.resize(edge, edge, image::imageops::FilterType::Lanczos3)
        }
        _ => rendered,
    };
    let rgb = rendered.to_rgb8();

    let stem = asset
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "photo".into());
    let target = unique_path(destination, &stem, options.format.extension());
    let file = std::io::BufWriter::new(std::fs::File::create(&target)?);

    match options.format {
        ExportFormat::Jpeg => JpegEncoder::new_with_quality(file, options.quality).write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ExtendedColorType::Rgb8,
        )?,
        ExportFormat::Png => image::codecs::png::PngEncoder::new(file).write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ExtendedColorType::Rgb8,
        )?,
    }
    Ok(())
}

fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stem}.{ext}"));
    let mut n = 1;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}-{n}.{ext}"));
        n += 1;
    }
    candidate
}
