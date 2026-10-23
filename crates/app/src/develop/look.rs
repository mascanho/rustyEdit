//! Looks taken from other photos (see `rustyedit_processing::look`):
//!
//! - Match look: pick any image and the selected photos are fitted to its
//!   look (tone, white balance, colour, grading, vignette), as one undo
//!   step. The result can be saved as a preset like any other look.
//! - New preset from photo: pick any image and its look is saved as a
//!   preset, ready to apply to anything.
//!
//! Both measure renders, so they run on a background thread.

use std::path::{Path, PathBuf};

use gpui::{Context, Window};
use rustyedit_core::{DevelopSection, DevelopSettings};
use rustyedit_processing::{DevelopSource, LookStats};

use crate::{
    actions::{MatchLook, PresetFromPhoto},
    develop::HistoryNote,
    workspace::{Mode, Workspace},
};

/// Image files a look can be taken from.
const IMAGE_EXTENSIONS: [&str; 22] = [
    "jpg", "jpeg", "png", "tif", "tiff", "webp", "heic", "heif", "avif", "bmp", "gif", "dng",
    "cr2", "cr3", "nef", "arw", "raf", "orf", "rw2", "pef", "srw", "jxl",
];

/// Asks for an image to take a look from.
fn pick_reference() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Choose a photo whose look you want")
        .add_filter("Images", &IMAGE_EXTENSIONS)
        .pick_file()
}

/// The look of the image at `path`.
fn read_look(path: &Path) -> Option<LookStats> {
    let image = rustyedit_io::open_image(path).ok()?;
    Some(LookStats::of_image(&image))
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "photo".into())
}

impl Workspace {
    pub fn match_look_action(&mut self, _: &MatchLook, _w: &mut Window, cx: &mut Context<Self>) {
        self.match_look(cx);
    }

    pub fn preset_from_photo_action(
        &mut self,
        _: &PresetFromPhoto,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preset_from_photo(cx);
    }

    /// Match look: picks a reference image, then fits every target photo
    /// to it. The photo open in Develop uses its decoded copy; others are
    /// decoded in the background.
    pub(crate) fn match_look(&mut self, cx: &mut Context<Self>) {
        if self.develop_ui.auto_busy || self.is_importing || self.mode == Mode::Export {
            return;
        }
        let jobs: Vec<(usize, u64, PathBuf, DevelopSettings, Option<DevelopSource>)> = self
            .target_indices()
            .into_iter()
            .filter_map(|ix| {
                let asset = self.catalog.assets.get(ix).filter(|a| !a.missing)?;
                let decoded = self
                    .source_image
                    .clone()
                    .filter(|_| self.source_image_for == Some(asset.id));
                Some((ix, asset.id, asset.path.clone(), asset.develop, decoded))
            })
            .collect();
        if jobs.is_empty() {
            return;
        }
        let count = jobs.len();

        cx.spawn(async move |this, cx| {
            let Some(reference) = cx.background_executor().spawn(async { pick_reference() }).await
            else {
                return;
            };
            let name = file_stem(&reference);
            this.update(cx, |this, cx| {
                this.develop_ui.auto_busy = true;
                this.status_message = Some(format!("Matching the look of {name}\u{2026}").into());
                cx.notify();
            })
            .ok();

            let work = cx.background_executor().spawn(async move {
                let look = read_look(&reference)?;
                let results: Vec<_> = jobs
                    .into_iter()
                    .filter_map(|(ix, id, path, settings, decoded)| {
                        let source = match decoded {
                            Some(source) => source,
                            None => rustyedit_io::open_for_develop(&path).ok()?,
                        };
                        let matched = rustyedit_processing::match_look(&source, &settings, &look);
                        Some((ix, id, path, matched))
                    })
                    .collect();
                Some(results)
            });
            let results = work.await;

            this.update(cx, |this, cx| {
                this.develop_ui.auto_busy = false;
                let Some(results) = results else {
                    this.status_message = Some(format!("Couldn't read {name}.").into());
                    cx.notify();
                    return;
                };
                // Only photos that are still where they were. The look
                // sections come from the match; everything else (edits
                // made meanwhile to crop, spots, masks) stays.
                let updates: Vec<(usize, DevelopSettings)> = results
                    .into_iter()
                    .filter_map(|(ix, id, path, matched)| {
                        let asset = this
                            .catalog
                            .assets
                            .get(ix)
                            .filter(|a| a.id == id && a.path == path)?;
                        let mut develop = asset.develop;
                        for section in DevelopSection::LOOK {
                            section.copy(&matched, &mut develop);
                            develop.set_section_enabled(section, matched.is_section_enabled(section));
                        }
                        Some((ix, develop))
                    })
                    .collect();
                let done = updates.len();
                let detail = format!("From {name}");
                this.replace_develop_each_and(
                    updates,
                    "match look",
                    HistoryNote::Record {
                        title: "Match look",
                        detail: &detail,
                    },
                    false,
                    cx,
                    // The look is no longer a preset's.
                    |asset, _| asset.applied_preset = None,
                );
                this.status_message = Some(match done {
                    0 => "Couldn't read the photo to match.".into(),
                    1 if count == 1 => format!(
                        "Matched the look of {name}. Save it as a preset to reuse it."
                    )
                    .into(),
                    n => format!("Matched {n} photos to the look of {name}.").into(),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// New preset from photo: picks an image and saves its look as a preset
    /// named after it.
    pub(crate) fn preset_from_photo(&mut self, cx: &mut Context<Self>) {
        if self.develop_ui.auto_busy {
            return;
        }
        cx.spawn(async move |this, cx| {
            let Some(reference) = cx.background_executor().spawn(async { pick_reference() }).await
            else {
                return;
            };
            let name = file_stem(&reference);
            this.update(cx, |this, cx| {
                this.develop_ui.auto_busy = true;
                this.status_message = Some(format!("Reading the look of {name}\u{2026}").into());
                cx.notify();
            })
            .ok();

            let work = cx.background_executor().spawn(async move {
                let look = read_look(&reference)?;
                Some(rustyedit_processing::preset_from_look(&look))
            });
            let settings = work.await;

            this.update(cx, |this, cx| {
                this.develop_ui.auto_busy = false;
                match settings {
                    Some(settings) => {
                        let saved = this.develop_ui.presets.save_in(
                            &format!("Look of {name}"),
                            "Custom",
                            &settings,
                            &DevelopSection::LOOK,
                        );
                        this.persist_presets(format!(
                            "Added preset \u{201C}{saved}\u{201D} (in Custom). Click it to apply."
                        ));
                    }
                    None => this.status_message = Some(format!("Couldn't read {name}.").into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
