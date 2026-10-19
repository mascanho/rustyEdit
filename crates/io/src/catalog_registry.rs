//! The set of named catalogs, stored one JSON file each under the config
//! directory, plus which one was open last.

use std::path::PathBuf;

use rustyedit_core::CatalogDef;

fn dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rustyedit").join("catalogs"))
}

fn file(id: &str) -> Option<PathBuf> {
    dir().map(|d| d.join(format!("{id}.json")))
}

fn active_file() -> Option<PathBuf> {
    dir().map(|d| d.join("active.txt"))
}

/// Every saved catalog, sorted by name.
pub fn list() -> Vec<CatalogDef> {
    let Some(dir) = dir() else { return Vec::new() };
    let mut defs: Vec<CatalogDef> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "json"))
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .filter_map(|data| serde_json::from_str(&data).ok())
        .collect();
    defs.sort_by_key(|d| d.name.to_lowercase());
    defs
}

pub fn load(id: &str) -> Option<CatalogDef> {
    serde_json::from_str(&std::fs::read_to_string(file(id)?).ok()?).ok()
}

pub fn save(def: &CatalogDef) -> anyhow::Result<()> {
    let path = file(&def.id).ok_or_else(|| anyhow::anyhow!("no config directory"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(def)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn delete(id: &str) {
    if let Some(path) = file(id) {
        let _ = std::fs::remove_file(path);
    }
}

/// Creates and saves an empty catalog called `name` with a fresh unique id.
pub fn create(name: &str) -> anyhow::Result<CatalogDef> {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let slug = if slug.is_empty() {
        "catalog".to_string()
    } else {
        slug
    };

    let mut id = slug.clone();
    let mut n = 2;
    while file(&id).is_some_and(|p| p.exists()) {
        id = format!("{slug}-{n}");
        n += 1;
    }
    let def = CatalogDef::new(id, name.trim());
    save(&def)?;
    Ok(def)
}

pub fn active_id() -> Option<String> {
    std::fs::read_to_string(active_file()?)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn set_active(id: &str) {
    if let Some(path) = active_file() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, id);
    }
}
