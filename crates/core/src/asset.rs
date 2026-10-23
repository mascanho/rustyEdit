use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    sections::DevelopSection,
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
    pub applied_preset: Option<AppliedPreset>,
}

/// The preset that makes up a photo's look. Only one applies at a time:
/// applying another replaces it completely.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppliedPreset {
    pub name: String,
    /// The photo's settings from before the preset. The next preset starts
    /// from these rather than from this preset's look, so none of this one
    /// is left behind (even at partial strength).
    pub base: DevelopSettings,
    /// How strongly it was applied, 0-1.
    #[serde(default = "full_strength")]
    pub amount: f32,
}

fn full_strength() -> f32 {
    1.0
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
    /// Set on a virtual copy: another entry for the same file with its own
    /// edits, flags and metadata. Holds its name, e.g. "Copy 1". `None` for
    /// the original (master) photo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy_name: Option<String>,
    /// The preset currently making up the photo's look, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_preset: Option<AppliedPreset>,
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
            copy_name: None,
            applied_preset: None,
            missing: false,
        }
    }

    /// Takes everything set on `old` (the same photo as previously loaded):
    /// culling, edits, metadata, stacking, history, and the facts read from
    /// the file, so a rescan never loses work. The file-system side (path,
    /// thumbnail, missing) stays as scanned.
    pub fn carry_over_from(&mut self, old: &Asset) {
        self.flag = old.flag;
        self.rating = old.rating;
        self.color_label = old.color_label;
        self.develop = old.develop;
        self.title = old.title.clone();
        self.caption = old.caption.clone();
        self.creator = old.creator.clone();
        self.copyright = old.copyright.clone();
        self.keywords = old.keywords.clone();
        self.stack_id = old.stack_id;
        self.stack_cover = old.stack_cover;
        self.history = old.history.clone();
        self.snapshots = old.snapshots.clone();
        self.copy_name = old.copy_name.clone();
        self.applied_preset = old.applied_preset.clone();
        if old.content_hash != 0 {
            self.content_hash = old.content_hash;
        }
        if old.phash != 0 {
            self.phash = old.phash;
        }
        if old.width > 0 && old.height > 0 {
            self.width = old.width;
            self.height = old.height;
            self.orientation = old.orientation;
        }
        if old.meta != PhotoMeta::default() {
            self.meta = old.meta.clone();
        }
    }

    /// The strength (0-1) of the photo's current preset, if it has one.
    pub fn current_preset_amount(&self) -> Option<f32> {
        self.current_preset()?;
        self.applied_preset.as_ref().map(|p| p.amount)
    }

    /// The name of the preset making up the photo's look, while it still
    /// has edits.
    pub fn current_preset(&self) -> Option<&str> {
        self.applied_preset
            .as_ref()
            .filter(|_| self.develop.is_edited())
            .map(|p| p.name.as_str())
    }

    /// Where a newly applied preset starts from: the photo as it is now,
    /// but with the look sections as they were before its current preset,
    /// so the new preset replaces that one entirely. Geometry, retouching
    /// and masks stay as they are now.
    pub fn preset_base(&self) -> DevelopSettings {
        let mut start = self.develop;
        if let Some(previous) = self.applied_preset.as_ref().filter(|_| self.develop.is_edited()) {
            for section in DevelopSection::LOOK {
                section.copy(&previous.base, &mut start);
            }
        }
        start
    }

    /// Applies `look` (only its `sections`, at `amount`) as the photo's one
    /// preset, replacing any preset applied before.
    pub fn apply_preset(
        &mut self,
        name: &str,
        look: &DevelopSettings,
        sections: &[DevelopSection],
        amount: f32,
    ) {
        let base = self.preset_base();
        self.develop = base.with_look_blended(look, sections, amount);
        self.applied_preset = Some(AppliedPreset {
            name: name.to_string(),
            base,
            amount: amount.clamp(0.0, 1.0),
        });
    }

    /// Whether this entry is a virtual copy rather than the original photo.
    pub fn is_virtual_copy(&self) -> bool {
        self.copy_name.is_some()
    }

    /// The file name, followed by the copy's name for a virtual copy, e.g.
    /// "IMG_0042.CR3 (Copy 1)". For showing photos apart in the UI.
    pub fn display_name(&self) -> String {
        match &self.copy_name {
            Some(copy) => format!("{} ({copy})", self.file_name()),
            None => self.file_name(),
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
            applied_preset: self.applied_preset.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_preset_replaces_the_previous_one_completely() {
        let mut photo = Asset::new(0, PathBuf::from("a.jpg"));
        photo.develop.exposure = 10.0;
        photo.develop.rotation_degrees = 90.0;
        let warm = DevelopSettings {
            temperature: 40.0,
            contrast: 30.0,
            ..Default::default()
        };
        let cool = DevelopSettings {
            temperature: -20.0,
            ..Default::default()
        };

        photo.apply_preset("Warm", &warm, &DevelopSection::LOOK, 1.0);
        assert_eq!(photo.develop.temperature, 40.0);
        assert_eq!(photo.current_preset(), Some("Warm"));

        // At half strength, Cool blends from the photo before Warm: nothing
        // of Warm's temperature or contrast is left.
        photo.apply_preset("Cool", &cool, &DevelopSection::LOOK, 0.5);
        assert_eq!(photo.develop.temperature, -10.0);
        assert_eq!(photo.develop.contrast, 0.0);
        assert_eq!(photo.develop.exposure, 5.0);
        assert_eq!(photo.current_preset(), Some("Cool"));
        // Geometry is never touched.
        assert_eq!(photo.develop.rotation_degrees, 90.0);

        // Applying Warm again lands exactly where Warm did the first time.
        photo.apply_preset("Warm", &warm, &DevelopSection::LOOK, 1.0);
        assert_eq!(photo.develop.temperature, 40.0);
        assert_eq!(photo.develop.contrast, 30.0);
    }

    #[test]
    fn changing_a_presets_strength_is_exact_and_reversible() {
        let mut photo = Asset::new(0, PathBuf::from("a.jpg"));
        photo.develop.exposure = 10.0;
        let look = DevelopSettings {
            exposure: 50.0,
            ..Default::default()
        };
        photo.apply_preset("Bright", &look, &DevelopSection::LOOK, 1.0);
        assert_eq!(photo.develop.exposure, 50.0);
        photo.apply_preset("Bright", &look, &DevelopSection::LOOK, 0.5);
        assert_eq!(photo.develop.exposure, 30.0);
        assert_eq!(photo.current_preset_amount(), Some(0.5));
        photo.apply_preset("Bright", &look, &DevelopSection::LOOK, 0.0);
        assert_eq!(photo.develop.exposure, 10.0);
        photo.apply_preset("Bright", &look, &DevelopSection::LOOK, 1.0);
        assert_eq!(photo.develop.exposure, 50.0);
    }

    #[test]
    fn crops_made_after_a_preset_survive_the_next_one() {
        let mut photo = Asset::new(0, PathBuf::from("a.jpg"));
        let look = DevelopSettings {
            vibrance: 20.0,
            ..Default::default()
        };
        photo.apply_preset("A", &look, &DevelopSection::LOOK, 1.0);
        photo.develop.rotation_degrees = 180.0;
        photo.apply_preset("B", &look, &DevelopSection::LOOK, 1.0);
        assert_eq!(photo.develop.rotation_degrees, 180.0);
    }
}
