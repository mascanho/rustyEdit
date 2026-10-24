//! Reading and writing XMP sidecars by hand, for the selected photos: the
//! automatic pass (see `rustyedit_io::xmp`) covers import and saving.

use gpui::Context;
use rustyedit_io::xmp::{self, WriteOutcome};

use crate::workspace::Workspace;

impl Workspace {
    /// The photos the sidecar commands act on: real files, not virtual
    /// copies (which share their original's sidecar).
    fn sidecar_targets(&self) -> Vec<usize> {
        self.target_indices()
            .into_iter()
            .filter(|&ix| {
                self.catalog
                    .assets
                    .get(ix)
                    .is_some_and(|a| !a.missing && a.copy_name.is_none())
            })
            .collect()
    }

    /// Whether any target photo has a sidecar file to read.
    pub(crate) fn targets_have_sidecars(&self) -> bool {
        self.sidecar_targets().into_iter().any(|ix| {
            xmp::sidecar_path(&self.catalog.assets[ix].path).is_file()
        })
    }

    /// Gives the target photos what their sidecars say: rating, flag,
    /// label, keywords and text, and the edits (this app's own, or the
    /// Camera Raw basics another app wrote). One undo step each for the
    /// edits and for the rest.
    pub(crate) fn read_from_sidecars(&mut self, cx: &mut Context<Self>) {
        let mut changes = Vec::new();
        let mut edits = Vec::new();
        let mut read = 0;
        for ix in self.sidecar_targets() {
            let Some(data) = xmp::read_sidecar(&self.catalog.assets[ix].path) else {
                continue;
            };
            read += 1;
            let asset = &mut self.catalog.assets[ix];
            let before = asset.state();
            // Edits go through the Develop path so previews and thumbnails
            // refresh; everything else is set here.
            data.apply_to(asset, false);
            let after = asset.state();
            if before != after {
                changes.push((ix, before, after));
            }
            if let Some(develop) = data.develop.or(data.camera_raw) {
                edits.push((ix, develop));
            }
        }
        if read == 0 {
            self.status_message = Some("No XMP sidecar to read.".into());
            cx.notify();
            return;
        }
        let touched = changes.len();
        self.push_undo("Read XMP sidecar", changes, false);
        let edited = edits.len();
        self.replace_develop_each(
            edits,
            "Read XMP sidecar edits",
            crate::develop::HistoryNote::Record {
                title: "Read XMP sidecar",
                detail: "Edits from the sidecar",
            },
            cx,
        );
        self.facets = crate::filters_panel::Facets::build(&self.catalog);
        self.mark_catalog_dirty(cx);
        self.status_message = Some(
            format!(
                "Read {read} sidecar{} ({touched} changed, {edited} with edits).",
                if read == 1 { "" } else { "s" }
            )
            .into(),
        );
        cx.notify();
    }

    /// Writes a sidecar for each target photo now, whatever the automatic
    /// setting says. Sidecars other apps made are left alone.
    pub(crate) fn write_sidecars_now(&mut self, cx: &mut Context<Self>) {
        let include_edits = self.settings.sidecars.include_edits;
        let (mut written, mut foreign, mut failed) = (0, 0, 0);
        for ix in self.sidecar_targets() {
            let asset = &self.catalog.assets[ix];
            let text = xmp::to_xmp(asset, include_edits);
            match xmp::write_sidecar(&asset.path, &text) {
                Ok(WriteOutcome::Written) => written += 1,
                Ok(WriteOutcome::Unchanged) => {}
                Ok(WriteOutcome::Foreign) => foreign += 1,
                Err(_) => failed += 1,
            }
        }
        let mut message = format!("Wrote {written} XMP sidecar{}.", if written == 1 { "" } else { "s" });
        if foreign > 0 {
            message.push_str(&format!(" Left {foreign} made by other apps untouched."));
        }
        if failed > 0 {
            message = format!("Couldn't write {failed} XMP sidecar(s). {message}");
        }
        self.status_message = Some(message.into());
        cx.notify();
    }
}
