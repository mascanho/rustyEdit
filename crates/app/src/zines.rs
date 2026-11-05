//! The Zines module, between Develop and Export: photos picked with
//! right-click ▸ Add to zine (one or many) are laid out on pages and
//! written as a PDF (pages, or a sheet that folds into a booklet) or an
//! HTML page.
//!
//! Left: how it is made (format, layout, paper, text). Centre: the pages as
//! they will fall, then the photos in order, each of which can be moved or
//! taken out. Every photo is rendered through its develop settings, as in
//! Export.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use gpui::{
    div, img, prelude::*, px, relative, AnyElement, ClickEvent, Context, ObjectFit,
    Render, SharedString, Window,
};
use rustyedit_core::{
    Asset, Zine, ZineContent, ZineFormat, ZineLayout, ZineMode, ZineOptions, ZinePageLayout,
    ZinePhoto, ZineSpot,
};
use rustyedit_io::zine::ZineImage;
use rustyedit_ui::{sp, Icon, Theme};

use crate::workspace::{Mode, TextTarget, Workspace};

/// Pixels per inch the pictures are rendered at, and the least and most
/// long edge, so a zine stays a reasonable size.
const PRINT_DPI: f32 = 250.0;
const MIN_EDGE: u32 = 900;
const MAX_EDGE: u32 = 3000;
/// The long edge of pictures on a web page.
const WEB_EDGE: u32 = 2000;
/// The width of a photo tile.
const TILE_W: f32 = 168.0;

/// A zine being made.
pub(crate) struct ZineRun {
    done: Arc<AtomicUsize>,
    total: usize,
}

impl ZineRun {
    pub(crate) fn progress(&self) -> (usize, usize) {
        (self.done.load(Ordering::Relaxed), self.total)
    }
}

/// Manual pages are started from the automatic layout the first time they
/// are wanted, and again when the layout changes between a folded sheet
/// and ordinary pages (their slots mean different things).
pub(crate) fn sync_manual(zine: &mut Zine) {
    if zine.options.mode != ZineMode::Manual {
        return;
    }
    let wants_sheet = zine.options.layout == ZineLayout::FoldedSheet;
    let has_sheet = zine.manual.pages.iter().any(|p| p.is_sheet());
    if !zine.manual.started || (wants_sheet != has_sheet && !zine.manual.pages.is_empty()) {
        zine.seed_manual();
    }
}

/// A photo being dragged onto a page: from the tray, or from a slot.
#[derive(Clone)]
pub(crate) struct ZineDrag {
    pub(crate) photo: ZinePhoto,
    pub(crate) from: Option<ZineSpot>,
    pub(crate) label: String,
    pub(crate) theme: Theme,
}

impl Render for ZineDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .h(sp(26.0))
            .px_3()
            .rounded_md()
            .bg(theme.surface_elevated)
            .border_1()
            .border_color(theme.accent)
            .shadow_lg()
            .text_size(sp(11.5))
            .text_color(theme.text)
            .child(self.label.clone())
    }
}

fn zine_photo(asset: &Asset) -> ZinePhoto {
    ZinePhoto {
        path: asset.path.clone(),
        copy_name: asset.copy_name.clone(),
    }
}

impl Workspace {
    // --- Picking photos ------------------------------------------------------

    /// The right-click targets as zine photos.
    pub(crate) fn zine_targets(&self) -> Vec<ZinePhoto> {
        self.target_indices()
            .into_iter()
            .filter_map(|ix| self.catalog.assets.get(ix))
            .filter(|asset| !asset.missing)
            .map(zine_photo)
            .collect()
    }

    /// Whether every target photo is already in the zine.
    pub(crate) fn targets_in_zine(&self) -> bool {
        let targets = self.zine_targets();
        !targets.is_empty() && targets.iter().all(|p| self.zine.contains(p))
    }

    /// Right-click ▸ Add to zine: puts the target photos in, or takes them
    /// out again when they all are.
    pub(crate) fn toggle_in_zine(&mut self, cx: &mut Context<Self>) {
        let targets = self.zine_targets();
        if targets.is_empty() {
            return;
        }
        let message = if self.targets_in_zine() {
            let removed = self.zine.remove(&targets);
            format!("Took {removed} photo{} out of the zine", plural(removed))
        } else {
            let added = self.zine.add(targets);
            format!(
                "Added {added} photo{} to the zine ({} in all)",
                plural(added),
                self.zine.photos.len()
            )
        };
        self.zine_changed(cx);
        self.status_message = Some(message.into());
        cx.notify();
    }

