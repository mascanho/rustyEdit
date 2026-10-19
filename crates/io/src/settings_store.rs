use std::path::PathBuf;

use rustyedit_core::Settings;

/// Where app-wide preferences live: the platform config dir (on macOS,
/// `~/Library/Application Support/rustyedit/settings.json`). Unlike the
/// catalog sidecar, these apply to every folder.
pub fn settings_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("rustyedit").join("settings.json"))
}

/// Loads saved settings, falling back to defaults if the file is missing or
/// unreadable so a corrupt file can never stop the app from starting.
pub fn load() -> Settings {
    settings_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

pub fn save(settings: &Settings) -> anyhow::Result<()> {
    let path = settings_path().ok_or_else(|| anyhow::anyhow!("no config directory"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(settings)?)?;
    Ok(())
}
