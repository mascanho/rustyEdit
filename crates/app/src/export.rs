//! The Export module: choose which photos, format, size, naming and
//! destination in the left panel; the centre previews exactly what will be
//! written, then shows each photo working, done or failed while the export
//! runs. Every photo is rendered through its saved develop settings at full
//! resolution.

use std::{
    collections::HashSet,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use gpui::{
    div, hsla, img, prelude::*, px, relative, Animation, AnimationExt, AnyElement, App, ClickEvent,
    Context, Entity, ObjectFit, SharedString, Window,
};
use image::{codecs::jpeg::JpegEncoder, DynamicImage, ExtendedColorType, ImageEncoder};
use rustyedit_core::{
    Asset, ExportConflict, ExportFormat, ExportNaming, ExportOptions, ExportPngCompression,
    ExportScope, ExportSharpen, ExportSize, ExportSubfolder, Flag,
};
use rustyedit_ui::Theme;

use crate::workspace::Workspace;

/// Most tiles drawn in the preview / run grid; the totals still count all.
const MAX_TILES: usize = 160;

/// Where one photo of a running export has got to.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ItemStatus {
    Pending,
    Working,
    /// Written; the file's size in bytes.
    Done(u64),
    /// The target already existed and the conflict rule said to leave it.
    Skipped,
    Failed(String),
}

/// What a tile needs to draw one photo of the export.
struct Tile {
    thumbnail: Option<PathBuf>,
    /// File name the photo will be written as.
    name: String,
    dims: String,
}

/// One export in flight, or just finished and still on show.
pub(crate) struct ExportRun {
    /// Position in the export (drives sequence naming) and the photo.
    jobs: Vec<(usize, Asset)>,
    tiles: Vec<Tile>,
    status: Arc<Mutex<Vec<ItemStatus>>>,
    cancel: Arc<AtomicBool>,
    destination: PathBuf,
    started: Instant,
    /// Set once the worker has finished (or was cancelled).
    finished: Option<Duration>,
    cancelled: bool,
}

impl ExportRun {
    fn elapsed(&self) -> Duration {
        self.finished.unwrap_or_else(|| self.started.elapsed())
    }
}

impl Workspace {
    /// Indices into the catalog of the photos `scope` selects.
    fn scope_indices(&self, scope: ExportScope) -> Vec<usize> {
        let assets = &self.catalog.assets;
        let skip_rejected = self.export_options.skip_rejected;
        let mut indices: Vec<usize> = match scope {
            ExportScope::Selected => {
                if self.catalog.selected_asset().is_none() {
                    return Vec::new();
                }
                self.target_indices()
            }
            _ => (0..assets.len())
                .filter(|&ix| match scope {
                    ExportScope::Picked => assets[ix].flag == Flag::Picked,
                    ExportScope::TopRated => assets[ix].rating.value() >= 3,
                    ExportScope::Edited => assets[ix].develop.is_edited(),
                    _ => true,
                })
                .collect(),
        };
        indices.retain(|&ix| {
            assets
                .get(ix)
                .is_some_and(|a| !a.missing && !(skip_rejected && a.flag == Flag::Rejected))
        });
        indices
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
        if self
            .export_run
            .as_ref()
            .is_some_and(|r| r.finished.is_none())
        {
            return;
        }
        let Some(destination) = self.export_options.destination.clone() else {
            self.export_message = Some("Choose a destination folder first.".into());
            cx.notify();
            return;
        };
        let jobs: Vec<(usize, Asset)> = self
            .scope_indices(self.export_options.scope)
            .into_iter()
            .enumerate()
            .map(|(n, ix)| (n, self.catalog.assets[ix].clone()))
            .collect();
        if jobs.is_empty() {
            self.export_message = Some("Nothing to export for this selection.".into());
            cx.notify();
            return;
        }
        self.run_export(jobs, destination, cx);
    }

    /// Exports only the photos that failed in the last run.
    fn retry_failed(&mut self, cx: &mut Context<Self>) {
        let Some(run) = &self.export_run else { return };
        let status = run.status.lock().unwrap().clone();
        let jobs: Vec<(usize, Asset)> = run
            .jobs
            .iter()
            .zip(status)
            .filter(|(_, s)| matches!(s, ItemStatus::Failed(_)))
            .map(|(job, _)| job.clone())
            .collect();
        let destination = run.destination.clone();
        if !jobs.is_empty() {
            self.run_export(jobs, destination, cx);
        }
    }