    /// A photo dropped on a slot (or the cover).
    pub(crate) fn drop_on_spot(&mut self, drag: &ZineDrag, to: ZineSpot, cx: &mut Context<Self>) {
        let (photo, from) = (drag.photo.clone(), drag.from);
        self.update_zine(cx, |z| z.place(photo, to, from));
    }

    /// A photo dropped on a page, but not on one of its slots: the first
    /// empty one takes it.
    pub(crate) fn drop_on_page(&mut self, drag: &ZineDrag, page: usize, cx: &mut Context<Self>) {
        let Some(slots) = self.zine.manual.pages.get(page).map(|p| p.slots.len()) else {
            return;
        };
        let spot = (0..slots)
            .map(|slot| ZineSpot::Slot { page, slot })
            .find(|&spot| self.zine.at(spot).is_none() && !self.zine.is_fixed(spot));
        if let Some(to) = spot {
            self.drop_on_spot(drag, to, cx);
        }
    }

    pub(crate) fn go_zines(&mut self, cx: &mut Context<Self>) {
        self.mode = Mode::Zines;
        cx.notify();
    }

    pub(crate) fn zine_changed(&mut self, cx: &mut Context<Self>) {
        self.zine_message = None;
        self.zine_library.sync(&self.zine);
        if let Err(error) = rustyedit_io::zine::save_library(&self.zine_library) {
            eprintln!("Could not save the zine: {error}");
        }
        cx.notify();
    }

    /// Typing in the title or author field finished.
    pub(crate) fn commit_zine_text(&mut self, target: TextTarget, text: String, cx: &mut Context<Self>) {
        let text = text.trim().to_string();
        self.update_zine_options(cx, |o| match target {
            TextTarget::ZineTitle => o.title = text,
            _ => o.author = text,
        });
    }

