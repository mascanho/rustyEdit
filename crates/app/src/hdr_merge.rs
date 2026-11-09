//! Merges the selected photos into one: a bracketed set of selected photos — different exposures of the
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

use crate::{
    actions::{MergeFocusStack, MergePanorama, MergeToHdr},
    workspace::Workspace,
};

/// What the selected photos are being merged into.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MergeKind {
    /// An exposure bracket, fused into one well-exposed photo.
    Hdr,
    /// Shots focused at different distances, sharp throughout.
    FocusStack,
    /// Overlapping shots in order, stitched side by side.
    Panorama,
}

impl MergeKind {
    fn doing(self) -> &'static str {
        match self {
            MergeKind::Hdr => "Merging {} photos to HDR",
            MergeKind::FocusStack => "Stacking focus of {} photos",
            MergeKind::Panorama => "Stitching {} photos into a panorama",
        }
    }

    fn done(self) -> &'static str {
        match self {
            MergeKind::Hdr => "Merged to HDR.",
            MergeKind::FocusStack => "Focus stacked.",
            MergeKind::Panorama => "Panorama stitched.",
        }
    }

    fn needs(self) -> &'static str {
        match self {
            MergeKind::Hdr => "Select at least two photos to merge to HDR.",
            MergeKind::FocusStack => "Select at least two photos to focus stack.",
            MergeKind::Panorama => "Select at least two photos to stitch a panorama.",
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            MergeKind::Hdr => "HDR",
            MergeKind::FocusStack => "Stack",
            MergeKind::Panorama => "Pano",
        }
    }
}

impl Workspace {
    pub fn merge_to_hdr(&mut self, _: &MergeToHdr, _window: &mut Window, cx: &mut Context<Self>) {
        self.merge_selection(MergeKind::Hdr, cx);
    }

    pub fn merge_focus_stack(
        &mut self,
        _: &MergeFocusStack,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.merge_selection(MergeKind::FocusStack, cx);
    }

    pub fn merge_panorama(
        &mut self,
        _: &MergePanorama,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.merge_selection(MergeKind::Panorama, cx);
    }

    /// Merges every selected photo (in catalogue order) into one. Needs at
    /// least two real (not missing, not a virtual copy) photos.
    pub(crate) fn merge_selection(&mut self, kind: MergeKind, cx: &mut Context<Self>) {
        if self.is_importing || self.hdr_merge_running {
            return;
        }
        let mut indices = self.target_indices();
        indices.sort_unstable();
        let targets: Vec<Asset> = indices
            .into_iter()
            .filter_map(|ix| self.catalog.assets.get(ix))
            .filter(|a| !a.missing && !a.is_virtual_copy())
            .cloned()
            .collect();
        if targets.len() < 2 {
            self.status_message = Some(kind.needs().into());
            cx.notify();
            return;
        }
        let Some(folder) = targets[0].path.parent().map(PathBuf::from) else {
            return;
        };

        self.hdr_merge_running = true;
        self.status_message =
            Some(format!("{}\u{2026}", kind.doing().replace("{}", &targets.len().to_string())).into());
        cx.notify();

        cx.spawn(async move |this, cx| {
            let folder_for_render = folder.clone();
            let result = cx
                .background_executor()
                .spawn(async move { render_merge(kind, &targets, &folder_for_render) })
                .await;
            this.update(cx, |this, cx| {
                this.hdr_merge_running = false;
                match result {
                    Ok(path) => {
                        this.status_message = Some(kind.done().into());
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

/// Renders every photo through its own Develop settings, merges them, and
/// writes the result as a TIFF next to the first photo. Returns the new
/// file's path.
fn render_merge(kind: MergeKind, targets: &[Asset], folder: &std::path::Path) -> Result<PathBuf, String> {
    let mut images = Vec::with_capacity(targets.len());
    for asset in targets {
        let source = rustyedit_io::open_for_develop(&asset.path)
            .map_err(|e| format!("Couldn't read {}: {e}", asset.display_name()))?;
        images.push(rustyedit_processing::apply(&source, &asset.develop));
    }
    let same_size = images.windows(2).all(|w| w[0].dimensions() == w[1].dimensions());
    let merged = match kind {
        MergeKind::Hdr => rustyedit_processing::merge_exposures(&images)
            .ok_or_else(|| "Photos must be exactly the same size to merge to HDR.".to_string())?,
        MergeKind::FocusStack => {
            if !same_size {
                return Err("Photos must be exactly the same size to focus stack.".into());
            }
            rustyedit_processing::stack_focus(&images)
                .ok_or_else(|| "The photos don't line up closely enough to stack.".to_string())?
        }
        MergeKind::Panorama => rustyedit_processing::stitch_panorama(&images)?,
    };

    let stem = targets[0].path.file_stem().and_then(|s| s.to_str()).unwrap_or("merge");
    let path = unique_path(folder, &format!("{stem}-{}", kind.suffix()), "tif");
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
