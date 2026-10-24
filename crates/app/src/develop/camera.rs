//! Camera matching in Develop (see `rustyedit_processing::camera`):
//!
//! - "Camera" (beside Auto in Basic): the selected RAW photos take the
//!   camera's own tone and colour, from the JPEG preview in the file.
//! - "Match the camera's corrections" (Lens panel): distortion and
//!   vignetting as the camera corrected them, for lenses the lens profiles
//!   don't know.
//! - The first time a RAW photo is opened in Develop it starts from the
//!   camera's look and lens corrections (Settings ▸ Develop ▸ RAW photos).
//!   The lens profile is used when the lens is known, the camera's own
//!   correction otherwise. One undo step, and only once per photo.

use std::path::PathBuf;

use gpui::Context;
use rustyedit_core::{lens_profile, DevelopSettings, LensCorrection};
use rustyedit_processing::{CameraMatch, DevelopSource};

use crate::{
    develop::HistoryNote,
    workspace::{Mode, Workspace},
};

/// One photo to match.
struct Job {
    ix: usize,
    id: u64,
    path: PathBuf,
    settings: DevelopSettings,
    decoded: Option<DevelopSource>,
    /// The lens profile's corrections, when the lens is known exactly.
    profile: Option<LensCorrection>,
}

/// `own` with the parts `what` covers taken from `matched` (and the lens
/// profile, when there is one).
fn merged(
    own: &DevelopSettings,
    matched: &DevelopSettings,
    what: CameraMatch,
    profile: Option<LensCorrection>,
) -> DevelopSettings {
    let mut out = *own;
    if what.look {
        out.exposure = matched.exposure;
        out.contrast = matched.contrast;
        out.highlights = matched.highlights;
        out.shadows = matched.shadows;
        out.whites = matched.whites;
        out.blacks = matched.blacks;
        out.temperature = matched.temperature;
        out.tint = matched.tint;
        out.vibrance = matched.vibrance;
        out.saturation = matched.saturation;
        out.curves = matched.curves;
        out.parametric = matched.parametric;
        out.mixer = matched.mixer;
        out.grading = matched.grading;
    }
    if what.lens {
        match profile {
            Some(profile) => {
                out.lens_distortion = profile.distortion;
                out.lens_vignetting = profile.vignetting;
                out.lens_ca = profile.chromatic_aberration;
            }
            None => {
                out.lens_distortion = matched.lens_distortion;
                out.lens_vignetting = matched.lens_vignetting;
            }
        }
    }
    out
}

impl Workspace {
    /// The exactly-known lens profile for photo `ix`, if any.
    fn exact_lens_profile(&self, ix: usize) -> Option<LensCorrection> {
        let asset = self.catalog.assets.get(ix)?;
        let (lens, focal) = (asset.meta.lens.trim(), asset.meta.focal_mm);
        if lens.is_empty() {
            return None;
        }
        let aspect = if asset.width > 0 && asset.height > 0 {
            asset.width.max(asset.height) as f32 / asset.width.min(asset.height) as f32
        } else {
            1.5
        };
        let folder = self.settings.develop.lens_db_folder.clone();
        rustyedit_io::find_lens_profile(folder.as_deref(), lens, focal, aspect)
            .or_else(|| lens_profile(lens, focal))
            .filter(|p| p.exact)
            .map(|p| p.correction)
    }

    /// Whether the selected photo is a RAW file (so camera matching applies).
    pub(crate) fn selected_is_raw(&self) -> bool {
        self.catalog
            .selected_asset()
            .is_some_and(|a| rustyedit_io::is_raw_file(&a.path))
    }