    fn run_export(
        &mut self,
        jobs: Vec<(usize, Asset)>,
        destination: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let options = self.export_options.clone();
        let tiles = jobs
            .iter()
            .map(|(n, asset)| preview_tile(asset, *n, &options))
            .collect();
        let status = Arc::new(Mutex::new(vec![ItemStatus::Pending; jobs.len()]));
        let cancel = Arc::new(AtomicBool::new(false));
        self.export_message = None;
        self.export_run = Some(ExportRun {
            jobs: jobs.clone(),
            tiles,
            status: status.clone(),
            cancel: cancel.clone(),
            destination: destination.clone(),
            started: Instant::now(),
            finished: None,
            cancelled: false,
        });
        cx.notify();

        let job = cx.background_executor().spawn({
            let options = options.clone();
            let destination = destination.clone();
            async move {
                let next = AtomicUsize::new(0);
                let claimed = Mutex::new(HashSet::new());
                let workers = std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(2)
                    .clamp(1, 3);
                std::thread::scope(|scope| {
                    for _ in 0..workers {
                        scope.spawn(|| loop {
                            if cancel.load(Ordering::Relaxed) {
                                break;
                            }
                            let i = next.fetch_add(1, Ordering::Relaxed);
                            let Some((n, asset)) = jobs.get(i) else { break };
                            status.lock().unwrap()[i] = ItemStatus::Working;
                            let result = export_one(asset, *n, &options, &destination, &claimed);
                            status.lock().unwrap()[i] = match result {
                                Ok(Some(bytes)) => ItemStatus::Done(bytes),
                                Ok(None) => ItemStatus::Skipped,
                                Err(error) => ItemStatus::Failed(error.to_string()),
                            };
                        });
                    }
                });
            }
        });

        cx.spawn(async move |this, cx| {
            // Repaint often so the progress bar and tile effects move.
            let ticker = cx.spawn({
                let this = this.clone();
                async move |cx| {
                    while this.update(cx, |_, cx| cx.notify()).is_ok() {
                        cx.background_executor()
                            .timer(Duration::from_millis(60))
                            .await;
                    }
                }
            });
            job.await;
            drop(ticker);
            this.update(cx, |this, cx| {
                let open = this.export_options.open_when_done;
                if let Some(run) = &mut this.export_run {
                    run.cancelled = run.cancel.load(Ordering::Relaxed);
                    run.finished = Some(run.started.elapsed());
                    if open {
                        reveal(&run.destination);
                    }
                }
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
        let entity = cx.entity();
        let running = self
            .export_run
            .as_ref()
            .is_some_and(|r| r.finished.is_none());
        let selected_indices = self.scope_indices(options.scope);
        let count = selected_indices.len();
        let ready = count > 0 && options.destination.is_some();

        let scope_counts: Vec<(ExportScope, usize)> = ExportScope::ALL
            .iter()
            .map(|&s| (s, self.scope_indices(s).len()))
            .collect();

        let scope_row = option_row(
            &theme,
            &entity,
            "export-scope",
            &ExportScope::ALL,
            options.scope,
            |s| {
                let n = scope_counts
                    .iter()
                    .find(|(x, _)| *x == s)
                    .map_or(0, |(_, n)| *n);
                format!("{} ({n})", s.label())
            },
            |o, v| o.scope = v,
        );
        let skip_rejected = toggle_row(
            &theme,
            &entity,
            "export-skip-rejected",
            "Skip rejected",
            options.skip_rejected,
            |o| o.skip_rejected = !o.skip_rejected,
        );

        let mut format_children = vec![option_row(
            &theme,
            &entity,
            "export-format",
            &ExportFormat::ALL,
            options.format,
            |f| f.label().to_string(),
            |o, v| o.format = v,
        )];
        if options.format.is_lossy() {
            let stepper_entity = entity.clone();
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
                        stepper_entity.update(cx, |this, cx| {
                            this.update_export(cx, |o| o.quality = value.clamp(1, 100) as u8)
                        });
                    },
                )
                .into_any_element(),
            ));
            format_children.push(option_row(
                &theme,
                &entity,
                "export-quality-preset",
                &[60u8, 75, 90, 100],
                options.quality,
                |q| match q {
                    60 => "Small".to_string(),
                    75 => "Web".to_string(),
                    90 => "High".to_string(),
                    _ => "Max".to_string(),
                },
                |o, v| o.quality = v,
            ));
        }
        if options.format == ExportFormat::Png {
            format_children.push(labelled(&theme, "Compression", div().into_any_element()));
            format_children.push(option_row(
                &theme,
                &entity,
                "export-png",
                &ExportPngCompression::ALL,
                options.png_compression,
                |c| c.label().to_string(),
                |o, v| o.png_compression = v,
            ));
        }

        let size_row = option_row(
            &theme,
            &entity,
            "export-size",
            &ExportSize::PRESETS,
            options.size,
            |s| s.label(),
            |o, v| o.size = v,
        );
        let sharpen_row = option_row(
            &theme,
            &entity,
            "export-sharpen",
            &ExportSharpen::ALL,
            options.sharpen,
            |s| s.label().to_string(),
            |o, v| o.sharpen = v,
        );
        let naming_row = option_row(
            &theme,
            &entity,
            "export-naming",
            &ExportNaming::ALL,
            options.naming,
            |n| n.label().to_string(),
            |o, v| o.naming = v,
        );
        let subfolder_row = option_row(
            &theme,
            &entity,
            "export-subfolder",
            &ExportSubfolder::ALL,
            options.subfolder,
            |s| s.label().to_string(),
            |o, v| o.subfolder = v,
        );
        let conflict_row = option_row(
            &theme,
            &entity,
            "export-conflict",
            &ExportConflict::ALL,
            options.conflict,
            |c| c.label().to_string(),
            |o, v| o.conflict = v,
        );
        let open_when_done = toggle_row(
            &theme,
            &entity,
            "export-open",
            "Open folder when done",
            options.open_when_done,
            |o| o.open_when_done = !o.open_when_done,
        );

        let destination_text = options
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
                    .child(destination_text.clone()),
            )
            .child(rustyedit_ui::chrome_button(
                "export-choose",
                "Choose…",
                false,
                &theme,
                cx.listener(|this, _: &ClickEvent, _w, cx| this.choose_export_destination(cx)),
            ))
            .into_any_element();

