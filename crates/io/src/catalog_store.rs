use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use rustyedit_core::{Asset, Catalog, FilterMode};

/// Where a given source folder's catalog sidecar (flags, ratings, edits)
/// is persisted, mirroring Lightroom's "catalog remembers your culling"
/// behavior without needing a database.
fn sidecar_path(folder: &Path) -> PathBuf {
    folder.join(".rustyedit").join("catalog.json")
}

pub fn load(folder: &Path) -> Option<Catalog> {
    let path = sidecar_path(folder);
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Writes the catalog's per-photo state back to the sidecar of whichever
/// folder owns each photo, so a folder keeps its culling and edits no matter
/// which catalogs include it.
pub fn save(catalog: &Catalog) -> anyhow::Result<()> {
    let mut folders = catalog.folders.clone();
    if folders.is_empty() {
        folders.extend(catalog.folder.clone());
    }
    if folders.is_empty() {
        return Err(anyhow::anyhow!("catalog has no folder to save alongside"));
    }
    // Deepest folder first, so a photo goes to its most specific owner.
    folders.sort_by_key(|f| std::cmp::Reverse(f.components().count()));

    // Which photos each folder owns, by reference: they're written straight
    // out, never copied.
    let mut owned: Vec<Vec<&Asset>> = vec![Vec::new(); folders.len()];
    for asset in &catalog.assets {
        if let Some(ix) = folders.iter().position(|f| asset.path.starts_with(f)) {
            owned[ix].push(asset);
        }
    }

    let mut first_error = None;
    for (folder, assets) in folders.iter().zip(&owned) {
        if let Err(err) = save_folder(folder, assets) {
            first_error.get_or_insert(err);
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// What a folder's sidecar holds: the same shape as a [`Catalog`] made of
/// just this folder's photos, without building one.
#[derive(serde::Serialize)]
struct FolderCatalog<'a> {
    folder: &'a Path,
    folders: [&'a Path; 1],
    assets: &'a [&'a Asset],
    filter: FilterMode,
}

/// A hash of the bytes last written to each sidecar, so an unchanged
/// folder isn't written again.
fn last_written() -> &'static std::sync::Mutex<HashMap<PathBuf, u64>> {
    static LAST: std::sync::OnceLock<std::sync::Mutex<HashMap<PathBuf, u64>>> =
        std::sync::OnceLock::new();
    LAST.get_or_init(Default::default)
}

fn save_folder(folder: &Path, assets: &[&Asset]) -> anyhow::Result<()> {
    let path = sidecar_path(folder);
    // Compact JSON: pretty-printing roughly doubles the file for large
    // catalogs.
    let data = serde_json::to_vec(&FolderCatalog {
        folder,
        folders: [folder],
        assets,
        filter: FilterMode::All,
    })?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&data, &mut hasher);
    let hash = std::hash::Hasher::finish(&hasher);
    // Nothing changed since this was last written (and it's still there):
    // leave the disk alone.
    if last_written().lock().ok().and_then(|m| m.get(&path).copied()) == Some(hash)
        && path.exists()
    {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Written to a temp file and renamed into place so a crash mid-write
    // can never leave a truncated catalog behind.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, &path)?;
    if let Ok(mut last) = last_written().lock() {
        last.insert(path, hash);
    }
    Ok(())
}

/// Saved photos whose originals are no longer on disk. Returned as assets
/// (marked `missing`) so they can stay listed in the catalog for relocation
/// or explicit removal instead of silently disappearing.
pub fn missing_assets(saved: &Catalog) -> Vec<Asset> {
    saved
        .assets
        .iter()
        .filter(|asset| !asset.path.exists())
        .cloned()
        .map(|mut asset| {
            asset.missing = true;
            asset
        })
        .collect()
}

/// Carries flags, ratings, colour labels and develop settings from a previously saved
/// catalog onto a freshly rescanned asset list (matched by file path), so
/// re-opening a folder never loses prior culling or edit work even if files
/// were added or removed since the last save. Saved virtual copies of files
/// that are still there come back too, each right after its original.
pub fn merge_saved_state(fresh: &mut Vec<Asset>, saved: &Catalog) {
    let by_path: HashMap<&Path, &Asset> = saved
        .assets
        .iter()
        .filter(|asset| !asset.is_virtual_copy())
        .map(|asset| (asset.path.as_path(), asset))
        .collect();

    for asset in fresh.iter_mut() {
        if let Some(previous) = by_path.get(asset.path.as_path()) {
            asset.flag = previous.flag;
            asset.rating = previous.rating;
            asset.color_label = previous.color_label;
            asset.develop = previous.develop;
            asset.title = previous.title.clone();
            asset.caption = previous.caption.clone();
            asset.creator = previous.creator.clone();
            asset.copyright = previous.copyright.clone();
            asset.keywords = previous.keywords.clone();
            asset.stack_id = previous.stack_id;
            asset.stack_cover = previous.stack_cover;
            asset.content_hash = previous.content_hash;
            asset.phash = previous.phash;
            asset.sharpness = previous.sharpness;
            asset.history = previous.history.clone();
            asset.snapshots = previous.snapshots.clone();
            asset.applied_preset = previous.applied_preset.clone();
            // Remembered so reopening never has to re-read every file.
            asset.width = previous.width;
            asset.height = previous.height;
            asset.orientation = previous.orientation;
            asset.meta = previous.meta.clone();
        }
    }

    let mut copies: HashMap<&Path, Vec<&Asset>> = HashMap::new();
    for asset in saved.assets.iter().filter(|a| a.is_virtual_copy()) {
        copies.entry(asset.path.as_path()).or_default().push(asset);
    }
    if copies.is_empty() {
        return;
    }
    let mut merged = Vec::with_capacity(fresh.len());
    for asset in fresh.drain(..) {
        let own = copies.remove(asset.path.as_path()).unwrap_or_default();
        merged.push(asset);
        merged.extend(own.into_iter().map(|copy| {
            let mut copy = copy.clone();
            copy.missing = false;
            copy
        }));
    }
    *fresh = merged;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merging_restores_virtual_copies_after_their_original() {
        let a = PathBuf::from("/p/a.jpg");
        let b = PathBuf::from("/p/b.jpg");
        let gone = PathBuf::from("/p/gone.jpg");
        let mut master = Asset::new(0, a.clone());
        master.develop.exposure = 10.0;
        let mut copy = Asset::new(1, a.clone());
        copy.copy_name = Some("Copy 1".into());
        copy.develop.exposure = -30.0;
        let mut orphan = Asset::new(2, gone);
        orphan.copy_name = Some("Copy 1".into());
        let saved = Catalog::new(PathBuf::from("/p"), vec![copy, master, orphan]);

        let mut fresh = vec![Asset::new(0, a.clone()), Asset::new(1, b.clone())];
        merge_saved_state(&mut fresh, &saved);
        assert_eq!(fresh.len(), 3);
        assert_eq!(fresh[0].path, a);
        assert!(!fresh[0].is_virtual_copy());
        // The original keeps its own edits, not the copy's.
        assert_eq!(fresh[0].develop.exposure, 10.0);
        assert_eq!(fresh[1].copy_name.as_deref(), Some("Copy 1"));
        assert_eq!(fresh[1].develop.exposure, -30.0);
        assert_eq!(fresh[2].path, b);
    }

    #[test]
    fn missing_assets_flags_only_vanished_files() {
        let dir = std::env::temp_dir().join(format!(
            "rustyedit-missing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let present = dir.join("present.jpg");
        std::fs::write(&present, b"x").unwrap();
        let gone = dir.join("gone.jpg");

        let saved = Catalog::new(
            dir.clone(),
            vec![Asset::new(0, present.clone()), Asset::new(1, gone.clone())],
        );
        let missing = missing_assets(&saved);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].path, gone);
        assert!(missing[0].missing);
        assert!(missing.iter().all(|asset| asset.path != present));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod stack_tests {
    use super::*;

    /// Catalogs load on worker threads (Grand Central Dispatch's have
    /// 512 KB of stack), so deserializing a big one must fit.
    #[test]
    fn a_large_catalog_loads_on_a_small_stack() {
        let mut catalog = Catalog::default();
        for i in 0..400 {
            let mut asset = Asset::new(i, PathBuf::from(format!("/photos/{i}.jpg")));
            asset.develop.exposure = 10.0;
            catalog.assets.push(asset);
        }
        let json = serde_json::to_string(&catalog).unwrap();
        let loaded = std::thread::Builder::new()
            .stack_size(512 * 1024)
            .spawn(move || serde_json::from_str::<Catalog>(&json).map(|c| c.assets.len()))
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(loaded.unwrap(), 400);
    }
}

#[cfg(test)]
mod save_tests {
    use super::*;

    fn catalog_in(dir: &Path) -> Catalog {
        let assets = (0..3)
            .map(|i| {
                let mut asset = Asset::new(i, dir.join(format!("{i}.jpg")));
                asset.develop.exposure = i as f32 * 10.0;
                asset.keywords = vec![format!("k{i}")];
                asset
            })
            .collect();
        Catalog::new(dir.to_path_buf(), assets)
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rustyedit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_sidecar_is_the_same_as_a_catalog_of_the_folders_photos() {
        let dir = temp("save-shape");
        let catalog = catalog_in(&dir);
        save(&catalog).unwrap();
        // Byte for byte what serializing a Catalog of those photos gives.
        let written = std::fs::read(sidecar_path(&dir)).unwrap();
        assert_eq!(written, serde_json::to_vec(&catalog).unwrap());
        // And it loads back.
        let loaded = load(&dir).unwrap();
        assert_eq!(loaded.assets.len(), 3);
        assert_eq!(loaded.assets[2].develop.exposure, 20.0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unchanged_catalog_is_not_written_again() {
        let dir = temp("save-skip");
        let mut catalog = catalog_in(&dir);
        save(&catalog).unwrap();
        let path = sidecar_path(&dir);
        let first = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        save(&catalog).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), first);
        // A change is written; and a deleted sidecar is put back.
        catalog.assets[0].develop.exposure = 99.0;
        save(&catalog).unwrap();
        assert_eq!(load(&dir).unwrap().assets[0].develop.exposure, 99.0);
        std::fs::remove_file(&path).unwrap();
        save(&catalog).unwrap();
        assert!(path.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
