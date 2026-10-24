//! The Export module: choose which photos, format, size, naming and
//! destination in the left panel; the centre previews exactly what will be
//! written, then shows each photo working, done or failed while the export
//! runs. Every photo is rendered through its saved develop settings at full
//! resolution.

use rustyedit_ui::sp;
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
use image::{codecs::jpeg::JpegEncoder, ExtendedColorType, ImageEncoder};
use rustyedit_core::{
    Asset, ExportBitDepth, ExportColorSpace, ExportConflict, ExportFormat, ExportNaming, ExportOptions,
    ExportPngCompression,
    ExportScope, ExportSharpen, ExportSize, ExportSubfolder, Flag,
};
use rustyedit_ui::Theme;

use crate::workspace::Workspace;

/// Most tiles drawn in the preview / run grid; the totals still count all.
const MAX_TILES: usize = 160;
/// Smallest and largest tile width, and the gap between tiles.
pub(crate) const TILE_MIN: f32 = 150.0;
/// Largest the tile size slider goes.
const TILE_MAX: f32 = 480.0;
const TILE_GAP: f32 = 16.0;

/// The range the footer's size slider spans, in pixels.
pub(crate) fn tile_size_range() -> (f32, f32) {
    (TILE_MIN, TILE_MAX)
}

/// A tile width that makes whole tiles fill `avail` exactly, row after row.
fn tile_width(avail: f32, min: f32) -> (f32, usize) {
    let cols = ((avail + TILE_GAP) / (min + TILE_GAP))
        .floor()
        .max(1.0);
    (
        ((avail - TILE_GAP * (cols - 1.0)) / cols).floor(),
        cols as usize,
    )
}

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
    id: u64,
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
        // Photos clicked in the filmstrip join the queue after the rest.
        for &id in &self.export_added {
            if let Some(ix) = assets.iter().position(|a| a.id == id) {
                if !indices.contains(&ix) {
                    indices.push(ix);
                }
            }
        }
        indices.retain(|&ix| {
            assets.get(ix).is_some_and(|a| {
                !a.missing
                    && !(skip_rejected && a.flag == Flag::Rejected)
                    && !self.export_excluded.contains(&a.id)
            })
        });
        indices
    }

    /// In the Export module, clicking a photo in the filmstrip adds it to
    /// the queue (bringing it back if it was removed) rather than
    /// replacing what's queued.
    pub(crate) fn add_to_export_queue(&mut self, ix: usize) {
        // With "Selected", the queue *is* the selection, which the click is
        // about to replace: keep what's there.
        if self.export_options.scope == ExportScope::Selected {
            let current: Vec<u64> = self
                .target_indices()
                .into_iter()
                .filter_map(|i| self.catalog.assets.get(i).map(|a| a.id))
                .collect();
            for id in current {
                if !self.export_added.contains(&id) {
                    self.export_added.push(id);
                }
            }
        }
        if let Some(id) = self.catalog.assets.get(ix).map(|a| a.id) {
            self.export_excluded.remove(&id);
            if !self.export_added.contains(&id) {
                self.export_added.push(id);
            }
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
                // Async: on its own thread, as the blocking dialog fails on
                // Windows' background threads (see `start_import`).
                .spawn(async {
                    rfd::AsyncFileDialog::new()
                        .pick_folder()
                        .await
                        .map(|handle| handle.path().to_path_buf())
                })
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
        // The watermark designed in Settings, if this export uses it.
        let watermark = (self.settings.export.watermark_enabled && options.watermark)
            .then(|| self.settings.export.watermark.clone());
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
                            let result = export_one(
                                asset,
                                *n,
                                &options,
                                watermark.as_ref(),
                                &destination,
                                &claimed,
                            );
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

    pub(crate) fn render_export(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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

        if options.format != ExportFormat::Bmp {
            format_children.push(labelled(&theme, "Colour space", div().into_any_element()));
            format_children.push(option_row(
                &theme,
                &entity,
                "export-color-space",
                &ExportColorSpace::ALL,
                options.color_space,
                |c| c.label().to_string(),
                |o, v| o.color_space = v,
            ));
        }
        // 16 bits per channel, for the formats that have them.
        if matches!(options.format, ExportFormat::Png | ExportFormat::Tiff) {
            format_children.push(labelled(&theme, "Bit depth", div().into_any_element()));
            format_children.push(option_row(
                &theme,
                &entity,
                "export-bit-depth",
                &ExportBitDepth::ALL,
                options.bit_depth,
                |d| d.label().to_string(),
                |o, v| o.bit_depth = v,
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
                    .min_w(sp(0.0))
                    .overflow_hidden()
                    .px_3()
                    .h(sp(28.0))
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

        let presets = self.export_presets_section(cx);
        let watermark = self.export_watermark_section(cx);
        let sections: Vec<AnyElement> = presets
            .into_iter()
            .chain([
            export_section(&theme, "Photos", vec![scope_row, skip_rejected]).into_any_element(),
            export_section(&theme, "Format", format_children).into_any_element(),
            export_section(&theme, "Size", vec![size_row]).into_any_element(),
            export_section(&theme, "Output sharpening", vec![sharpen_row]).into_any_element(),
            export_section(&theme, "File naming", vec![naming_row]).into_any_element(),
            export_section(&theme, "Subfolders", vec![subfolder_row]).into_any_element(),
            export_section(&theme, "If a file exists", vec![conflict_row]).into_any_element(),
            export_section(&theme, "Destination", vec![destination_row, open_when_done])
                .into_any_element(),
            ])
            .chain(watermark)
            .collect();

        // The action sits in the panel's footer, like the other modules'.
        let footer_button = div()
            .id("export-run")
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .h(sp(26.0))
            .rounded_md()
            .text_size(sp(11.5))
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
                .text_color(gpui::white())
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
        // Width left for tiles: the window minus the sidebar, page padding
        // (24 + 24) and room for the scrollbar.
        let avail = (f32::from(window.viewport_size().width)
            - self.settings.layout.inspector_width as f32
            - 64.0)
            .max(TILE_MIN);
        let (tile_w, cols) = tile_width(avail, self.export_tile_size.clamp(TILE_MIN, TILE_MAX));
        // The tiles are dressed like the filmstrip, in whatever style it's set to.
        let look = rustyedit_ui::FilmLook::for_style(self.settings.filmstrip.style, &theme);
        let show_edge_print = self.settings.filmstrip.show_edge_print;

        // `top` is pinned above the photos, `bottom` (run progress) below them;
        // `body` scrolls between.
        let mut top: Vec<AnyElement> = Vec::new();
        let mut bottom: Vec<AnyElement> = Vec::new();
        let mut body: Vec<AnyElement> = Vec::new();
        if let Some(run) = &self.export_run {
            bottom.push(self.run_card(run, cx));
            let statuses = run.status.lock().unwrap().clone();
            // Only a window of tiles is drawn; slide it down with the
            // export so the photos being worked on are always on show.
            let total = statuses.len();
            let active = statuses
                .iter()
                .position(|s| matches!(s, ItemStatus::Pending | ItemStatus::Working))
                .unwrap_or(total);
            let start = active
                .saturating_sub(cols)
                .min(total.saturating_sub(MAX_TILES));
            let start = start - start % cols;
            let end = (start + MAX_TILES).min(total);
            if total > MAX_TILES {
                body.push(
                    div()
                        .text_xs()
                        .text_color(theme.text_faint)
                        .child(format!(
                            "Showing photos {}\u{2013}{end} of {total}",
                            start + 1
                        ))
                        .into_any_element(),
                );
            }
            body.push(tile_grid(
                &theme,
                &look,
                show_edge_print,
                &run.tiles[start..end],
                Some(&statuses[start..end]),
                tile_w,
                start,
                None,
            ));
        } else {
            let tiles: Vec<Tile> = selected_indices
                .iter()
                .enumerate()
                .take(MAX_TILES)
                .map(|(n, &ix)| preview_tile(&self.catalog.assets[ix], n, &options))
                .collect();
            top.push(self.preview_card(&options, count, &destination_text, cx));
            bottom.push(self.queue_bar(count, cx));
            if let Some(message) = &self.export_message {
                top.push(
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
                body.push(tile_grid(&theme, &look, show_edge_print, &tiles, None, tile_w, 0, Some(&entity)));
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

        let has_top = !top.is_empty();
        let top_bar = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap_2()
            .px_4()
            .py_2()
            .bg(theme.surface)
            .border_b_1()
            .border_color(theme.border)
            .children(top);

        let center = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(sp(0.0))
            .min_h(sp(0.0))
            .bg(theme.background)
            .when(has_top, |el| el.child(top_bar))
            .child(
                div()
                    .id("export-page")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(sp(0.0))
                    .overflow_y_scroll()
                    .gap_4()
                    .px_6()
                    .py_4()
                    .children(body),
            )
            .when(!bottom.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .px_4()
                        .py_2()
                        .bg(theme.surface)
                        .border_t_1()
                        .border_color(theme.border)
                        .children(bottom),
                )
            })
            .into_any_element();

        let main = div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h(sp(0.0))
            .child(panel)
            .child(center);
        self.with_filmstrip(main.into_any_element(), cx)
    }

    /// The summary above the preview grid: what will be written and where.
    fn preview_card(
        &self,
        options: &ExportOptions,
        count: usize,
        destination: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let mut pills = vec![pill(
            &theme,
            if options.format.is_lossy() {
                format!("{} {}", options.format.label(), options.quality)
            } else {
                options.format.label().to_string()
            },
        )];
        pills.push(pill(&theme, options.size.label()));
        if output_space(options) != rustyedit_processing::OutputSpace::Srgb {
            pills.push(pill(&theme, options.color_space.label().to_string()));
        }
        if options.sharpen != ExportSharpen::Off {
            pills.push(pill(&theme, format!("Sharpen {}", options.sharpen.label())));
        }
        pills.push(pill(&theme, options.naming.label().to_string()));
        if options.subfolder != ExportSubfolder::None {
            pills.push(pill(
                &theme,
                format!("By {}", options.subfolder.label().to_lowercase()),
            ));
        }
        if options.conflict != ExportConflict::Rename {
            pills.push(pill(
                &theme,
                format!("{} existing", options.conflict.label()),
            ));
        }
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_x_3()
            .gap_y_2()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_baseline()
                    .gap_1()
                    .child(
                        div()
                            .text_size(sp(18.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child(count.to_string()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text_faint)
                            .child(format!("of {} photos", self.catalog.assets.len())),
                    ),
            )
            .child(div().w(sp(1.0)).h(sp(14.0)).bg(theme.border))
            .children(pills)
            .child(
                div()
                    .flex_1()
                    .min_w(sp(80.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_xs()
                    .text_color(theme.text_faint)
                    .text_right()
                    .child(destination.to_string()),
            )
            .child(
                div()
                    .id("export-folder-icon")
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .size(sp(28.0))
                    .rounded_md()
                    .cursor_pointer()
                    .border_1()
                    .when(options.destination.is_some(), |el| {
                        el.border_color(theme.border).bg(theme.background)
                    })
                    .when(options.destination.is_none(), |el| {
                        el.border_color(theme.accent).bg(theme.accent_muted)
                    })
                    .hover(|el| el.bg(theme.surface_elevated))
                    .tooltip(rustyedit_ui::tip(
                        format!("Export folder: {destination}"),
                        &theme,
                    ))
                    .child(rustyedit_ui::icon(
                        rustyedit_ui::Icon::Folder,
                        16.0,
                        if options.destination.is_some() {
                            theme.text_muted
                        } else {
                            theme.text
                        },
                    ))
                    .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                        this.choose_export_destination(cx)
                    })),
            )
            .into_any_element()
    }

    /// Bottom bar while previewing: how many photos are queued, and a way to
    /// bring back any that were removed.
    fn queue_bar(&self, count: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let removed = self.export_excluded.len();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .h(sp(28.0))
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child(format!(
                        "{count} photo{} in the queue",
                        if count == 1 { "" } else { "s" }
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .text_xs()
                    .text_color(theme.text_faint)
                    .child("Hover a photo and click \u{2715} to remove it from the export."),
            )
            .when(count > 0, |el| {
                el.child(rustyedit_ui::chrome_button(
                    "export-clear-queue",
                    "Remove all",
                    false,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| {
                        // Take every photo the current selection would export
                        // out of the queue; "Restore" brings them back.
                        let ids: Vec<u64> = this
                            .scope_indices(this.export_options.scope)
                            .into_iter()
                            .filter_map(|ix| this.catalog.assets.get(ix).map(|a| a.id))
                            .collect();
                        this.export_excluded.extend(ids);
                        this.export_added.clear();
                        cx.notify();
                    }),
                ))
            })
            .when(removed > 0, |el| {
                el.child(rustyedit_ui::chrome_button(
                    "export-restore",
                    format!("Restore {removed} removed"),
                    false,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| {
                        this.export_excluded.clear();
                        cx.notify();
                    }),
                ))
            })
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
                    .w(sp(60.0))
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
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_x_4()
            .gap_y_2()
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(colour)
                    .child(headline),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(sp(120.0))
                    .h(sp(5.0))
                    .rounded_full()
                    .bg(theme.border)
                    .child(fill),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(theme.text_muted)
                    .child(format!("{}%", (fraction * 100.0).round() as u32)),
            )
            .child(dot_count(&theme, done, "done", theme.pick))
            .when(failed > 0, |el| {
                el.child(dot_count(&theme, failed, "failed", theme.danger))
            })
            .when(skipped > 0, |el| {
                el.child(dot_count(&theme, skipped, "skipped", theme.star))
            })
            .when(running && working > 0, |el| {
                el.child(dot_count(&theme, working, "working", theme.accent))
            })
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_faint)
                    .child(match eta {
                        Some(eta) => format!(
                            "{} \u{b7} {} left \u{b7} {}",
                            fmt_secs(elapsed as u64),
                            fmt_secs(eta),
                            fmt_bytes(bytes)
                        ),
                        None => format!("{} \u{b7} {}", fmt_secs(elapsed as u64), fmt_bytes(bytes)),
                    }),
            )
            .when(!buttons.is_empty(), |el| {
                el.child(div().flex().flex_row().gap_1().children(buttons))
            })
            .into_any_element()
    }
}

/// A coloured dot and "12 done" style count.
fn dot_count(theme: &Theme, value: usize, label: &'static str, colour: gpui::Hsla) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .child(div().size(sp(6.0)).rounded_full().bg(colour))
        .child(
            div()
                .text_xs()
                .text_color(theme.text_muted)
                .child(format!("{value} {label}")),
        )
        .into_any_element()
}

/// A small rounded tag for one setting.
fn pill(theme: &Theme, text: String) -> AnyElement {
    div()
        .flex_shrink_0()
        .px_2()
        .h(sp(20.0))
        .flex()
        .items_center()
        .rounded_full()
        .bg(theme.background)
        .border_1()
        .border_color(theme.border)
        .text_xs()
        .text_color(theme.text_muted)
        .child(text)
        .into_any_element()
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
    let theme = *theme;
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap_1()
        .p_1()
        .rounded_lg()
        .bg(theme.background)
        .border_1()
        .border_color(theme.border_subtle)
        .children(values.iter().enumerate().map(|(i, &value)| {
            let entity = entity.clone();
            let selected = value == current;
            div()
                .id(SharedString::from(format!("{id}-{i}")))
                .flex()
                .items_center()
                .justify_center()
                .h(sp(24.0))
                .px_2p5()
                .rounded_md()
                .text_xs()
                .cursor_pointer()
                .border_1()
                .when(selected, |el| {
                    el.bg(theme.accent_muted)
                        .border_color(theme.accent)
                        .text_color(theme.text)
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                })
                .when(!selected, |el| {
                    el.border_color(theme.background)
                        .text_color(theme.text_muted)
                        .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                })
                .child(label(value))
                .on_click(move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                    entity.update(cx, |this, cx| this.update_export(cx, |o| apply(o, value)));
                })
                .into_any_element()
        }))
        .into_any_element()
}