        let sections: Vec<AnyElement> = vec![
            rustyedit_ui::section(&theme, "Photos", vec![scope_row, skip_rejected])
                .into_any_element(),
            rustyedit_ui::section(&theme, "Format", format_children).into_any_element(),
            rustyedit_ui::section(&theme, "Size", vec![size_row]).into_any_element(),
            rustyedit_ui::section(&theme, "Output sharpening", vec![sharpen_row])
                .into_any_element(),
            rustyedit_ui::section(&theme, "File naming", vec![naming_row]).into_any_element(),
            rustyedit_ui::section(&theme, "Subfolders", vec![subfolder_row]).into_any_element(),
            rustyedit_ui::section(&theme, "If a file exists", vec![conflict_row])
                .into_any_element(),
            rustyedit_ui::section(&theme, "Destination", vec![destination_row, open_when_done])
                .into_any_element(),
        ];

        // The action sits in the panel's footer, like the other modules'.
        let footer_button = div()
            .id("export-run")
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .h(px(26.0))
            .rounded_md()
            .text_size(px(11.5))
            .font_weight(gpui::FontWeight::SEMIBOLD);
        let footer_button = if running {
            footer_button
                .bg(theme.danger)
                .text_color(theme.text)
                .cursor_pointer()
                .hover(|el| el.opacity(0.9))
                .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                    if let Some(run) = &this.export_run {
                        run.cancel.store(true, Ordering::Relaxed);
                    }
                    cx.notify();
                }))
                .child("Cancel export")
        } else if ready {
            footer_button
                .bg(theme.accent)
                .text_color(theme.background)
                .cursor_pointer()
                .hover(|el| el.opacity(0.9))
                .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.start_export(cx)))
                .child(format!(
                    "Export {count} photo{}",
                    if count == 1 { "" } else { "s" }
                ))
        } else {
            footer_button
                .bg(theme.surface_elevated)
                .text_color(theme.text_faint)
                .child(format!(
                    "Export {count} photo{}",
                    if count == 1 { "" } else { "s" }
                ))
        };
        let footer =
            rustyedit_ui::panel_footer(&theme, vec![vec![footer_button.into_any_element()]]);

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

        // --- Centre ---------------------------------------------------------
        let mut body: Vec<AnyElement> = Vec::new();
        if let Some(run) = &self.export_run {
            body.push(self.run_card(run, cx));
            let statuses = run.status.lock().unwrap().clone();
            body.push(tile_grid(&theme, &run.tiles, Some(&statuses)));
        } else {
            let tiles: Vec<Tile> = selected_indices
                .iter()
                .enumerate()
                .take(MAX_TILES)
                .map(|(n, &ix)| preview_tile(&self.catalog.assets[ix], n, &options))
                .collect();
            body.push(self.preview_card(&options, count, &destination_text));
            if let Some(message) = &self.export_message {
                body.push(
                    div()
                        .text_sm()
                        .text_color(theme.danger)
                        .child(message.clone())
                        .into_any_element(),
                );
            }
            if tiles.is_empty() {
                body.push(
                    div()
                        .py_8()
                        .flex()
                        .justify_center()
                        .text_sm()
                        .text_color(theme.text_faint)
                        .child("No photos match this selection.")
                        .into_any_element(),
                );
            } else {
                body.push(tile_grid(&theme, &tiles, None));
                if count > tiles.len() {
                    body.push(
                        div()
                            .text_xs()
                            .text_color(theme.text_faint)
                            .child(format!("+ {} more not shown", count - tiles.len()))
                            .into_any_element(),
                    );
                }
            }
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
            .px_6()
            .py_6()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .w_full()
                    .max_w(px(980.0))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .pb_3()
                            .border_b_1()
                            .border_color(theme.border_subtle)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(22.0))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(theme.text)
                                            .child("Export"),
                                    )
                                    .child(div().text_sm().text_color(theme.text_faint).child(
                                        if self.export_run.is_some() {
                                            "Progress and results"
                                        } else {
                                            "Review what will be written, then export"
                                        },
                                    )),
                            )
                            .child(
                                div()
                                    .px_3()
                                    .py_1()
                                    .rounded_full()
                                    .bg(theme.accent_muted)
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child(if self.export_run.is_some() {
                                        "RESULTS"
                                    } else {
                                        "PREVIEW"
                                    }),
                            ),
                    )
                    .children(body),
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

    /// The summary above the preview grid: what will be written and where.
    fn preview_card(&self, options: &ExportOptions, count: usize, destination: &str) -> AnyElement {
        let theme = self.theme;
        let size = match options.size.0 {
            Some(edge) => format!("max {edge} px"),
            None => "Original size".into(),
        };
        let format = if options.format.is_lossy() {
            format!(
                "{} \u{b7} quality {}",
                options.format.label(),
                options.quality
            )
        } else {
            options.format.label().to_string()
        };
        let folders = if options.subfolder == ExportSubfolder::None {
            "None".to_string()
        } else {
            format!("By {}", options.subfolder.label().to_lowercase())
        };
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .rounded_xl()
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border_subtle)
            .border_t_2()
            .border_color(theme.accent)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_baseline()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(28.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child(count.to_string()),
                    )
                    .child(div().text_sm().text_color(theme.text_muted).child(format!(
                        "of {} photos will be exported",
                        self.catalog.assets.len()
                    ))),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_x_6()
                    .gap_y_2()
                    .child(stat(&theme, "Format", format))
                    .child(stat(&theme, "Size", size))
                    .child(stat(&theme, "Sharpen", options.sharpen.label().to_string()))
                    .child(stat(&theme, "Naming", options.naming.label().to_string()))
                    .child(stat(&theme, "Subfolders", folders))
                    .child(stat(
                        &theme,
                        "If exists",
                        options.conflict.label().to_string(),
                    )),
            )
            .child(stat(&theme, "Destination", destination.to_string()))
            .into_any_element()
    }

    /// Progress header: bar with a moving shimmer, counters, timing, and the
    /// buttons that follow a finished run.
    fn run_card(&self, run: &ExportRun, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let statuses = run.status.lock().unwrap().clone();
        let total = statuses.len();
        let done = statuses
            .iter()
            .filter(|s| matches!(s, ItemStatus::Done(_)))
            .count();
        let failed = statuses
            .iter()
            .filter(|s| matches!(s, ItemStatus::Failed(_)))
            .count();
        let skipped = statuses
            .iter()
            .filter(|s| **s == ItemStatus::Skipped)
            .count();
        let working = statuses
            .iter()
            .filter(|s| **s == ItemStatus::Working)
            .count();
        let processed = done + failed + skipped;
        let bytes: u64 = statuses
            .iter()
            .map(|s| if let ItemStatus::Done(b) = s { *b } else { 0 })
            .sum();
        let fraction = if total == 0 {
            0.0
        } else {
            processed as f32 / total as f32
        };
        let running = run.finished.is_none();
        let elapsed = run.elapsed().as_secs_f32();
        let eta = if running && processed > 0 {
            let rate = processed as f32 / elapsed.max(0.001);
            Some(((total - processed) as f32 / rate).ceil() as u64)
        } else {
            None
        };

        let (headline, colour) = if running {
            (
                format!("Exporting {processed} of {total}\u{2026}"),
                theme.text,
            )
        } else if run.cancelled {
            (
                format!("Cancelled after {processed} of {total}"),
                theme.star,
            )
        } else if failed == 0 {
            (
                format!("Exported {done} photo{}", if done == 1 { "" } else { "s" }),
                theme.pick,
            )
        } else {
            (
                format!("Exported {done} of {total}; {failed} failed"),
                theme.danger,
            )
        };

        let bar_colour = if !running && failed > 0 {
            theme.danger
        } else if !running {
            theme.pick
        } else {
            theme.accent
        };
        let mut fill = div()
            .relative()
            .overflow_hidden()
            .h_full()
            .w(relative(fraction))
            .rounded_full()
            .bg(bar_colour);
        if running {
            fill = fill.child(
                div()
                    .id("export-bar-shimmer")
                    .absolute()
                    .top_0()
                    .h_full()
                    .w(px(60.0))
                    .bg(hsla(0.0, 0.0, 1.0, 0.35))
                    .with_animation(
                        "export-bar-shimmer-anim",
                        Animation::new(Duration::from_millis(1100)).repeat(),
                        |el, delta| el.left(relative(delta * 1.2 - 0.2)),
                    ),
            );
        }

        let mut buttons: Vec<AnyElement> = Vec::new();
        if !running {
            if failed > 0 {
                buttons.push(
                    rustyedit_ui::chrome_button(
                        "export-retry",
                        format!("Retry {failed} failed"),
                        false,
                        &theme,
                        cx.listener(|this, _: &ClickEvent, _w, cx| this.retry_failed(cx)),
                    )
                    .into_any_element(),
                );
            }
            let destination = run.destination.clone();
            buttons.push(
                rustyedit_ui::chrome_button(
                    "export-reveal",
                    "Open folder",
                    false,
                    &theme,
                    move |_: &ClickEvent, _w: &mut Window, _cx: &mut App| reveal(&destination),
                )
                .into_any_element(),
            );
            buttons.push(
                rustyedit_ui::chrome_button(
                    "export-back",
                    "Back to preview",
                    true,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| {
                        this.export_run = None;
                        cx.notify();
                    }),
                )
                .into_any_element(),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .rounded_xl()
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(colour)
                            .child(headline),
                    )
                    .child(
                        div()
                            .text_lg()
                            .text_color(theme.text_muted)
                            .child(format!("{}%", (fraction * 100.0).round() as u32)),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .h(px(8.0))
                    .rounded_full()
                    .bg(theme.border)
                    .child(fill),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_x_6()
                    .gap_y_2()
                    .child(counter(&theme, "Done", done, theme.pick))
                    .child(counter(&theme, "Failed", failed, theme.danger))
                    .child(counter(&theme, "Skipped", skipped, theme.star))
                    .when(running, |el| {
                        el.child(counter(&theme, "Working", working, theme.accent))
                    })
                    .child(stat(&theme, "Time", fmt_secs(elapsed as u64)))
                    .when_some(eta, |el, eta| {
                        el.child(stat(&theme, "Remaining", fmt_secs(eta)))
                    })
                    .child(stat(&theme, "Written", fmt_bytes(bytes))),
            )
            .when(!buttons.is_empty(), |el| {
                el.child(div().flex().flex_row().gap_2().children(buttons))
            })
            .into_any_element()
    }
}

