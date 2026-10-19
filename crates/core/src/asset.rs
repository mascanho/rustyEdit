use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    color_label::ColorLabel, develop::DevelopSettings, flag::Flag, history, history::HistoryEntry,
    rating::Rating,
};

fn upright() -> u8 {
    1
}

/// A single photo known to the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: u64,
    pub path: PathBuf,
    pub thumbnail_path: Option<PathBuf>,
    pub width: u32,
    pub height: u32,
    /// EXIF orientation (1 = upright). Defaulted for old catalogs.
    #[serde(default = "upright")]
    pub orientation: u8,
    pub flag: Flag,
    pub rating: Rating,
    /// Defaulted so catalogs saved before colour labels existed still load.
    #[serde(default)]
    pub color_label: ColorLabel,
    pub develop: DevelopSettings,
    /// Every change made to this photo, oldest first. Defaulted so catalogs
    /// saved before history existed still load.
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
}

impl Asset {
    pub fn new(id: u64, path: PathBuf) -> Self {
        Self {
            id,
            path,
            thumbnail_path: None,
            width: 0,
            height: 0,
            orientation: 1,
            flag: Flag::default(),
            rating: Rating::default(),
            color_label: ColorLabel::default(),
            develop: DevelopSettings::default(),
            history: Vec::new(),
        }
    }

    /// Logs a change to this photo (see [`HistoryEntry`]).
    pub fn record_change(&mut self, label: &str, from: impl Into<String>, to: impl Into<String>) {
        history::record(&mut self.history, label, from.into(), to.into());
    }

    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// The best image we currently have to display: the generated thumbnail
    /// if one exists yet, otherwise the full source file.
    pub fn display_source(&self) -> &Path {
        self.thumbnail_path.as_deref().unwrap_or(&self.path)
    }
}