    fn choose_zine_destination(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let folder = cx
                .background_executor()
                .spawn(async {
                    rfd::AsyncFileDialog::new()
                        .pick_folder()
                        .await
                        .map(|handle| handle.path().to_path_buf())
                })
                .await;
            if let Some(folder) = folder {
                this.update(cx, |this, cx| {
                    this.update_zine_options(cx, |o| o.destination = Some(folder));
                })
                .ok();
            }
        })
        .detach();
    }

    // --- Making it -------------------------------------------------------------

    /// The assets behind the zine's photos, in order (`None` for a photo no
    /// longer in the library).
    fn zine_assets(&self) -> Vec<Option<Asset>> {
        let by_key: HashMap<(&PathBuf, &Option<String>), &Asset> = self
            .catalog
            .assets
            .iter()
            .filter(|a| !a.missing)
            .map(|a| ((&a.path, &a.copy_name), a))
            .collect();
        self.zine
            .photos
            .iter()
            .map(|p| by_key.get(&(&p.path, &p.copy_name)).map(|a| (*a).clone()))
            .collect()
    }

    fn start_zine(&mut self, cx: &mut Context<Self>) {
        if self.zine_run.is_some() {
            return;
        }
        let options = self.zine.options.clone();
        let Some(destination) = options.destination.clone() else {
            self.zine_message = Some("Choose a folder to save the zine in first.".into());
            cx.notify();
            return;
        };
        let assets = self.zine_assets();
        if assets.iter().all(Option::is_none) {
            self.zine_message = Some("Add photos first: right-click them in the Library.".into());
            cx.notify();
            return;
        }

        // How big the pictures need to be for the biggest place they sit.
        let layout = self.zine.layout();
        let biggest_mm = layout
            .pages
            .iter()
            .flat_map(|p| &p.cells)
            .filter(|c| matches!(c.content, ZineContent::Photo(_) | ZineContent::Cover { .. }))
            .map(|c| c.rect.w.max(c.rect.h))
            .fold(0.0f32, f32::max);
        let edge = match options.format {
            ZineFormat::Pdf => ((biggest_mm / 25.4 * PRINT_DPI) as u32).clamp(MIN_EDGE, MAX_EDGE),
            ZineFormat::Html | ZineFormat::HtmlSingle | ZineFormat::Epub => WEB_EDGE,
            // Pictures of the pages: a photo is never larger than a page.
            ZineFormat::Png | ZineFormat::Jpeg => options.image_edge.clamp(MIN_EDGE, MAX_EDGE),
        };

        let total = assets.len();
        let done = Arc::new(AtomicUsize::new(0));
        self.zine_run = Some(ZineRun {
            done: done.clone(),
            total,
        });
        self.zine_message = None;
        cx.notify();

        let job = cx.background_executor().spawn({
            let options = options.clone();
            let done = done.clone();
            async move { make_zine(assets, &options, &layout, &destination, edge, &done) }
        });
        cx.spawn(async move |this, cx| {
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
            let result = job.await;
            drop(ticker);
            this.update(cx, |this, cx| {
                this.zine_run = None;
                match result {
                    Ok((path, skipped)) => {
                        crate::export::reveal(&path);
                        let mut message = format!("Saved {}", path.display());
                        if skipped > 0 {
                            message.push_str(&format!(
                                " ({skipped} photo{} could not be rendered)",
                                plural(skipped)
                            ));
                        }
                        this.zine_message = Some(message.into());
                    }
                    Err(error) => this.zine_message = Some(format!("Could not make the zine: {error}").into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // --- UI --------------------------------------------------------------------

    pub(crate) fn render_zines(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        // The edited thumbnails the pages show are made in the background.
        let wanted: Vec<usize> = {
            let by_key: HashMap<(&PathBuf, &Option<String>), usize> = self
                .catalog
                .assets
                .iter()
                .enumerate()
                .map(|(i, a)| ((&a.path, &a.copy_name), i))
                .collect();
            self.zine
                .photos
                .iter()
                .filter_map(|p| by_key.get(&(&p.path, &p.copy_name)).copied())
                .take(200)
                .collect()
        };
        self.ensure_export_thumbnails(&wanted, cx);

        let theme = self.theme;
        let options = self.zine.options.clone();
        let count = self.zine.photos.len();
        let layout = self.zine.layout();
        let running = self.zine_run.as_ref().map(ZineRun::progress);

        // Folded cards: what is rarely needed starts out of the way.
        if !self.zine_ui.panel_ready {
            self.zine_ui.panel_ready = true;
            for key in crate::zine_panel::FOLDED_AT_FIRST {
                self.collapsed_sections.insert(key);
            }
        }
        let sections = self.zine_panel_sections(cx);

        let ready = count > 0 && options.destination.is_some();
        let footer_button = div()
            .id("zine-make")
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .h(sp(32.0))
            .rounded_md()
            .text_size(sp(12.0))
            .font_weight(gpui::FontWeight::SEMIBOLD);
        let footer_button = if let Some((done, total)) = running {
            footer_button
                .bg(theme.surface_elevated)
                .text_color(theme.text_muted)
                .child(format!("Making your zine\u{2026} {done} of {total}"))
        } else if ready {
            let noun = options.format.noun();
            footer_button
                .bg(theme.accent)
                .text_color(gpui::white())
                .cursor_pointer()
                .shadow_md()
                .hover(|el| el.opacity(0.92))
                .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.start_zine(cx)))
                .child(format!("Make the {noun}"))
        } else {
            footer_button
                .bg(theme.surface_elevated)
                .text_color(theme.text_faint)
                .child(if count == 0 {
                    "Add photos to begin"
                } else {
                    "Choose a folder to save in"
                })
        };
        let mut footer_rows = Vec::new();
        if let Some(message) = &self.zine_message {
            footer_rows.push(vec![div()
                .flex_1()
                .px_2()
                .py_1p5()
                .rounded_md()
                .bg(theme.background)
                .border_1()
                .border_color(theme.border_subtle)
                .text_size(sp(10.5))
                .text_color(theme.text_muted)
                .child(message.clone())
                .into_any_element()]);
        }
        footer_rows.push(vec![footer_button.into_any_element()]);
        let footer = rustyedit_ui::panel_footer(&theme, footer_rows);
        let panel = rustyedit_ui::side_panel(
            "zine-side-panel",
            &theme,
            rustyedit_ui::PanelSide::Left,
            self.settings.layout.inspector_width as f32,
            &self.zine_panel_scroll,
            None,
            Some(footer),
            sections,
        )
        .into_any_element();

        // The finished zine, page by page.
        if self.zine_ui.preview.is_some() && count > 0 {
            return div()
                .flex()
                .flex_row()
                .flex_1()
                .min_h(sp(0.0))
                .child(panel)
                .child(self.render_zine_preview(window, cx))
                .into_any_element();
        }

        // Manual: pages in the middle, the photos in a bar on the right.
        if options.mode == ZineMode::Manual && count > 0 {
            return div()
                .flex()
                .flex_row()
                .flex_1()
                .min_h(sp(0.0))
                .child(panel)
                .child(self.render_zine_workbench(&layout, window, cx))
                .into_any_element();
        }

        // --- Centre: the pages, then the photos ---------------------------------
        let mut body: Vec<AnyElement> = Vec::new();
        body.push(self.zine_header(count, layout.pages.len(), layout.left_out, cx));
        if let Some(message) = &self.zine_message {
            body.push(
                div()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .text_size(sp(11.5))
                    .text_color(theme.text)
                    .child(message.clone())
                    .into_any_element(),
            );
        }
        if count == 0 {
            body.push(empty_state(&theme));
            body.extend(self.start_with_best(cx));
        } else {
            body.push(page_strip(&theme, &layout));
            body.push(self.zine_tiles(cx));
        }

        let center = div()
            .relative()
            .flex_1()
            .min_w(sp(0.0))
            .min_h(sp(0.0))
            .bg(theme.background)
            .child(
                div()
                    .id("zine-page")
                    .track_scroll(&self.zine_ui.auto_scroll)
                    .flex()
                    .flex_col()
                    .size_full()
                    .overflow_y_scroll()
                    .gap_4()
                    .px_6()
                    .py_4()
                    .children(body),
            )
            .child(rustyedit_ui::vertical_scrollbar(
                "zine-page-scrollbar",
                self.zine_ui.auto_scroll.clone(),
                &theme,
            ));

        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h(sp(0.0))
            .child(panel)
            .child(center)
            .into_any_element()
    }

    fn zine_header(&self, count: usize, pages: usize, left_out: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let mut summary = if count == 0 {
            "No photos yet".to_string()
        } else {
            format!("{count} photo{} \u{00B7} {pages} page{}", plural(count), plural(pages))
        };
        if left_out > 0 {
            summary.push_str(&format!(" \u{00B7} {left_out} won't fit on the sheet"));
        }
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .text_size(sp(18.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child(self.zine.options.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(sp(11.5))
                            .text_color(if left_out > 0 { theme.star } else { theme.text_faint })
                            .child(summary),
                    ),
            )
            .when(count > 0, |el| {
                el.child(
                    div()
                        .id("zine-preview-open")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .text_size(sp(11.0))
                        .text_color(theme.text_muted)
                        .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                        .child("Preview")
                        .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.open_zine_preview(cx))),
                )
                .child(
                    div()
                        .id("zine-clear")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .text_size(sp(11.0))
                        .text_color(theme.text_muted)
                        .hover(|el| el.bg(theme.surface_elevated).text_color(theme.danger))
                        .child("Clear all")
                        .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                            this.update_zine(cx, |z| z.clear())
                        })),
                )
            })
            .into_any_element()
    }

    /// The photos in order as tiles with move and remove buttons.
    fn zine_tiles(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let by_key: HashMap<(&PathBuf, &Option<String>), &Asset> = self
            .catalog
            .assets
            .iter()
            .map(|a| ((&a.path, &a.copy_name), a))
            .collect();
        let last = self.zine.photos.len().saturating_sub(1);
        let tiles = self.zine.photos.iter().enumerate().map(|(n, photo)| {
            let asset = by_key.get(&(&photo.path, &photo.copy_name)).copied();
            let name = photo
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let thumbnail = asset.and_then(|a| self.export_thumbnail(a));
            let button = |id: SharedString, glyph: &'static str, tip: &'static str, enabled: bool| {
                div()
                    .id(id)
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(sp(22.0))
                    .rounded_md()
                    .text_size(sp(12.0))
                    .when(enabled, |el| {
                        el.cursor_pointer()
                            .text_color(theme.text_muted)
                            .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                    })
                    .when(!enabled, |el| el.text_color(theme.text_faint.opacity(0.4)))
                    .tooltip(rustyedit_ui::tip(tip, &theme))
                    .child(glyph)
            };
            div()
                .id(SharedString::from(format!("zine-tile-{n}")))
                .flex()
                .flex_col()
                .gap_1()
                .w(sp(TILE_W))
                .p_1()
                .rounded_lg()
                .bg(theme.surface)
                .border_1()
                .border_color(theme.border_subtle)
                .child(
                    div()
                        .relative()
                        .w_full()
                        .h(sp(TILE_W * 0.72))
                        .rounded_md()
                        .overflow_hidden()
                        .bg(gpui::black())
                        .child(match thumbnail {
                            Some(path) => img(path)
                                .object_fit(ObjectFit::Contain)
                                .size_full()
                                .into_any_element(),
                            None => div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(sp(10.0))
                                .text_color(theme.text_faint)
                                .child(if asset.is_none() { "Not in the library" } else { "" })
                                .into_any_element(),
                        })
                        .child(
                            div()
                                .absolute()
                                .top(sp(4.0))
                                .left(sp(4.0))
                                .px_1p5()
                                .h(sp(16.0))
                                .flex()
                                .items_center()
                                .rounded_full()
                                .bg(theme.background.opacity(0.85))
                                .text_size(sp(9.5))
                                .text_color(theme.text)
                                .child(format!("{}", n + 1)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex_1()
                                .min_w(sp(0.0))
                                .truncate()
                                .text_size(sp(10.5))
                                .text_color(theme.text_muted)
                                .child(name),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .flex_shrink_0()
                                .child(
                                    button(SharedString::from(format!("zine-left-{n}")), "\u{25C0}", "Earlier", n > 0)
                                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                            this.update_zine(cx, |z| {
                                                z.shift(n, -1);
                                            })
                                        })),
                                )
                                .child(
                                    button(SharedString::from(format!("zine-right-{n}")), "\u{25B6}", "Later", n < last)
                                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                            this.update_zine(cx, |z| {
                                                z.shift(n, 1);
                                            })
                                        })),
                                )
                                .child(
                                    button(SharedString::from(format!("zine-remove-{n}")), "\u{2715}", "Take out of the zine", true)
                                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                            this.update_zine(cx, |z| {
                                                if let Some(photo) = z.photos.get(n).cloned() {
                                                    z.remove(&[photo]);
                                                }
                                            })
                                        })),
                                ),
                        ),
                )
                .into_any_element()
        });
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_3()
            .children(tiles)
            .into_any_element()
    }

    /// The thumbnail of a photo of the zine.
    pub(crate) fn zine_thumb(&self, photo: &ZinePhoto) -> Option<PathBuf> {
        self.catalog
            .assets
            .iter()
            .find(|a| a.path == photo.path && a.copy_name == photo.copy_name)
            .and_then(|a| self.export_thumbnail(a))
    }

    pub(crate) fn zine_destination(&self, options: &ZineOptions, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let (name, parent) = match options.destination.as_deref() {
            Some(path) => (
                path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string()),
                path.parent().map(|p| p.display().to_string()).unwrap_or_default(),
            ),
            None => ("Choose a folder".to_string(), "Where the zine is saved".to_string()),
        };
        let chosen = options.destination.is_some();
        div()
            .id("zine-destination")
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .p_2()
            .rounded_lg()
            .cursor_pointer()
            .border_1()
            .when(chosen, |el| el.border_color(theme.border_subtle).bg(theme.background))
            .when(!chosen, |el| el.border_color(theme.accent).bg(theme.accent_muted))
            .hover(|el| el.border_color(theme.accent))
            .child(rustyedit_ui::icon(Icon::Folder, 18.0, theme.accent))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(sp(0.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(sp(12.5))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child(name),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(sp(10.5))
                            .text_color(theme.text_faint)
                            .child(parent),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(sp(11.0))
                    .text_color(theme.accent)
                    .child(if chosen { "Change" } else { "Choose\u{2026}" }),
            )
            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.choose_zine_destination(cx)))
            .into_any_element()
    }

    /// A one-line text field: click to type, Enter to keep it.
    pub(crate) fn zine_text_field(
        &self,
        id: &'static str,
        target: TextTarget,
        value: &str,
        placeholder: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let field = div()
            .id(id)
            .flex()
            .flex_row()
            .items_center()
            .h(sp(28.0))
            .px_2()
            .rounded_md()
            .bg(theme.background)
            .border_1()
            .text_size(sp(12.0))
            .overflow_hidden();
        match self.draft_for(target) {
            Some(draft) => field
                .border_color(theme.accent)
                .text_color(theme.text)
                .child(draft.to_string())
                .child(div().w(sp(1.0)).h(sp(14.0)).ml(sp(1.0)).bg(theme.accent))
                .into_any_element(),
            None => {
                let current = value.to_string();
                field
                    .border_color(theme.border)
                    .text_color(if value.is_empty() { theme.text_faint } else { theme.text })
                    .cursor_text()
                    .hover(|el| el.border_color(theme.accent))
                    .tooltip(rustyedit_ui::tip("Click to edit, Enter to keep", &theme))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.text_draft = Some((target, current.clone()));
                        cx.notify();
                    }))
                    .child(if value.is_empty() { placeholder.to_string() } else { value.to_string() })
                    .into_any_element()
            }
        }
    }
}

