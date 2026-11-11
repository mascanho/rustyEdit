//! The import dialog: after a folder is picked, shows what is in it by file
//! type, rating, colour label and protection, and imports only what is
//! ticked. Ratings and labels are those set in the camera or another
//! program.

use std::{collections::HashSet, path::PathBuf};

use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context, MouseButton, SharedString};
use rustyedit_core::{Candidate, ColorLabel, FileKind, ImportFilter};
use rustyedit_ui::{sp, Theme};

use crate::workspace::Workspace;

const DIALOG_WIDTH: f32 = 440.0;

/// The open dialog.
pub(crate) struct ImportDialog {
    pub(crate) folder: PathBuf,
    /// Read in the background; `None` until it's done.
    pub(crate) candidates: Option<Vec<Candidate>>,
    pub(crate) filter: ImportFilter,
    /// Photos the library already has: never left out by the filter.
    in_library: HashSet<PathBuf>,
}

impl ImportDialog {
    fn matching(&self) -> usize {
        self.candidates.as_ref().map_or(0, |all| {
            all.iter()
                .filter(|c| self.in_library.contains(&c.path) || self.filter.matches(c))
                .count()
        })
    }

    /// The photos the filter leaves out.
    fn skipped(&self) -> Vec<PathBuf> {
        self.candidates
            .iter()
            .flatten()
            .filter(|c| !self.in_library.contains(&c.path) && !self.filter.matches(c))
            .map(|c| c.path.clone())
            .collect()
    }
}

