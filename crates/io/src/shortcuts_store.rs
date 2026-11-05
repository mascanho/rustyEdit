//! User-customised keyboard shortcuts: a small table of overrides (action
//! name -> keystroke), kept separate from the rest of `Settings` since it's
//! edited one entry at a time from the shortcut sheet rather than a
//! settings page.

use std::{collections::HashMap, path::PathBuf};

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rustyedit").join("shortcuts.json"))
}

/// Every customised shortcut, by the action's own stable name (see
/// `gpui::Action::name`). Empty when nothing has been changed, or on the
/// first run.
pub fn load_shortcut_overrides() -> HashMap<String, String> {
    let Some(path) = path() else {
        return HashMap::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

pub fn save_shortcut_overrides(overrides: &HashMap<String, String>) -> anyhow::Result<()> {
    let path = path().ok_or_else(|| anyhow::anyhow!("no config directory"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(overrides)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_table_is_valid_json() {
        // Just exercises the serialization shape the store relies on;
        // the real path isn't touched by this test.
        let overrides: HashMap<String, String> = HashMap::new();
        let json = serde_json::to_string(&overrides).unwrap();
        let back: HashMap<String, String> = serde_json::from_str(&json).unwrap();
        assert!(back.is_empty());
    }
}