/// Renders every photo, lays the zine out and writes it. Returns where it
/// went and how many photos could not be rendered.
fn make_zine(
    assets: Vec<Option<Asset>>,
    options: &ZineOptions,
    layout: &ZinePageLayout,
    destination: &std::path::Path,
    edge: u32,
    done: &AtomicUsize,
) -> anyhow::Result<(PathBuf, usize)> {
    // Only the photos the pages show are rendered.
    let used = rustyedit_io::zine::photos_used(layout);
    let images: Mutex<Vec<Option<ZineImage>>> = Mutex::new(vec![None; assets.len()]);
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(1, 3);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(entry) = assets.get(i) else { break };
                if let Some(asset) = entry.as_ref().filter(|_| used.contains(&i)) {
                    if let Ok(image) = render_photo(asset, options, edge) {
                        images.lock().unwrap()[i] = Some(image);
                    }
                }
                done.fetch_add(1, Ordering::Relaxed);
            });
        }
    });
    let images = images.into_inner().unwrap();
    let skipped = used.iter().filter(|&&n| images[n].is_none()).count();

    std::fs::create_dir_all(destination)?;
    let stem = rustyedit_io::zine::file_stem(&options.title);
    let path = match options.format {
        ZineFormat::Pdf => {
            let path = free_name(destination, &stem, "pdf");
            rustyedit_io::zine::write_pdf(&path, options, layout, &images)?;
            path
        }
        ZineFormat::Html => {
            let folder = free_name(destination, &stem, "");
            rustyedit_io::zine_web::write_html(&folder, options, layout, &images)?;
            folder
        }
        ZineFormat::HtmlSingle => {
            let path = free_name(destination, &stem, "html");
            rustyedit_io::zine_web::write_html_single(&path, options, layout, &images)?;
            path
        }
        ZineFormat::Epub => {
            let path = free_name(destination, &stem, "epub");
            rustyedit_io::zine_web::write_epub(&path, options, layout, &images)?;
            path
        }
        ZineFormat::Png | ZineFormat::Jpeg => {
            let folder = free_name(destination, &stem, "");
            let kind = if options.format == ZineFormat::Png {
                rustyedit_io::zine_raster::PageImage::Png
            } else {
                rustyedit_io::zine_raster::PageImage::Jpeg(options.quality)
            };
            rustyedit_io::zine_raster::write_page_images(&folder, options, layout, &images, kind)?;
            folder
        }
    };
    Ok((path, skipped))
}

