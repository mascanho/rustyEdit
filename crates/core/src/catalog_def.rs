use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    albums::Albums,
    collections::{default_keyword_sets, KeywordSet, SmartCollections},
};

/// A named catalog: the folders it gathers photos from, plus its albums.
/// Per-photo state (flags, ratings, edits, history) stays in each folder's
/// own sidecar, so a folder keeps its work no matter which catalogs include
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogDef {
    /// Stable file-system-safe identifier; never changes after creation.
    pub id: String,
    pub name: String,
    pub folders: Vec<PathBuf>,
    #[serde(default)]
    pub albums: Albums,
    #[serde(default)]
    pub smart_collections: SmartCollections,
    /// Named groups of keywords offered in the Keywords panel.
    #[serde(default = "default_keyword_sets")]
    pub keyword_sets: Vec<KeywordSet>,
    /// Photos removed from this catalog with "Remove from Catalog". They stay
    /// on disk, but are skipped on every subsequent scan so they don't
    /// reappear until the user restores them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded: Vec<PathBuf>,
}

impl CatalogDef {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            folders: Vec::new(),
            albums: Albums::default(),
            smart_collections: SmartCollections::default(),
            keyword_sets: default_keyword_sets(),
            excluded: Vec::new(),
        }
    }

    /// Adds `path` to the catalog's exclusion list (idempotent).
    pub fn exclude(&mut self, path: &Path) {
        if !self.is_excluded(path) {
            self.excluded.push(path.to_path_buf());
        }
    }

    pub fn is_excluded(&self, path: &Path) -> bool {
        self.excluded.iter().any(|excluded| excluded == path)
    }

    pub fn restore_excluded(&mut self) {
        self.excluded.clear();
    }

    /// Adds `folder`. A folder already covered by one of the catalog's
    /// folders is ignored; adding a parent of existing folders absorbs them.
    /// Returns whether the set of folders changed.
    pub fn add_folder(&mut self, folder: &Path) -> bool {
        if self
            .folders
            .iter()
            .any(|existing| folder.starts_with(existing))
        {
            return false;
        }
        self.folders
            .retain(|existing| !existing.starts_with(folder));
        self.folders.push(folder.to_path_buf());
        true
    }

    pub fn remove_folder(&mut self, folder: &Path) {
        self.folders.retain(|existing| existing != folder);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_folders_collapse() {
        let mut def = CatalogDef::new("c", "C");
        assert!(def.add_folder(Path::new("/photos/2024/may")));
        assert!(!def.add_folder(Path::new("/photos/2024/may/day1")));
        assert!(def.add_folder(Path::new("/photos/2023")));
        // A parent absorbs the folders beneath it.
        assert!(def.add_folder(Path::new("/photos")));
        assert_eq!(def.folders, vec![PathBuf::from("/photos")]);
        def.remove_folder(Path::new("/photos"));
        assert!(def.folders.is_empty());
    }

    #[test]
    fn excluded_photos_are_remembered_once() {
        let mut def = CatalogDef::new("c", "C");
        let path = Path::new("/photos/hidden.jpg");
        assert!(!def.is_excluded(path));
        def.exclude(path);
        def.exclude(path);
        assert_eq!(def.excluded.len(), 1);
        assert!(def.is_excluded(path));
        def.restore_excluded();
        assert!(!def.is_excluded(path));
    }
}