/// A titled group in the left panel, built from the same section header the
/// other modules use so the panels match.
fn export_section(
    theme: &Theme,
    title: &'static str,
    children: Vec<AnyElement>,
) -> impl IntoElement {
    rustyedit_ui::section(theme, title, children)
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

/// The photos as a wrapping grid of tiles. With `statuses`, each tile shows
/// its export state: dimmed while waiting, a sweeping shimmer while working,
/// a green tick when done, a red cross (and the reason) when it failed.
fn tile_grid(
    theme: &Theme,
    look: &rustyedit_ui::FilmLook,
    show_edge_print: bool,
    tiles: &[Tile],
    statuses: Option<&[ItemStatus]>,
    tile_w: f32,
    offset: usize,
    remove: Option<&Entity<Workspace>>,
) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(px(TILE_GAP))
        .children(tiles.iter().enumerate().take(MAX_TILES).map(|(i, tile)| {
            tile_view(
                theme,
                look,
                show_edge_print,
                offset + i,
                tile,
                statuses.and_then(|s| s.get(i)),
                tile_w,
                remove,
            )
        }))
        .into_any_element()
}

fn tile_view(
    theme: &Theme,
    look: &rustyedit_ui::FilmLook,
    show_edge_print: bool,
    ix: usize,
    tile: &Tile,
    status: Option<&ItemStatus>,
    tile_w: f32,
    remove: Option<&Entity<Workspace>>,
) -> AnyElement {
    let w = tile_w;
    let h = (tile_w * 0.75).floor();

    let border = match status {
        Some(ItemStatus::Done(_)) => theme.pick,
        Some(ItemStatus::Failed(_)) => theme.danger,
        Some(ItemStatus::Skipped) => theme.star,
        Some(ItemStatus::Working) => theme.accent,
        _ => theme.border_subtle,
    };
    let photo = match &tile.thumbnail {
        // Landscapes fill the frame; portraits are shown whole with black
        // at the sides.
        Some(path) => img(path.clone())
            .object_fit(if crate::filmstrip::thumb_is_portrait_of(path, (0, 0)) {
                ObjectFit::Contain
            } else {
                ObjectFit::Cover
            })
            .grayscale(look.monochrome)
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .into_any_element(),
        None => div()
            .size_full()
            .bg(theme.surface_elevated)
            .into_any_element(),
    };
    // Dressed like the filmstrip: its base, perforations, edge print, and
    // the print border of the instant style.
    let look = *look;
    let edge_print = look.edge_print.filter(|_| show_edge_print).map(|edge| {
        let (left, right) = (edge.label)(ix + 1);
        div()
            .flex()
            .flex_row()
            .flex_shrink_0()
            .justify_between()
            .h(px(crate::filmstrip::EDGE_PRINT_HEIGHT))
            .px(sp(6.0))
            .when(look.monospace, |el| el.font_family(crate::filmstrip::EDGE_PRINT_FONT))
            .text_size(sp(8.0))
            .text_color(edge.color)
            .child(SharedString::from(left))
            .child(SharedString::from(right))
    });
    let picture = div()
        .relative()
        .flex_1()
        .min_h(sp(0.0))
        .mx(px(look.picture_inset.min(6.0)))
        .overflow_hidden()
        .rounded(px(look.picture_radius))
        .bg(gpui::black())
        .border_1()
        .border_color(look.picture_border)
        .child(photo);
    let photo = div()
        .size_full()
        .flex()
        .flex_col()
        .bg(look.mat.unwrap_or(look.base_bottom))
        .when(look.sprockets.is_none(), |el| el.py(sp(4.0)))
        .children(look.sprockets.map(crate::filmstrip::sprocket_row))
        .children(edge_print)
        .child(picture)
        .children(look.sprockets.map(crate::filmstrip::sprocket_row))
        .into_any_element();

    let mut frame = div()
        .id(("export-tile", ix))
        .relative()
        .overflow_hidden()
        .flex_shrink_0()
        .w(px(w))
        .h(px(h))
        .rounded_lg()
        .bg(theme.background)
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
                    .w(sp(36.0))
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
                    .top(sp(4.0))
                    .right(sp(4.0))
                    .size(sp(20.0))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(colour)
                    .text_color(theme.background)
                    .text_size(sp(12.0))
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

    // Before an export starts, a hovered tile offers to leave the queue.
    let group = SharedString::from(format!("export-tile-group-{ix}"));
    if let Some(entity) = remove {
        let entity = entity.clone();
        let id = tile.id;
        let hover_group = group.clone();
        frame = frame.child(
            div()
                .id(("export-tile-remove", ix))
                .absolute()
                .top(sp(4.0))
                .left(sp(4.0))
                .size(sp(22.0))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .bg(hsla(0.0, 0.0, 0.0, 0.72))
                .text_color(gpui::white())
                .text_size(sp(12.0))
                .opacity(0.0)
                .group_hover(hover_group, |el| el.opacity(1.0))
                .hover(|el| el.bg(theme.danger))
                .child("\u{2715}")
                .on_click(move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                    entity.update(cx, |this, cx| {
                        this.export_excluded.insert(id);
                        cx.notify();
                    });
                }),
        );
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
        .group(group)
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap_1()
        .w(px(w))
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
        id: asset.id,
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
    watermark: Option<&rustyedit_core::Watermark>,
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

    let written = render_to(asset, options, watermark, file);
    match written {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) => {
            let _ = std::fs::remove_file(&target);
            Err(error)
        }
    }
}