/// A photo rendered through its develop settings at up to `edge` pixels,
/// as a JPEG for the zine.
fn render_photo(asset: &Asset, options: &ZineOptions, edge: u32) -> anyhow::Result<ZineImage> {
    let source = rustyedit_io::open_for_develop(&asset.path)?;
    let mut rgb = rustyedit_processing::render_export(
        &source,
        &asset.develop,
        Some(edge),
        None,
        &rustyedit_processing::OutputSpace::Srgb,
    );
    let frame = asset.develop.effective().frame;
    if frame.is_active() {
        rgb = rustyedit_processing::frame_rgb8(&rgb, &frame);
    }
    let caption = asset
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    ZineImage::encode(&rgb, options.quality, caption)
}

/// `stem.ext` (or just `stem` for a folder) in `dir`, numbered when taken
/// so nothing is overwritten.
fn free_name(dir: &std::path::Path, stem: &str, ext: &str) -> PathBuf {
    let name = |n: u32| {
        let base = if n == 0 { stem.to_string() } else { format!("{stem}-{n}") };
        if ext.is_empty() { base } else { format!("{base}.{ext}") }
    };
    (0..)
        .map(|n| dir.join(name(n)))
        .find(|path| !path.exists())
        .expect("an unused name")
}

pub(crate) fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

