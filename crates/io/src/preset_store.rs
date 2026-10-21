use std::path::PathBuf;

use rustyedit_core::Presets;

/// Develop presets are app-wide, so they live beside the settings file
/// rather than in any one folder's catalog.
fn presets_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("rustyedit").join("presets.json"))
}

/// Loads saved presets, falling back to none if the file is missing or
/// unreadable.
pub fn load() -> Presets {
    presets_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

pub fn save(presets: &Presets) -> anyhow::Result<()> {
    let path = presets_path().ok_or_else(|| anyhow::anyhow!("no config directory"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(presets)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Writes `presets` to a file the user can share.
pub fn export_to(path: &std::path::Path, presets: &[rustyedit_core::DevelopPreset]) -> anyhow::Result<()> {
    let file = Presets {
        user: presets.to_vec(),
        builtin_favorites: Vec::new(),
    };
    std::fs::write(path, serde_json::to_vec_pretty(&file)?)?;
    Ok(())
}

/// Reads presets from a file written by [`export_to`] (or a presets file).
pub fn import_from(path: &std::path::Path) -> anyhow::Result<Vec<rustyedit_core::DevelopPreset>> {
    let data = std::fs::read_to_string(path)?;
    if let Ok(single) = serde_json::from_str::<rustyedit_core::DevelopPreset>(&data) {
        return Ok(vec![single]);
    }
    if let Ok(list) = serde_json::from_str::<Vec<rustyedit_core::DevelopPreset>>(&data) {
        return Ok(list);
    }
    Ok(serde_json::from_str::<Presets>(&data)?.user)
}
