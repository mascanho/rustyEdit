use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    color_label::ColorLabel, develop::DevelopSettings, flag::Flag, history, history::HistoryEntry,
    rating::Rating,
};

/// Facts read from the file's EXIF once, kept so sorting and filtering never
/// have to reopen the originals.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PhotoMeta {
    /// Capture time as seconds since the Unix epoch (EXIF time read as UTC;
    /// the file's modified time when it has none). 0 = unknown.
    pub captured: i64,
    /// Make and model, e.g. "SONY ILCE-7M2".
    pub camera: String,
    pub lens: String,
    pub iso: u32,
    pub focal_mm: f32,
}

impl PhotoMeta {
    /// Calendar year of the capture time.
    pub fn year(&self) -> Option<i32> {
        if self.captured <= 0 {
            return None;
        }
        // Civil-from-days (Howard Hinnant's algorithm), year only.
        let z = self.captured.div_euclid(86_400) + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let year = yoe + era * 400 + i64::from(mp >= 10);
        Some(year as i32)
    }
}

fn upright() -> u8 {
    1
}

/// Everything about a photo the user can change and undo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AssetState {
    pub flag: Flag,
    pub rating: Rating,
    pub color_label: ColorLabel,
    pub develop: DevelopSettings,
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
    #[serde(default)]
    pub meta: PhotoMeta,
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
            meta: PhotoMeta::default(),
            history: Vec::new(),
        }
    }

    /// Logs a change to this photo (see [`HistoryEntry`]).
    pub fn record_change(&mut self, label: &str, from: impl Into<String>, to: impl Into<String>) {
        history::record(&mut self.history, label, from.into(), to.into());
    }

    /// A snapshot of the undoable state.
    pub fn state(&self) -> AssetState {
        AssetState {
            flag: self.flag,
            rating: self.rating,
            color_label: self.color_label,
            develop: self.develop,
        }
    }

    /// Upper-case file extension, e.g. "DNG".
    pub fn kind(&self) -> String {
        self.path
            .extension()
            .map(|e| e.to_string_lossy().to_uppercase())
            .unwrap_or_default()
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