// --- Drawing helpers ------------------------------------------------------------

fn empty_state(theme: &Theme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_2()
        .py_12()
        .child(rustyedit_ui::icon(Icon::Sidecar, 36.0, theme.text_faint))
        .child(
            div()
                .text_size(sp(14.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text)
                .child("Your zine is empty"),
        )
        .child(
            div()
                .max_w(sp(380.0))
                .text_center()
                .text_size(sp(12.0))
                .text_color(theme.text_muted)
                .child(
                    "Right-click a photo, or a selection of them, in the Library and choose \
                     \u{201C}Add to zine\u{201D}. Come back here to lay them out and make a \
                     PDF or a web page.",
                ),
        )
        .into_any_element()
}

/// The first pages drawn small, so the layout can be judged at a glance.
fn page_strip(theme: &Theme, layout: &rustyedit_core::ZinePageLayout) -> AnyElement {
    const SHOWN: usize = 8;
    let height = 120.0;
    let (pw, ph) = layout.size;
    let scale = height / ph;
    let width = pw * scale;
    let theme = *theme;
    let pages = layout.pages.iter().take(SHOWN).enumerate().map(|(n, page)| {
        let cells = page.cells.iter().map(|cell| {
            let r = cell.rect;
            let (fill, label) = match cell.content {
                ZineContent::Photo(i) => (theme.accent_muted, format!("{}", i + 1)),
                ZineContent::Cover { .. } => (theme.accent.opacity(0.45), "Cover".to_string()),
                ZineContent::Back => (theme.surface_elevated, "Back".to_string()),
                ZineContent::Text => (theme.surface_elevated, "Text".to_string()),
            };
            div()
                .absolute()
                .left(relative(r.x / pw))
                .top(relative(r.y / ph))
                .w(relative(r.w / pw))
                .h(relative(r.h / ph))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .bg(fill)
                .border_1()
                .border_color(theme.accent.opacity(0.5))
                .text_size(sp(8.5))
                .text_color(theme.text)
                // The top row of a folded sheet prints upside down.
                .child(if cell.rotated { format!("{label} \u{21BB}") } else { label })
        });
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .flex_shrink_0()
            .child(
                div()
                    .relative()
                    .w(px(width))
                    .h(px(height))
                    .bg(gpui::white().opacity(0.92))
                    .border_1()
                    .border_color(theme.border)
                    .children(cells),
            )
            .child(
                div()
                    .text_size(sp(9.5))
                    .text_color(theme.text_faint)
                    .child(if layout.pages.len() == 1 && page.cells.len() > 1 {
                        "Print, cut and fold".to_string()
                    } else {
                        format!("Page {}", n + 1)
                    }),
            )
    });
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_size(sp(10.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text_faint)
                .child("PAGES"),
        )
        .child(
            div()
                .id("zine-pages")
                .flex()
                .flex_row()
                .gap_3()
                .overflow_x_scroll()
                .pb_1()
                .children(pages)
                .when(layout.pages.len() > SHOWN, |el| {
                    el.child(
                        div()
                            .flex()
                            .items_center()
                            .flex_shrink_0()
                            .text_size(sp(11.0))
                            .text_color(theme.text_faint)
                            .child(format!("+ {} more", layout.pages.len() - SHOWN)),
                    )
                }),
        )
        .into_any_element()
}
