//! Merges a bracketed set of selected photos — different exposures of the
//! same, already-aligned scene, e.g. a tripod sequence — into one
//! well-exposed photo, by exposure fusion
//! ([`rustyedit_processing::merge_exposures`]). The brackets must share
//! the same pixel dimensions; there's no attempt here to align or warp
//! mismatched framing, so a handheld sequence with drift between frames
//! won't line up.
//!
//! Each source photo is rendered through its own current Develop
//! settings first, so a merge reflects whatever's already been done to
//! each (white balance, exposure nudges, …), not just the raw file. The
//! merged result is written as a new TIFF next to the first photo and
//! brought into the catalog like any newly imported file.

use std::path::PathBuf;

use gpui::{Context, Window};
use rustyedit_core::Asset;

use crate::{actions::MergeToHdr, workspace::Workspace};

impl Workspace {
    pub fn merge_to_hdr(&mut self, _: &MergeToHdr, _window: &mut Window, cx: &mut Context<Self>) {
        self.merge_selection_to_hdr(cx);
    }

    /// Merges every selected photo as one exposure bracket. Needs at
    /// least two real (not missing, not a virtual copy) photos.
    pub(crate) fn merge_selection_to_hdr(&mut self, cx: &mut Context<Self>) {
        if self.is_importing || self.hdr_merge_running {
            return;
        }
        let targets: Vec<Asset> = self
            .target_indices()
            .into_iter()
            .filter_map(|ix| self.catalog.assets.get(ix))
            .filter(|a| !a.missing && !a.is_virtual_copy())
            .cloned()
            .collect();
        if targets.len() < 2 {
            self.status_message = Some("Select at least two photos to merge to HDR.".into());
            cx.notify();
            return;
        }
        let Some(folder) = targets[0].path.parent().map(PathBuf::from) else {
            return;
        };

        self.hdr_merge_running = true;
        self.status_message = Some(format!("Merging {} photos to HDR\u{2026}", targets.len()).into());
        cx.notify();

        cx.spawn(async move |this, cx| {
            let folder_for_render = folder.clone();
            let result = cx
                .background_executor()
                .spawn(async move { render_hdr_merge(&targets, &folder_for_render) })
                .await;
            this.update(cx, |this, cx| {
                this.hdr_merge_running = false;
                match result {
                    Ok(path) => {
                        this.status_message = Some("Merged to HDR.".into());
                        this.import_folder(folder, Some(path), cx);
                    }
                    Err(error) => {
                        this.status_message = Some(error.into());
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }
}

/// Renders every photo through its own Develop settings, fuses them, and
/// writes the result as a TIFF next to the first photo. Returns the new
/// file's path.
fn render_hdr_merge(targets: &[Asset], folder: &std::path::Path) -> Result<PathBuf, String> {
    let mut images = Vec::with_capacity(targets.len());
    for asset in targets {
        let source = rustyedit_io::open_for_develop(&asset.path)
            .map_err(|e| format!("Couldn't read {}: {e}", asset.display_name()))?;
        images.push(rustyedit_processing::apply(&source, &asset.develop));
    }
    let merged = rustyedit_processing::merge_exposures(&images)
        .ok_or_else(|| "Photos must be exactly the same size to merge to HDR.".to_string())?;

    let stem = targets[0].path.file_stem().and_then(|s| s.to_str()).unwrap_or("merge");
    let path = unique_path(folder, &format!("{stem}-HDR"), "tif");
    merged.save(&path).map_err(|e| format!("Couldn't save the merged photo: {e}"))?;
    Ok(path)
}

/// `folder/base.ext`, or `folder/base-2.ext`, `folder/base-3.ext`, … if
/// that's already taken.
fn unique_path(folder: &std::path::Path, base: &str, ext: &str) -> PathBuf {
    let mut candidate = folder.join(format!("{base}.{ext}"));
    let mut n = 2;
    while candidate.exists() {
        candidate = folder.join(format!("{base}-{n}.{ext}"));
        n += 1;
    }
    candidate
}