// --- UI helpers ---------------------------------------------------------------

/// A wrapping row of chips, one per value; the current one is highlighted.
fn option_row<T: Copy + PartialEq + 'static>(
    theme: &Theme,
    entity: &Entity<Workspace>,
    id: &'static str,
    values: &[T],
    current: T,
    label: impl Fn(T) -> String,
    apply: fn(&mut ExportOptions, T),
) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap_1()
        .children(values.iter().enumerate().map(|(i, &value)| {
            let entity = entity.clone();
            rustyedit_ui::chrome_button(
                SharedString::from(format!("{id}-{i}")),
                label(value),
                value == current,
                theme,
                move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                    entity.update(cx, |this, cx| this.update_export(cx, |o| apply(o, value)));
                },
            )
            .into_any_element()
        }))
        .into_any_element()
}

fn toggle_row(
    theme: &Theme,
    entity: &Entity<Workspace>,
    id: &'static str,
    label: &'static str,
    on: bool,
    flip: fn(&mut ExportOptions),
) -> AnyElement {
    let entity = entity.clone();
    labelled(
        theme,
        label,
        rustyedit_ui::toggle_switch(
            id,
            on,
            theme,
            move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                entity.update(cx, |this, cx| this.update_export(cx, flip));
            },
        )
        .into_any_element(),
    )
}