    /// Camera matching on the target photos (`automatic`: just the photo
    /// being opened, for the first time). Non-RAW photos and RAW files
    /// without a usable preview are left alone.
    pub(crate) fn match_camera(&mut self, what: CameraMatch, automatic: bool, cx: &mut Context<Self>) {
        if self.develop_ui.auto_busy || self.is_importing || (!what.look && !what.lens) {
            return;
        }
        let targets = if automatic {
            vec![self.catalog.selected]
        } else {
            self.target_indices()
        };
        let jobs: Vec<Job> = targets
            .into_iter()
            .filter_map(|ix| {
                let asset = self.catalog.assets.get(ix).filter(|a| !a.missing)?;
                if !rustyedit_io::is_raw_file(&asset.path) {
                    return None;
                }
                let decoded = self
                    .source_image
                    .clone()
                    .filter(|_| self.source_image_for == Some(asset.id));
                Some(Job {
                    ix,
                    id: asset.id,
                    path: asset.path.clone(),
                    settings: asset.develop,
                    decoded,
                    // Opening a photo prefers a known lens profile; the
                    // button asks for the camera's own correction.
                    profile: (what.lens && automatic)
                        .then(|| self.exact_lens_profile(ix))
                        .flatten(),
                })
            })
            .collect();
        if jobs.is_empty() {
            if !automatic {
                self.status_message = Some("Camera matching works on RAW photos.".into());
                cx.notify();
            }
            return;
        }
        let count = jobs.len();
        self.develop_ui.auto_busy = true;
        if !automatic {
            self.status_message = Some("Matching the camera\u{2026}".into());
            cx.notify();
        }

        let work = cx.background_executor().spawn(async move {
            jobs.into_iter()
                .map(|job| {
                    let matched = (|| {
                        let preview = rustyedit_io::embedded_preview(&job.path)?;
                        let source = match job.decoded {
                            Some(source) => source,
                            None => rustyedit_io::open_for_develop(&job.path).ok()?,
                        };
                        // A known lens profile beats fitting the preview.
                        let fit = CameraMatch {
                            look: what.look,
                            lens: what.lens && job.profile.is_none(),
                        };
                        let matched = if fit.look || fit.lens {
                            rustyedit_processing::match_camera(&source, &job.settings, &preview, fit)
                        } else {
                            job.settings
                        };
                        Some(merged(&job.settings, &matched, what, job.profile))
                    })();
                    (job.ix, job.id, job.path, job.settings, matched)
                })
                .collect::<Vec<_>>()
        });

        cx.spawn(async move |this, cx| {
            let results = work.await;
            this.update(cx, |this, cx| {
                this.develop_ui.auto_busy = false;
                let mut updates = Vec::new();
                for (ix, id, path, started, matched) in results {
                    let Some(asset) = this
                        .catalog
                        .assets
                        .get_mut(ix)
                        .filter(|a| a.id == id && a.path == path)
                    else {
                        continue;
                    };
                    // Done for this photo, whatever came of it.
                    asset.camera_matched = true;
                    // Opening a photo never overrides edits made meanwhile.
                    if automatic && asset.develop != started {
                        continue;
                    }
                    if let Some(matched) = matched {
                        updates.push((ix, matched));
                    }
                }
                let done = updates.len();
                let title = match (what.look, what.lens) {
                    (true, true) => "Camera matching",
                    (true, false) => "Camera look",
                    _ => "Camera lens corrections",
                };
                this.replace_develop_each_and(
                    updates,
                    &title.to_lowercase(),
                    HistoryNote::Record {
                        title,
                        detail: "From the camera",
                    },
                    false,
                    cx,
                    |_, _| {},
                );
                this.mark_catalog_dirty(cx);
                this.status_message = Some(match (done, automatic) {
                    (0, true) => return,
                    (0, false) if count == 1 => {
                        "This RAW file has no usable camera preview to match.".into()
                    }
                    (0, false) => "None of these RAW files has a usable camera preview.".into(),
                    (1, true) => "Started from the camera's look.".into(),
                    (1, false) => format!("{title} applied.").into(),
                    (n, _) => format!("{title} applied to {n} photos.").into(),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Called once a photo has been decoded for Develop: a RAW photo opened
    /// for the first time, still unedited, starts from the camera.
    pub(crate) fn match_camera_on_open(&mut self, cx: &mut Context<Self>) {
        if self.mode != Mode::Develop {
            return;
        }
        let develop = &self.settings.develop;
        let what = CameraMatch {
            look: develop.camera_look_on_open,
            lens: develop.camera_lens_on_open,
        };
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        if asset.camera_matched
            || asset.develop.is_edited()
            || !rustyedit_io::is_raw_file(&asset.path)
        {
            return;
        }
        self.match_camera(what, true, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_lens_profile_beats_the_fitted_correction() {
        let own = DevelopSettings {
            clarity: 15.0,
            ..Default::default()
        };
        let matched = DevelopSettings {
            exposure: 20.0,
            lens_distortion: 12.0,
            ..Default::default()
        };
        let profile = LensCorrection {
            distortion: 30.0,
            vignetting: 25.0,
            chromatic_aberration: 5.0,
        };
        let both = CameraMatch {
            look: true,
            lens: true,
        };
        let out = merged(&own, &matched, both, Some(profile));
        assert_eq!(out.exposure, 20.0);
        assert_eq!(out.lens_distortion, 30.0);
        assert_eq!(out.lens_ca, 5.0);
        // Untouched parts stay the photo's own.
        assert_eq!(out.clarity, 15.0);

        let look_only = merged(&own, &matched, CameraMatch { look: true, lens: false }, None);
        assert_eq!(look_only.lens_distortion, 0.0);
    }
}
