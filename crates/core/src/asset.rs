use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{color_label::ColorLabel, develop::DevelopSettings, flag::Flag, rating::Rating};

/// A single photo known to the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: u64,
    pub path: PathBuf,
    pub thumbnail_path: Option<PathBuf>,
    pub width: u32,
    pub height: u32,
    pub flag: Flag,
    pub rating: Rating,
    /// Defaulted so catalogs saved before colour labels existed still load.
    #[serde(default)]
    pub color_label: ColorLabel,
    pub develop: DevelopSettings,
}

impl Asset {
    pub fn new(id: u64, path: PathBuf) -> Self {
        Self {
            id,
            path,
            thumbnail_path: None,
            width: 0,
            height: 0,
            flag: Flag::default(),
            rating: Rating::default(),
            color_label: ColorLabel::default(),
            develop: DevelopSettings::default(),
        }
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
