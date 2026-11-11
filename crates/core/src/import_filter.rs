//! Choosing which photos of a folder to import: by file type, star rating,
//! colour label and whether the file is protected (read-only). Ratings and
//! labels are those set in the camera or another program, which the import
//! reads from the photo's sidecar or embedded metadata.

use std::{collections::BTreeSet, path::PathBuf};

use crate::ColorLabel;

/// The kinds of file offered in the filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileKind {
    Raw,
    Dng,
    Jpeg,
    Tiff,
    Png,
    Other,
}

impl FileKind {
    pub const ALL: [FileKind; 6] = [
        FileKind::Raw,
        FileKind::Dng,
        FileKind::Jpeg,
        FileKind::Tiff,
        FileKind::Png,
        FileKind::Other,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            FileKind::Raw => "RAW",
            FileKind::Dng => "DNG",
            FileKind::Jpeg => "JPG",
            FileKind::Tiff => "TIFF",
            FileKind::Png => "PNG",
            FileKind::Other => "Other",
        }
    }

    /// The kind a file with this (lower-case) extension is; `is_raw` says
    /// whether it is a camera RAW format.
    pub fn of_extension(ext: &str, is_raw: bool) -> FileKind {
        match ext {
            "dng" => FileKind::Dng,
            "jpg" | "jpeg" => FileKind::Jpeg,
            "tif" | "tiff" => FileKind::Tiff,
            "png" => FileKind::Png,
            _ if is_raw => FileKind::Raw,
            _ => FileKind::Other,
        }
    }
}

/// What the import knows about one photo of the folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub path: PathBuf,
    pub kind: FileKind,
    /// Stars set outside this app, 0..=5.
    pub rating: u8,
    pub label: ColorLabel,
    /// Read-only or locked in the file system.
    pub protected: bool,
}

/// Which candidates to import: everything that matches all four rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportFilter {
    /// Kinds left out.
    pub hidden_kinds: BTreeSet<FileKind>,
    /// Star counts left out (index 0 is "no rating").
    pub hidden_ratings: [bool; 6],
    /// Labels left out ([`ColorLabel::None`] is "no label").
    pub hidden_labels: Vec<ColorLabel>,
    pub skip_protected: bool,
}

impl Default for ImportFilter {
    fn default() -> Self {
        ImportFilter {
            hidden_kinds: BTreeSet::new(),
            hidden_ratings: [false; 6],
            hidden_labels: Vec::new(),
            skip_protected: false,
        }
    }
}

impl ImportFilter {
    pub fn matches(&self, c: &Candidate) -> bool {
        !self.hidden_kinds.contains(&c.kind)
            && !self.hidden_ratings[c.rating.min(5) as usize]
            && !self.hidden_labels.contains(&c.label)
            && !(self.skip_protected && c.protected)
    }

    /// Whether everything is let through.
    pub fn is_open(&self) -> bool {
        *self == ImportFilter::default()
    }

    pub fn toggle_kind(&mut self, kind: FileKind) {
        if !self.hidden_kinds.remove(&kind) {
            self.hidden_kinds.insert(kind);
        }
    }

    pub fn toggle_rating(&mut self, stars: u8) {
        let hidden = &mut self.hidden_ratings[stars.min(5) as usize];
        *hidden = !*hidden;
    }

    pub fn toggle_label(&mut self, label: ColorLabel) {
        if let Some(at) = self.hidden_labels.iter().position(|l| *l == label) {
            self.hidden_labels.remove(at);
        } else {
            self.hidden_labels.push(label);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(kind: FileKind, rating: u8, label: ColorLabel, protected: bool) -> Candidate {
        Candidate { path: PathBuf::from("a"), kind, rating, label, protected }
    }

    #[test]
    fn everything_passes_an_open_filter() {
        let filter = ImportFilter::default();
        assert!(filter.is_open());
        for c in [
            photo(FileKind::Raw, 0, ColorLabel::None, false),
            photo(FileKind::Jpeg, 5, ColorLabel::Purple, true),
        ] {
            assert!(filter.matches(&c));
        }
    }

    #[test]
    fn each_rule_narrows_the_import() {
        let mut filter = ImportFilter::default();
        filter.toggle_kind(FileKind::Jpeg);
        assert!(!filter.matches(&photo(FileKind::Jpeg, 3, ColorLabel::None, false)));
        assert!(filter.matches(&photo(FileKind::Raw, 3, ColorLabel::None, false)));

        filter.toggle_rating(0);
        assert!(!filter.matches(&photo(FileKind::Raw, 0, ColorLabel::None, false)));
        assert!(filter.matches(&photo(FileKind::Raw, 1, ColorLabel::None, false)));

        filter.toggle_label(ColorLabel::Red);
        assert!(!filter.matches(&photo(FileKind::Raw, 4, ColorLabel::Red, false)));
        assert!(filter.matches(&photo(FileKind::Raw, 4, ColorLabel::Blue, false)));

        filter.skip_protected = true;
        assert!(!filter.matches(&photo(FileKind::Raw, 4, ColorLabel::Blue, true)));
        assert!(!filter.is_open());
    }

    #[test]
    fn toggling_twice_puts_it_back() {
        let mut filter = ImportFilter::default();
        filter.toggle_kind(FileKind::Png);
        filter.toggle_rating(2);
        filter.toggle_label(ColorLabel::Green);
        assert!(!filter.is_open());
        filter.toggle_kind(FileKind::Png);
        filter.toggle_rating(2);
        filter.toggle_label(ColorLabel::Green);
        assert!(filter.is_open());
    }

    #[test]
    fn extensions_map_to_kinds() {
        assert_eq!(FileKind::of_extension("arw", true), FileKind::Raw);
        assert_eq!(FileKind::of_extension("dng", true), FileKind::Dng);
        assert_eq!(FileKind::of_extension("jpeg", false), FileKind::Jpeg);
        assert_eq!(FileKind::of_extension("webp", false), FileKind::Other);
    }
}