fn labelled(theme: &Theme, label: &'static str, control: AnyElement) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .child(div().text_sm().text_color(theme.text_muted).child(label))
        .child(control)
        .into_any_element()
}

/// A small caption over a value.
fn stat(theme: &Theme, label: &'static str, value: String) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .min_w(px(0.0))
        .child(div().text_xs().text_color(theme.text_faint).child(label))
        .child(
            div()
                .overflow_hidden()
                .text_sm()
                .text_color(theme.text)
                .child(value),
        )
        .into_any_element()
}

fn counter(theme: &Theme, label: &'static str, value: usize, colour: gpui::Hsla) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .child(div().text_xs().text_color(theme.text_faint).child(label))
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(if value > 0 { colour } else { theme.text_muted })
                .child(value.to_string()),
        )
        .into_any_element()
}

/// The photos as a wrapping grid of tiles. With `statuses`, each tile shows
/// its export state: dimmed while waiting, a sweeping shimmer while working,
/// a green tick when done, a red cross (and the reason) when it failed.
fn tile_grid(theme: &Theme, tiles: &[Tile], statuses: Option<&[ItemStatus]>) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .justify_center()
        .gap_4()
        .p_4()
        .rounded_xl()
        .bg(theme.surface)
        .border_1()
        .border_color(theme.border_subtle)
        .children(
            tiles
                .iter()
                .enumerate()
                .take(MAX_TILES)
                .map(|(i, tile)| tile_view(theme, i, tile, statuses.and_then(|s| s.get(i)))),
        )
        .into_any_element()
}

