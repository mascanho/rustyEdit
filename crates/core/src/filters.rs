//! Facet filters beyond flag and rating: colour label, camera, lens, file
//! type, year, ISO and focal-length bands, edited state, and text search.
//! Each facet is single-choice; all active facets must match.

use serde::{Deserialize, Serialize};

use crate::{asset::Asset, color_label::ColorLabel};

/// Sensitivity bands, since exact ISO values are too many to list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IsoBand {
    /// ISO 400 and below.
    Low,
    /// 401 to 1600.
    Medium,
    /// 1601 to 6400.
    High,
    /// Above 6400.
    Extreme,
}

impl IsoBand {
    pub const ALL: [IsoBand; 4] = [
        IsoBand::Low,
        IsoBand::Medium,
        IsoBand::High,
        IsoBand::Extreme,
    ];

    pub fn of(iso: u32) -> Option<Self> {
        match iso {
            0 => None,
            1..=400 => Some(IsoBand::Low),
            401..=1600 => Some(IsoBand::Medium),
            1601..=6400 => Some(IsoBand::High),
            _ => Some(IsoBand::Extreme),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            IsoBand::Low => "ISO \u{2264} 400",
            IsoBand::Medium => "ISO 401\u{2013}1600",
            IsoBand::High => "ISO 1601\u{2013}6400",
            IsoBand::Extreme => "ISO > 6400",
        }
    }
}

/// Focal-length bands (as marked on the lens, not 35mm-equivalent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FocalBand {
    /// Under 35 mm.
    Wide,
    /// 35 to 85 mm.
    Standard,
    /// Over 85 mm.
    Tele,
}

impl FocalBand {
    pub const ALL: [FocalBand; 3] = [FocalBand::Wide, FocalBand::Standard, FocalBand::Tele];

    pub fn of(focal_mm: f32) -> Option<Self> {
        if focal_mm <= 0.0 {
            None
        } else if focal_mm < 35.0 {
            Some(FocalBand::Wide)
        } else if focal_mm <= 85.0 {
            Some(FocalBand::Standard)
        } else {
            Some(FocalBand::Tele)
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            FocalBand::Wide => "Wide \u{00B7} under 35 mm",
            FocalBand::Standard => "Standard \u{00B7} 35\u{2013}85 mm",
            FocalBand::Tele => "Telephoto \u{00B7} over 85 mm",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterSet {
    /// `Some(ColorLabel::None)` means "no label".
    pub label: Option<ColorLabel>,
    pub camera: Option<String>,
    pub lens: Option<String>,
    /// Upper-case file extension, e.g. `"DNG"`.
    pub kind: Option<String>,
    pub year: Option<i32>,
    pub iso: Option<IsoBand>,
    pub focal: Option<FocalBand>,
    /// `Some(true)` = has Develop edits, `Some(false)` = untouched.
    pub edited: Option<bool>,
    /// `Some(true)` = only photos marked for deletion, `Some(false)` = only
    /// those that are not.
    pub marked: Option<bool>,
    /// One keyword that every visible photo must carry (matched
    /// case-insensitively).
    pub keyword: Option<String>,
    /// Free text; every word must appear in the file name or in the photo's
    /// metadata (camera, lens, file type, title, caption, creator, copyright,
    /// keywords).
    pub search: String,
}

impl FilterSet {
    /// How many facets are narrowing the view (search counts as one).
    pub fn active_count(&self) -> usize {
        [
            self.label.is_some(),
            self.camera.is_some(),
            self.lens.is_some(),
            self.kind.is_some(),
            self.year.is_some(),
            self.iso.is_some(),
            self.focal.is_some(),
            self.edited.is_some(),
            self.marked.is_some(),
            self.keyword.is_some(),
            !self.search.trim().is_empty(),
        ]
        .into_iter()
        .filter(|on| *on)
        .count()
    }

    pub fn is_active(&self) -> bool {
        self.active_count() > 0
    }

    /// Whether any facet other than text search is set.
    pub fn has_facets(&self) -> bool {
        self.active_count() - usize::from(!self.search.trim().is_empty()) > 0
    }

    pub fn matches(&self, asset: &Asset) -> bool {
        if let Some(label) = self.label {
            if asset.color_label != label {
                return false;
            }
        }
        if let Some(camera) = &self.camera {
            if &asset.meta.camera != camera {
                return false;
            }
        }
        if let Some(lens) = &self.lens {
            if &asset.meta.lens != lens {
                return false;
            }
        }
        if let Some(kind) = &self.kind {
            if !asset.kind().eq_ignore_ascii_case(kind) {
                return false;
            }
        }
        if let Some(year) = self.year {
            if asset.meta.year() != Some(year) {
                return false;
            }
        }
        if let Some(band) = self.iso {
            if IsoBand::of(asset.meta.iso) != Some(band) {
                return false;
            }
        }
        if let Some(band) = self.focal {
            if FocalBand::of(asset.meta.focal_mm) != Some(band) {
                return false;
            }
        }
        if let Some(edited) = self.edited {
            if asset.develop.is_edited() != edited {
                return false;
            }
        }
        if let Some(marked) = self.marked {
            if asset.marked_delete != marked {
                return false;
            }
        }
        if let Some(keyword) = &self.keyword {
            if !asset
                .keywords
                .iter()
                .any(|k| k.eq_ignore_ascii_case(keyword))
            {
                return false;
            }
        }
        let query = self.search.trim();
        if !query.is_empty() {
            let haystack = format!(
                "{} {} {} {} {} {} {} {} {}",
                asset.file_name(),
                asset.meta.camera,
                asset.meta.lens,
                asset.kind(),
                asset.title,
                asset.caption,
                asset.creator,
                asset.copyright,
                asset.keywords.join(" ")
            )
            .to_lowercase();
            if !query
                .to_lowercase()
                .split_whitespace()
                .all(|word| haystack.contains(word))
            {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn the_marked_for_deletion_filter_selects_only_marked_photos() {
        let plain = Asset::new(1, PathBuf::from("/photos/a.jpg"));
        let mut marked = Asset::new(2, PathBuf::from("/photos/b.jpg"));
        marked.marked_delete = true;

        let only_marked = FilterSet {
            marked: Some(true),
            ..Default::default()
        };
        assert!(only_marked.is_active());
        assert!(only_marked.matches(&marked));
        assert!(!only_marked.matches(&plain));

        let not_marked = FilterSet {
            marked: Some(false),
            ..Default::default()
        };
        assert!(not_marked.matches(&plain) && !not_marked.matches(&marked));
        assert!(FilterSet::default().matches(&marked));
    }

    #[test]
    fn marks_survive_a_state_round_trip() {
        let mut catalog = crate::Catalog::new(
            PathBuf::from("/photos"),
            vec![Asset::new(1, PathBuf::from("/photos/a.jpg"))],
        );
        let mut state = catalog.assets[0].state();
        state.marked_delete = true;
        catalog.restore_state(0, &state);
        assert!(catalog.assets[0].marked_delete);
        assert!(catalog.assets[0].state().marked_delete);
    }
}
