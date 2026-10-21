use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    color_label::ColorLabel, develop::DevelopSettings, flag::Flag, history, history::HistoryEntry,
    rating::Rating, snapshot::Snapshot,
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
#[derive(Debug, Clone, PartialEq)]
pub struct AssetState {
    pub flag: Flag,
    pub rating: Rating,
    pub color_label: ColorLabel,
    pub develop: DevelopSettings,
    pub title: String,
    pub caption: String,
    pub creator: String,
    pub copyright: String,
    pub keywords: Vec<String>,
    pub stack_id: Option<u64>,
    pub stack_cover: bool,
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
    /// User-editable descriptive metadata. Defaulted so catalogs saved before
    /// these fields existed still load.
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub caption: String,
    #[serde(default)]
    pub creator: String,
    #[serde(default)]
    pub copyright: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Photos sharing a stack id belong to one stack; `stack_cover` marks the
    /// representative used when the stack is collapsed.
    #[serde(default)]
    pub stack_id: Option<u64>,
    #[serde(default)]
    pub stack_cover: bool,
    /// Sampled file-content hash, for exact-duplicate detection.
    #[serde(default)]
    pub content_hash: u64,
    /// 64-bit difference hash (dHash) of the thumbnail, for similar-photo
    /// detection.
    #[serde(default)]
    pub phash: u64,
    /// Every change made to this photo, oldest first. Defaulted so catalogs
    /// saved before history existed still load.
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
    /// Saved alternative Develop states, oldest first.
    #[serde(default)]
    pub snapshots: Vec<Snapshot>,
    /// Set when the file recorded in the catalog no longer exists on disk.
    /// Runtime-only: it is recomputed on every scan, so it is never saved.
    #[serde(skip)]
    pub missing: bool,
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
            title: String::new(),
            caption: String::new(),
            creator: String::new(),
            copyright: String::new(),
            keywords: Vec::new(),
            stack_id: None,
            stack_cover: false,
            content_hash: 0,
            phash: 0,
            history: Vec::new(),
            snapshots: Vec::new(),
            missing: false,
        }
    }

    /// How many clockwise 90-degree turns the develop rotation represents.
    pub fn rotation_quarters(&self) -> u8 {
        (((self.develop.rotation_degrees / 90.0).round() as i32).rem_euclid(4)) as u8
    }

    /// Adds `keyword` unless the photo already has it (case-insensitively).
    /// Returns whether it was added.
    pub fn add_keyword(&mut self, keyword: &str) -> bool {
        let keyword = keyword.trim();
        if keyword.is_empty()
            || self
                .keywords
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(keyword))
        {
            return false;
        }
        self.keywords.push(keyword.to_string());
        true
    }

    /// Removes `keyword` (case-insensitively). Returns whether it was present.
    pub fn remove_keyword(&mut self, keyword: &str) -> bool {
        let before = self.keywords.len();
        self.keywords
            .retain(|existing| !existing.eq_ignore_ascii_case(keyword.trim()));
        self.keywords.len() != before
    }

    /// Logs a change to this photo (see [`HistoryEntry`]).
    pub fn record_change(&mut self, label: &str, from: impl Into<String>, to: impl Into<String>) {
        history::record(&mut self.history, label, from.into(), to.into(), None);
    }

    /// Logs a change to the Develop settings, remembering the settings it
    /// produced so the change can be stepped back to later.
    pub fn record_develop_change(
        &mut self,
        label: &str,
        from: impl Into<String>,
        to: impl Into<String>,
    ) {
        let develop = Some(self.develop);
        history::record(&mut self.history, label, from.into(), to.into(), develop);
    }

    /// A snapshot of the undoable state.
    pub fn state(&self) -> AssetState {
        AssetState {
            flag: self.flag,
            rating: self.rating,
            color_label: self.color_label,
            develop: self.develop,
            title: self.title.clone(),
            caption: self.caption.clone(),
            creator: self.creator.clone(),
            copyright: self.copyright.clone(),
            keywords: self.keywords.clone(),
            stack_id: self.stack_id,
            stack_cover: self.stack_cover,
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
