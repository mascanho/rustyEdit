use std::path::PathBuf;

use rustyedit_core::Albums;

/// Albums are app-wide, so they live beside the settings file rather than in
/// any one folder's catalog.
fn albums_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("rustyedit").join("albums.json"))
}

/// Loads saved albums, falling back to none if the file is missing or
/// unreadable.
pub fn load() -> Albums {
    albums_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

pub fn save(albums: &Albums) -> anyhow::Result<()> {
    let path = albums_path().ok_or_else(|| anyhow::anyhow!("no config directory"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(albums)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}