impl Workspace {
    /// Starts reading `folder` for the dialog.
    pub(crate) fn open_import_dialog(&mut self, folder: PathBuf, cx: &mut Context<Self>) {
        let in_library = self.catalog.assets.iter().map(|a| a.path.clone()).collect();
        self.import_dialog = Some(ImportDialog {
            folder: folder.clone(),
            candidates: None,
            filter: ImportFilter::default(),
            in_library,
        });
        cx.notify();
        let job = {
            let folder = folder.clone();
            cx.background_executor()
                .spawn(async move { rustyedit_io::read_import_candidates(&folder) })
        };
        cx.spawn(async move |this, cx| {
            let candidates = job.await;
            this.update(cx, |this, cx| {
                let still_open = this
                    .import_dialog
                    .as_ref()
                    .is_some_and(|d| d.folder == folder);
                if !still_open {
                    return;
                }
                if candidates.is_empty() {
                    // Nothing to choose from: the usual import says so.
                    this.import_dialog = None;
                    this.import_folder(folder, None, cx);
                    return;
                }
                if let Some(dialog) = this.import_dialog.as_mut() {
                    dialog.candidates = Some(candidates);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn close_import_dialog(&mut self, cx: &mut Context<Self>) {
        self.import_dialog = None;
        self.status_message = Some("Import cancelled.".into());
        cx.notify();
    }

    fn edit_import_filter(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut ImportFilter)) {
        if let Some(dialog) = self.import_dialog.as_mut() {
            edit(&mut dialog.filter);
            cx.notify();
        }
    }

    fn run_import(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.import_dialog.take() else {
            return;
        };
        let skipped = dialog.skipped();
        self.import_folder_without(dialog.folder, skipped, cx);
    }

    pub(crate) fn render_import_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let Some(dialog) = &self.import_dialog else {
            return div().into_any_element();
        };
        let name = dialog
            .folder
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| dialog.folder.display().to_string());

        let header = div()
            .flex()
            .flex_col()
            .gap_0p5()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(theme.border_subtle)
            .child(div().text_sm().font_weight(gpui::FontWeight::SEMIBOLD).child(format!("Import \u{201C}{name}\u{201D}")))
            .child(
                div()
                    .text_size(sp(rustyedit_ui::type_size::BODY))
                    .text_color(theme.text_muted)
                    .child(match &dialog.candidates {
                        None => "Looking through the folder\u{2026}".to_string(),
                        Some(all) => format!(
                            "{} of {} photo{} will be imported",
                            dialog.matching(),
                            all.len(),
                            if all.len() == 1 { "" } else { "s" }
                        ),
                    }),
            );

        let mut body = Vec::new();
        if let Some(all) = &dialog.candidates {
            let filter = &dialog.filter;
            let count = |f: &dyn Fn(&Candidate) -> bool| all.iter().filter(|c| f(c)).count();

            let kinds = FileKind::ALL.into_iter().filter_map(|kind| {
                let n = count(&|c| c.kind == kind);
                (n > 0).then(|| {
                    filter_chip(
                        format!("import-kind-{}", kind.label()),
                        format!("{} {n}", kind.label()),
                        !filter.hidden_kinds.contains(&kind),
                        &theme,
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.edit_import_filter(cx, |f| f.toggle_kind(kind))
                    }))
                    .into_any_element()
                })
            });
            body.push(group("File type", kinds.collect(), &theme));

            let ratings = (0..=5u8).filter_map(|stars| {
                let n = count(&|c| c.rating == stars);
                (n > 0).then(|| {
                    let label = if stars == 0 {
                        format!("Unrated {n}")
                    } else {
                        format!("{} {n}", "\u{2605}".repeat(stars as usize))
                    };
                    filter_chip(
                        format!("import-rating-{stars}"),
                        label,
                        !filter.hidden_ratings[stars as usize],
                        &theme,
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.edit_import_filter(cx, |f| f.toggle_rating(stars))
                    }))
                    .into_any_element()
                })
            });
            body.push(group("Rating", ratings.collect(), &theme));

            let labels = std::iter::once(ColorLabel::None)
                .chain(ColorLabel::ALL)
                .filter_map(|label| {
                    let n = count(&|c| c.label == label);
                    (n > 0).then(|| {
                        let text = if label == ColorLabel::None {
                            format!("No label {n}")
                        } else {
                            format!("{} {n}", label.label())
                        };
                        filter_chip(
                            format!("import-label-{}", label.label()),
                            text,
                            !filter.hidden_labels.contains(&label),
                            &theme,
                        )
                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            this.edit_import_filter(cx, |f| f.toggle_label(label))
                        }))
                        .into_any_element()
                    })
                });
            body.push(group("Colour label", labels.collect(), &theme));

            let protected = count(&|c| c.protected);
            if protected > 0 {
                body.push(group(
                    "Protected",
                    vec![filter_chip(
                        "import-protected".to_string(),
                        format!("Skip {protected} locked or read-only"),
                        filter.skip_protected,
                        &theme,
                    )
                    .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                        this.edit_import_filter(cx, |f| f.skip_protected = !f.skip_protected)
                    }))
                    .into_any_element()],
                    &theme,
                ));
            }
            if all.iter().all(|c| c.rating == 0 && c.label == ColorLabel::None) {
                body.push(
                    div()
                        .px_4()
                        .pb_2()
                        .text_size(sp(rustyedit_ui::type_size::CAPTION))
                        .text_color(theme.text_faint)
                        .child("No ratings or labels were found in these files or beside them.")
                        .into_any_element(),
                );
            }
        }

        let ready = dialog.candidates.is_some() && dialog.matching() > 0;
        let footer = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .gap_1()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(theme.border_subtle)
            .child(rustyedit_ui::chrome_button(
                "import-cancel",
                "Cancel",
                false,
                &theme,
                cx.listener(|this, _: &ClickEvent, _w, cx| this.close_import_dialog(cx)),
            ))
            .child(rustyedit_ui::chrome_button(
                "import-run",
                "Import",
                ready,
                &theme,
                cx.listener(|this, _: &ClickEvent, _w, cx| this.run_import(cx)),
            ));

        let card = div()
            .id("import-card")
            .flex()
            .flex_col()
            .w(px(DIALOG_WIDTH))
            .rounded_lg()
            .overflow_hidden()
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _w, cx| cx.stop_propagation())
            .child(header)
            .child(div().flex().flex_col().py_1().children(body))
            .child(footer);

        div()
            .id("import-backdrop")
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.55))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _w, cx| this.close_import_dialog(cx)),
            )
            .child(card)
            .into_any_element()
    }
}

fn group(title: &'static str, chips: Vec<AnyElement>, theme: &Theme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .px_4()
        .py_2()
        .child(
            div()
                .text_size(sp(rustyedit_ui::type_size::SMALL))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text_muted)
                .child(title),
        )
        .child(div().flex().flex_row().flex_wrap().gap_1().children(chips))
        .into_any_element()
}

/// A tick-style chip: filled when its kind of photo will be imported.
fn filter_chip(id: String, label: String, on: bool, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    let theme = *theme;
    div()
        .id(SharedString::from(id))
        .flex()
        .items_center()
        .h(sp(22.0))
        .px_2()
        .rounded_md()
        .border_1()
        .cursor_pointer()
        .text_size(sp(rustyedit_ui::type_size::SMALL))
        .when(on, |el| {
            el.bg(theme.accent_muted).border_color(theme.accent).text_color(theme.text)
        })
        .when(!on, |el| {
            el.border_color(theme.border_subtle)
                .text_color(theme.text_faint)
                .hover(|el| el.text_color(theme.text).border_color(theme.border))
        })
        .child(label)
}