fn tile_view(theme: &Theme, ix: usize, tile: &Tile, status: Option<&ItemStatus>) -> AnyElement {
    const W: f32 = 148.0;
    const H: f32 = 111.0;

    let border = match status {
        Some(ItemStatus::Done(_)) => theme.pick,
        Some(ItemStatus::Failed(_)) => theme.danger,
        Some(ItemStatus::Skipped) => theme.star,
        Some(ItemStatus::Working) => theme.accent,
        _ => theme.border_subtle,
    };
    let photo = match &tile.thumbnail {
        Some(path) => img(path.clone())
            .object_fit(ObjectFit::Cover)
            .size_full()
            .into_any_element(),
        None => div()
            .size_full()
            .bg(theme.surface_elevated)
            .into_any_element(),
    };

    let mut frame = div()
        .id(("export-tile", ix))
        .relative()
        .overflow_hidden()
        .w(px(W))
        .h(px(H))
        .rounded_lg()
        .bg(theme.surface)
        .border_2()
        .border_color(border)
        .hover(|el| el.border_color(theme.text_muted))
        .child(photo);

    match status {
        Some(ItemStatus::Pending) => frame = frame.opacity(0.45),
        Some(ItemStatus::Working) => {
            frame = frame.child(
                div()
                    .id(("export-tile-shimmer", ix))
                    .absolute()
                    .top_0()
                    .h_full()
                    .w(px(36.0))
                    .bg(hsla(0.0, 0.0, 1.0, 0.28))
                    .with_animation(
                        ("export-tile-shimmer-anim", ix),
                        Animation::new(Duration::from_millis(900)).repeat(),
                        |el, delta| el.left(relative(delta * 1.5 - 0.3)),
                    ),
            );
        }
        Some(done @ (ItemStatus::Done(_) | ItemStatus::Skipped | ItemStatus::Failed(_))) => {
            let (glyph, colour) = match done {
                ItemStatus::Done(_) => ("\u{2713}", theme.pick),
                ItemStatus::Skipped => ("\u{2212}", theme.star),
                _ => ("\u{2715}", theme.danger),
            };
            if matches!(done, ItemStatus::Failed(_)) {
                frame = frame.child(div().absolute().size_full().bg(hsla(
                    5.0 / 360.0,
                    0.7,
                    0.3,
                    0.35,
                )));
            }
            // The badge pops in once the photo settles.
            frame = frame.child(
                div()
                    .id(("export-tile-badge", ix))
                    .absolute()
                    .top(px(4.0))
                    .right(px(4.0))
                    .size(px(20.0))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(colour)
                    .text_color(theme.background)
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(glyph)
                    .with_animation(
                        ("export-tile-badge-anim", ix),
                        Animation::new(Duration::from_millis(260))
                            .with_easing(gpui::ease_out_quint()),
                        |el, delta| el.opacity(delta).size(px(8.0 + 12.0 * delta)),
                    ),
            );
        }
        _ => {}
    }

    let caption = match status {
        Some(ItemStatus::Failed(reason)) => div()
            .text_xs()
            .text_color(theme.danger)
            .overflow_hidden()
            .whitespace_nowrap()
            .child(reason.clone()),
        Some(ItemStatus::Done(bytes)) => div()
            .text_xs()
            .text_color(theme.text_faint)
            .child(fmt_bytes(*bytes)),
        Some(ItemStatus::Skipped) => div()
            .text_xs()
            .text_color(theme.star)
            .child("Already exists"),
        _ => div()
            .text_xs()
            .text_color(theme.text_faint)
            .child(tile.dims.clone()),
    };

    div()
        .flex()
        .flex_col()
        .gap_1()
        .w(px(W))
        .child(frame)
        .child(
            div()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_xs()
                .text_color(theme.text_muted)
                .child(tile.name.clone()),
        )
        .child(caption)
        .into_any_element()
}

