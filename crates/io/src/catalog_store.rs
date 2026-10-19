use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use rustyedit_core::{Asset, Catalog};

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

    let mut owned: Vec<Vec<Asset>> = vec![Vec::new(); folders.len()];
    for asset in &catalog.assets {
        if let Some(ix) = folders.iter().position(|f| asset.path.starts_with(f)) {
            owned[ix].push(asset.clone());
        }
    }

    let mut first_error = None;
    for (folder, assets) in folders.into_iter().zip(owned) {
        let part = Catalog::new(folder, assets);
        if let Err(err) = save_folder(&part) {
            first_error.get_or_insert(err);
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn save_folder(catalog: &Catalog) -> anyhow::Result<()> {
    let folder = catalog
        .folder
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("catalog has no folder to save alongside"))?;
    let path = sidecar_path(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Compact JSON: pretty-printing roughly doubles the file for large
    // catalogs. Written to a temp file and renamed into place so a crash
    // mid-write can never leave a truncated catalog behind.
    let data = serde_json::to_vec(catalog)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Carries flags, ratings, colour labels and develop settings from a previously saved
/// catalog onto a freshly rescanned asset list (matched by file path), so
/// re-opening a folder never loses prior culling or edit work even if files
/// were added or removed since the last save.
pub fn merge_saved_state(fresh: &mut [Asset], saved: &Catalog) {
    let by_path: HashMap<&Path, &Asset> = saved
        .assets
        .iter()
        .map(|asset| (asset.path.as_path(), asset))
        .collect();

    for asset in fresh.iter_mut() {
        if let Some(previous) = by_path.get(asset.path.as_path()) {
            asset.flag = previous.flag;
            asset.rating = previous.rating;
            asset.color_label = previous.color_label;
            asset.develop = previous.develop;
            asset.history = previous.history.clone();
            // Remembered so reopening never has to re-read every file.
            asset.width = previous.width;
            asset.height = previous.height;
            asset.orientation = previous.orientation;
        }
    }
}
