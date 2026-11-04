//! Catalog-level commands: backup/restore, compaction and clearing the
//! on-disk thumbnail/preview cache. The heavy filesystem work lives in
//! `rustyedit_io`; this module only wires it to the UI and reports results.

use gpui::Context;

use crate::{
    actions::{BackupCatalog, ClearCache, OptimizeCatalog, RestoreAutoBackup, RestoreCatalog},
    workspace::Workspace,
};

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn backup_file_name(name: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("{stem}.rustyedit-backup.json")
}

impl Workspace {
    pub fn backup_catalog_action(
        &mut self,
        _: &BackupCatalog,
        _w: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let def = self.catalog_def.clone();
        let default_name = backup_file_name(&def.name);
        let picked = cx.background_executor().spawn(async move {
            rfd::AsyncFileDialog::new()
                .set_title("Back Up Catalog")
                .set_file_name(default_name)
                .add_filter("RustyEdit backup", &["json"])
                .save_file()
                .await
                .map(|handle| handle.path().to_path_buf())
        });
        cx.spawn(async move |this, cx| {
            let Some(path) = picked.await else {
                return;
            };
            let display_path = path.clone();
            let job = cx.background_executor().spawn(async move {
                let backup = rustyedit_io::build_backup(&def)?;
                rustyedit_io::write_backup(&backup, &path)?;
                anyhow::Ok(())
            });
            let result = job.await;
            this.update(cx, |this, cx| {
                this.status_message = Some(match result {
                    Ok(()) => format!("Backed up catalog to {}", display_path.display()).into(),
                    Err(err) => format!("Couldn't back up catalog: {err}").into(),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn restore_catalog_action(
        &mut self,
        _: &RestoreCatalog,
        _w: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let picked = cx.background_executor().spawn(async {
            rfd::AsyncFileDialog::new()
                .set_title("Restore Catalog from Backup")
                .add_filter("RustyEdit backup", &["json"])
                .pick_file()
                .await
                .map(|handle| handle.path().to_path_buf())
        });
        cx.spawn(async move |this, cx| {
            let Some(path) = picked.await else {
                return;
            };
            let confirmed = cx
                .background_executor()
                .spawn(async move {
                    rfd::MessageDialog::new()
                        .set_title("Restore Catalog?")
                        .set_description(
                            "The current catalog definition and saved photo edits will be \
                             replaced by the backup. Your original photos are not touched.",
                        )
                        .set_buttons(rfd::MessageButtons::YesNo)
                        .show()
                        == rfd::MessageDialogResult::Yes
                })
                .await;
            if !confirmed {
                return;
            }
            let job = cx.background_executor().spawn(async move {
                let backup = rustyedit_io::read_backup(&path)?;
                rustyedit_io::restore_backup(&backup)?;
                anyhow::Ok(backup.def)
            });
            let result = job.await;
            this.update(cx, |this, cx| match result {
                Ok(def) => {
                    this.catalog_def = def;
                    this.save_catalog_def();
                    this.post_scan_message = Some("Restored catalog from backup.".into());
                    this.open_catalog(None, cx);
                }
                Err(err) => {
                    this.status_message = Some(format!("Couldn't restore catalog: {err}").into());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Restores the catalog's most recent automatic backup (see
    /// `workspace.rs`'s periodic calls to `write_auto_backup`), for going
    /// back further than Undo reaches, or after a crash. Older automatic
    /// backups are plain timestamped files under the config folder's
    /// `backups/<catalog id>` if an earlier one is ever needed instead.
    pub fn restore_auto_backup_action(
        &mut self,
        _: &RestoreAutoBackup,
        _w: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.catalog_def.id.clone();
        let Some(latest) = rustyedit_io::latest_auto_backup(&id) else {
            self.status_message = Some("No automatic backup is available yet.".into());
            cx.notify();
            return;
        };
        let when = rustyedit_io::relative_time(latest.created_at);
        cx.spawn(async move |this, cx| {
            let confirmed = cx
                .background_executor()
                .spawn({
                    let when = when.clone();
                    async move {
                        rfd::MessageDialog::new()
                            .set_title("Restore Automatic Backup?")
                            .set_description(format!(
                                "The current catalog definition and saved photo edits will be \
                                 replaced by the automatic backup from {when}. Your original \
                                 photos are not touched."
                            ))
                            .set_buttons(rfd::MessageButtons::YesNo)
                            .show()
                            == rfd::MessageDialogResult::Yes
                    }
                })
                .await;
            if !confirmed {
                return;
            }
            let path = latest.path.clone();
            let job = cx.background_executor().spawn(async move {
                let backup = rustyedit_io::read_auto_backup(&path)?;
                rustyedit_io::restore_backup(&backup)?;
                anyhow::Ok(backup.def)
            });
            let result = job.await;
            this.update(cx, |this, cx| match result {
                Ok(def) => {
                    this.catalog_def = def;
                    this.save_catalog_def();
                    this.post_scan_message =
                        Some(format!("Restored the automatic backup from {when}.").into());
                    this.open_catalog(None, cx);
                }
                Err(err) => {
                    this.status_message =
                        Some(format!("Couldn't restore automatic backup: {err}").into());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn optimize_catalog_action(
        &mut self,
        _: &OptimizeCatalog,
        _w: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let catalog = self.catalog.clone();
        let job = cx
            .background_executor()
            .spawn(async move { rustyedit_io::optimize_catalog(&catalog) });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |this, cx| {
                this.status_message = Some(match result {
                    Ok(report) => format!(
                        "Optimized catalog: saved {} ({} \u{2192} {}).",
                        human_bytes(report.saved()),
                        human_bytes(report.before),
                        human_bytes(report.after)
                    )
                    .into(),
                    Err(err) => format!("Couldn't optimize catalog: {err}").into(),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn clear_cache_action(
        &mut self,
        _: &ClearCache,
        _w: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_importing {
            self.status_message =
                Some("Wait for the import to finish before clearing the cache.".into());
            cx.notify();
            return;
        }
        let freed = rustyedit_io::clear_cache();
        if let Some(cache) = self.thumbnail_cache.clone() {
            for asset in &self.catalog.assets {
                cache.forget(&asset.path);
            }
        }
        self.histogram_cache.clear();
        self.develop_histogram = None;
        self.loupe_paths.clear();
        self.loupe_loading.clear();
        self.loupe_full_paths.clear();
        self.loupe_full_loading.clear();
        self.histogram_loading = None;
        self.status_message = Some(
            format!(
                "Cleared thumbnail and preview cache ({} freed); rebuilding thumbnails\u{2026}",
                human_bytes(freed)
            )
            .into(),
        );
        let indices: Vec<usize> = self
            .catalog
            .assets
            .iter()
            .enumerate()
            .filter(|(_, asset)| !asset.missing)
            .map(|(index, _)| index)
            .collect();
        self.refresh_asset_thumbnails(&indices, cx);
        cx.notify();
    }
}