fn preview_tile(asset: &Asset, index: usize, options: &ExportOptions) -> Tile {
    let name = format!(
        "{}.{}",
        options.naming.stem(asset, index),
        options.format.extension()
    );
    let (w, h) = options.size.fit(asset.width, asset.height);
    let dims = if w == 0 || h == 0 {
        String::new()
    } else {
        format!("{w} \u{d7} {h}")
    };
    Tile {
        thumbnail: asset.thumbnail_path.clone(),
        name,
        dims,
    }
}

fn fmt_secs(secs: u64) -> String {
    if secs >= 60 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

fn fmt_bytes(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    if bytes as f64 >= 1024.0 * MB {
        format!("{:.2} GB", bytes as f64 / (1024.0 * MB))
    } else if bytes as f64 >= MB {
        format!("{:.1} MB", bytes as f64 / MB)
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

/// Shows `path` in the system file manager.
fn reveal(path: &Path) {
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let opener = "xdg-open";
    let _ = std::process::Command::new(opener).arg(path).spawn();
}

// --- Rendering ----------------------------------------------------------------

/// Renders one photo through its develop settings and writes it under
/// `destination`. Returns the bytes written, or `None` when the file already
/// existed and the conflict rule is to skip it.
fn export_one(
    asset: &Asset,
    index: usize,
    options: &ExportOptions,
    destination: &Path,
    claimed: &Mutex<HashSet<PathBuf>>,
) -> anyhow::Result<Option<u64>> {
    let dir = match options.subfolder.folder(asset) {
        Some(folder) => destination.join(folder),
        None => destination.to_path_buf(),
    };
    std::fs::create_dir_all(&dir)?;
    let stem = options.naming.stem(asset, index);
    let Some((target, file)) = claim_target(&dir, &stem, options, claimed)? else {
        return Ok(None);
    };

    let written = render_to(asset, options, file);
    match written {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) => {
            let _ = std::fs::remove_file(&target);
            Err(error)
        }
    }
}

fn render_to(asset: &Asset, options: &ExportOptions, file: std::fs::File) -> anyhow::Result<u64> {
    let source = rustyedit_io::open_image(&asset.path)?;
    let rendered = DynamicImage::ImageRgb8(rustyedit_processing::apply(&source, &asset.develop));
    let rendered = match options.size.0 {
        Some(edge) if rendered.width().max(rendered.height()) > edge => {
            rendered.resize(edge, edge, image::imageops::FilterType::Lanczos3)
        }
        _ => rendered,
    };
    let mut rgb = rendered.to_rgb8();
    if let Some((sigma, threshold)) = options.sharpen.params() {
        rgb = image::imageops::unsharpen(&rgb, sigma, threshold);
    }

    let mut out = std::io::BufWriter::new(file);
    let (w, h) = (rgb.width(), rgb.height());
    match options.format {
        ExportFormat::Jpeg => JpegEncoder::new_with_quality(&mut out, options.quality)
            .write_image(rgb.as_raw(), w, h, ExtendedColorType::Rgb8)?,
        ExportFormat::Png => {
            use image::codecs::png::{CompressionType, FilterType, PngEncoder};
            let compression = match options.png_compression {
                ExportPngCompression::Fast => CompressionType::Fast,
                ExportPngCompression::Balanced => CompressionType::Default,
                ExportPngCompression::Smallest => CompressionType::Best,
            };
            PngEncoder::new_with_quality(&mut out, compression, FilterType::Adaptive).write_image(
                rgb.as_raw(),
                w,
                h,
                ExtendedColorType::Rgb8,
            )?
        }
        ExportFormat::Tiff => image::codecs::tiff::TiffEncoder::new(&mut out).write_image(
            rgb.as_raw(),
            w,
            h,
            ExtendedColorType::Rgb8,
        )?,
        ExportFormat::Bmp => image::codecs::bmp::BmpEncoder::new(&mut out).write_image(
            rgb.as_raw(),
            w,
            h,
            ExtendedColorType::Rgb8,
        )?,
    }
    out.flush()?;
    let file = out
        .into_inner()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(file.metadata()?.len())
}

/// Picks and opens the output file according to the conflict rule. Paths
/// already used by this export are never reused, so two photos with the same
/// name can't overwrite each other. `None` means "skip this photo".
fn claim_target(
    dir: &Path,
    stem: &str,
    options: &ExportOptions,
    claimed: &Mutex<HashSet<PathBuf>>,
) -> anyhow::Result<Option<(PathBuf, std::fs::File)>> {
    let ext = options.format.extension();
    let mut claimed = claimed.lock().unwrap();
    let mut n = 0;
    loop {
        let path = if n == 0 {
            dir.join(format!("{stem}.{ext}"))
        } else {
            dir.join(format!("{stem}-{n}.{ext}"))
        };
        n += 1;
        if claimed.contains(&path) {
            continue;
        }
        if n == 1 && path.exists() {
            match options.conflict {
                ExportConflict::Skip => return Ok(None),
                ExportConflict::Overwrite => {}
                ExportConflict::Rename => continue,
            }
        }
        let file = if options.conflict == ExportConflict::Overwrite && n == 1 {
            std::fs::File::create(&path)?
        } else {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => file,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        };
        claimed.insert(path.clone());
        return Ok(Some((path, file)));
    }
}
