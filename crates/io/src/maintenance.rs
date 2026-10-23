//! Catalog-level housekeeping that is too heavy or too filesystem-specific
//! for the app layer: backup/restore, compaction, moving originals to the
//! Trash, and measuring the catalog's on-disk footprint.

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rustyedit_core::{Catalog, CatalogDef};
use serde::{Deserialize, Serialize};

use crate::{catalog_registry, catalog_store};

const BACKUP_VERSION: u32 = 1;

/// A self-contained snapshot of one catalog: its definition (folders,
/// albums, smart collections, keyword sets) plus every folder's per-photo
/// sidecar. Stored as a single JSON file the user can move anywhere.
#[derive(Debug, Serialize, Deserialize)]
pub struct CatalogBackup {
    pub version: u32,
    pub created_at: u64,
    pub def: CatalogDef,
    pub sidecars: Vec<BackupSidecar>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupSidecar {
    pub folder: PathBuf,
    pub catalog: Catalog,
}

/// How much a compaction saved.
#[derive(Debug, Clone, Copy)]
pub struct OptimizeReport {
    pub before: u64,
    pub after: u64,
}

impl OptimizeReport {
    pub fn saved(&self) -> u64 {
        self.before.saturating_sub(self.after)
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn sidecar_path(folder: &Path) -> PathBuf {
    folder.join(".rustyedit").join("catalog.json")
}

fn sidecar_size(catalog: &Catalog) -> u64 {
    let mut folders = catalog.folders.clone();
    if folders.is_empty() {
        folders.extend(catalog.folder.clone());
    }
    folders
        .iter()
        .map(|folder| {
            std::fs::metadata(sidecar_path(folder))
                .map(|m| m.len())
                .unwrap_or(0)
        })
        .sum()
}

/// Gathers the catalog definition and every sidecar it references.
pub fn build_backup(def: &CatalogDef) -> anyhow::Result<CatalogBackup> {
    let mut sidecars = Vec::new();
    for folder in &def.folders {
        if let Some(catalog) = catalog_store::load(folder) {
            sidecars.push(BackupSidecar {
                folder: folder.clone(),
                catalog,
            });
        }
    }
    Ok(CatalogBackup {
        version: BACKUP_VERSION,
        created_at: now_secs(),
        def: def.clone(),
        sidecars,
    })
}

/// Writes a backup file atomically.
pub fn write_backup(backup: &CatalogBackup, path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(backup)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn read_backup(path: &Path) -> anyhow::Result<CatalogBackup> {
    let data = std::fs::read(path)?;
    let backup: CatalogBackup = serde_json::from_slice(&data)?;
    if backup.version > BACKUP_VERSION {
        anyhow::bail!(
            "This backup was made by a newer version (format {}).",
            backup.version
        );
    }
    Ok(backup)
}

/// Puts a backup's definition back into the registry and rewrites the
/// per-folder sidecars it contains.
pub fn restore_backup(backup: &CatalogBackup) -> anyhow::Result<()> {
    catalog_registry::save(&backup.def)?;
    for sidecar in &backup.sidecars {
        let mut catalog = sidecar.catalog.clone();
        catalog.folder = Some(sidecar.folder.clone());
        if catalog.folders.is_empty() {
            catalog.folders.push(sidecar.folder.clone());
        }
        catalog_store::save(&catalog)?;
    }
    Ok(())
}

/// Rewrites every sidecar in compact form (dropping accumulated whitespace)
/// so large catalogs open faster and use less disk. Returns before/after
/// byte counts.
pub fn optimize_catalog(catalog: &Catalog) -> anyhow::Result<OptimizeReport> {
    let before = sidecar_size(catalog);
    catalog_store::save(catalog)?;
    let after = sidecar_size(catalog);
    Ok(OptimizeReport { before, after })
}

/// Moves an original file to the system Trash / Recycle Bin, on macOS,
/// Windows and Linux alike. Unlike renaming into a trash folder, this works
/// for files on other drives and keeps "Put Back" working. Returns the path
/// the file had.
pub fn move_to_trash(path: &Path) -> anyhow::Result<PathBuf> {
    trash::delete(path).map_err(|err| anyhow::anyhow!("{}: {err}", path.display()))?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyedit_core::Asset;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rustyedit-test-{name}-{}-{}",
            std::process::id(),
            now_secs()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn backup_round_trips_sidecars() {
        let dir = temp_dir("backup");
        let mut catalog = Catalog::new(dir.clone(), vec![Asset::new(0, dir.join("a.jpg"))]);
        catalog.assets[0].caption = "hello".into();
        catalog_store::save(&catalog).unwrap();

        let mut def = CatalogDef::new("test", "Test");
        def.add_folder(&dir);
        let backup = build_backup(&def).unwrap();
        assert_eq!(backup.sidecars.len(), 1);

        let file = dir.join("backup.json");
        write_backup(&backup, &file).unwrap();
        let read = read_backup(&file).unwrap();
        assert_eq!(read.sidecars[0].catalog.assets[0].caption, "hello");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn move_to_trash_removes_the_file() {
        let dir = temp_dir("trash");
        let source = dir.join("photo.jpg");
        std::fs::write(&source, b"x").unwrap();
        // CI machines may have no trash to move to; only check the result
        // when the move worked.
        if move_to_trash(&source).is_ok() {
            assert!(!source.exists());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