/// The colour space a file in `options` is written in. BMP can't carry a
/// colour profile, so it's always sRGB.
fn output_space(options: &ExportOptions) -> rustyedit_processing::OutputSpace {
    use rustyedit_processing::OutputSpace;
    if options.format == ExportFormat::Bmp {
        return OutputSpace::Srgb;
    }
    match options.color_space {
        ExportColorSpace::Srgb => OutputSpace::Srgb,
        ExportColorSpace::DisplayP3 => OutputSpace::DisplayP3,
        ExportColorSpace::AdobeRgb => OutputSpace::AdobeRgb,
        ExportColorSpace::ProPhoto => OutputSpace::ProPhoto,
    }
}

fn render_to(
    asset: &Asset,
    options: &ExportOptions,
    watermark: Option<&rustyedit_core::Watermark>,
    file: std::fs::File,
) -> anyhow::Result<u64> {
    let source = rustyedit_io::open_for_develop(&asset.path)?;
    let space = output_space(options);
    // Develop (from the RAW data for RAW files), resize, output sharpening
    // and colour conversion in one GPU pass (or the CPU fallback).
    // PNG and TIFF can carry 16 bits per channel. A watermark is stamped on
    // 8-bit pixels, so those exports stay 8-bit.
    let deep = options.bit_depth == ExportBitDepth::Sixteen
        && matches!(options.format, ExportFormat::Png | ExportFormat::Tiff)
        && watermark.is_none();
    if deep {
        let image = rustyedit_processing::render_export16(
            &source,
            &asset.develop,
            options.size.0,
            options.sharpen.params(),
            &space,
        );
        return write_deep(image, options, &space, file);
    }
    let mut rgb = rustyedit_processing::render_export(
        &source,
        &asset.develop,
        options.size.0,
        options.sharpen.params(),
        &space,
    );
    if let Some(watermark) = watermark {
        rustyedit_processing::apply_watermark(&mut rgb, watermark);
    }
    let icc = space.icc();
    let tag = |encoder: &mut dyn ImageEncoderIcc| {
        if let Some(icc) = &icc {
            encoder.embed(icc.clone());
        }
    };

    let mut out = std::io::BufWriter::new(file);
    let (w, h) = (rgb.width(), rgb.height());
    match options.format {
        ExportFormat::Jpeg => {
            let mut encoder = JpegEncoder::new_with_quality(&mut out, options.quality);
            tag(&mut encoder);
            encoder.write_image(rgb.as_raw(), w, h, ExtendedColorType::Rgb8)?
        }
        ExportFormat::Png => {
            use image::codecs::png::{CompressionType, FilterType, PngEncoder};
            let compression = match options.png_compression {
                ExportPngCompression::Fast => CompressionType::Fast,
                ExportPngCompression::Balanced => CompressionType::Default,
                ExportPngCompression::Smallest => CompressionType::Best,
            };
            let mut encoder =
                PngEncoder::new_with_quality(&mut out, compression, FilterType::Adaptive);
            tag(&mut encoder);
            encoder.write_image(rgb.as_raw(), w, h, ExtendedColorType::Rgb8)?
        }
        ExportFormat::Tiff => {
            let mut encoder = image::codecs::tiff::TiffEncoder::new(&mut out);
            tag(&mut encoder);
            encoder.write_image(rgb.as_raw(), w, h, ExtendedColorType::Rgb8)?
        }
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

/// Writes a 16-bit PNG or TIFF, tagged with the colour space's profile.
fn write_deep(
    image: image::ImageBuffer<image::Rgb<u16>, Vec<u16>>,
    options: &ExportOptions,
    space: &rustyedit_processing::OutputSpace,
    file: std::fs::File,
) -> anyhow::Result<u64> {
    let (w, h) = image.dimensions();
    // The encoders take 16-bit samples as native-endian bytes.
    let bytes: Vec<u8> = image.as_raw().iter().flat_map(|v| v.to_ne_bytes()).collect();
    let icc = space.icc();
    let tag = |encoder: &mut dyn ImageEncoderIcc| {
        if let Some(icc) = &icc {
            encoder.embed(icc.clone());
        }
    };
    let mut out = std::io::BufWriter::new(file);
    match options.format {
        ExportFormat::Png => {
            use image::codecs::png::{CompressionType, FilterType, PngEncoder};
            let compression = match options.png_compression {
                ExportPngCompression::Fast => CompressionType::Fast,
                ExportPngCompression::Balanced => CompressionType::Default,
                ExportPngCompression::Smallest => CompressionType::Best,
            };
            let mut encoder =
                PngEncoder::new_with_quality(&mut out, compression, FilterType::Adaptive);
            tag(&mut encoder);
            encoder.write_image(&bytes, w, h, ExtendedColorType::Rgb16)?
        }
        _ => {
            let mut encoder = image::codecs::tiff::TiffEncoder::new(&mut out);
            tag(&mut encoder);
            encoder.write_image(&bytes, w, h, ExtendedColorType::Rgb16)?
        }
    }
    out.flush()?;
    let file = out
        .into_inner()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(file.metadata()?.len())
}

/// Embedding an ICC profile, for any encoder that can.
trait ImageEncoderIcc {
    fn embed(&mut self, icc: Vec<u8>);
}

impl<E: ImageEncoder> ImageEncoderIcc for E {
    fn embed(&mut self, icc: Vec<u8>) {
        // Every format offered with a colour space supports profiles; if
        // one didn't, the file would simply be untagged.
        let _ = self.set_icc_profile(icc);
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageDecoder;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rustyedit-export-{name}-{}", std::process::id()))
    }

    /// A 16-bit file keeps its depth and carries the space's profile.
    #[test]
    fn deep_png_and_tiff_are_16_bit_and_tagged() {
        let image = image::ImageBuffer::from_fn(8, 4, |x, y| {
            image::Rgb([(x * 8000) as u16, (y * 16000) as u16, 40_000])
        });
        for (format, ext) in [(ExportFormat::Png, "png"), (ExportFormat::Tiff, "tif")] {
            let options = ExportOptions {
                format,
                bit_depth: ExportBitDepth::Sixteen,
                ..ExportOptions::default()
            };
            let path = temp(ext).with_extension(ext);
            let file = std::fs::File::create(&path).unwrap();
            write_deep(
                image.clone(),
                &options,
                &rustyedit_processing::OutputSpace::AdobeRgb,
                file,
            )
            .unwrap();

            let mut decoder = image::ImageReader::open(&path)
                .unwrap()
                .with_guessed_format()
                .unwrap()
                .into_decoder()
                .unwrap();
            assert_eq!(
                decoder.color_type(),
                image::ColorType::Rgb16,
                "{ext} should be 16-bit"
            );
            // PNG stores the profile compressed; read it back. TIFF stores it
            // as a plain field that this decoder doesn't surface, so look for
            // the profile's "acsp" signature in the file.
            let tagged = if format == ExportFormat::Png {
                decoder
                    .icc_profile()
                    .unwrap()
                    .is_some_and(|profile| profile.len() > 100)
            } else {
                std::fs::read(&path)
                    .unwrap()
                    .windows(4)
                    .any(|w| w == b"acsp")
            };
            assert!(tagged, "{ext} should carry a profile");
            let decoded = image::DynamicImage::from_decoder(decoder).unwrap().to_rgb16();
            assert_eq!(decoded.get_pixel(3, 2).0, image.get_pixel(3, 2).0, "{ext}");
            let _ = std::fs::remove_file(&path);
        }
    }
}
