use std::{collections::HashSet, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{asset::Asset, color_label::ColorLabel, flag::Flag, rating::Rating};

/// Which subset of the catalog's assets should currently be visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FilterMode {
    #[default]
    All,
    Picked,
    Rejected,
    Unflagged,
}

impl FilterMode {
    pub fn label(&self) -> &'static str {
        match self {
            FilterMode::All => "All",
            FilterMode::Picked => "Picked",
            FilterMode::Rejected => "Rejected",
            FilterMode::Unflagged => "Unflagged",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            FilterMode::All => FilterMode::Picked,
            FilterMode::Picked => FilterMode::Rejected,
            FilterMode::Rejected => FilterMode::Unflagged,
            FilterMode::Unflagged => FilterMode::All,
        }
    }

    pub fn matches(&self, flag: Flag) -> bool {
        match self {
            FilterMode::All => true,
            FilterMode::Picked => flag == Flag::Picked,
            FilterMode::Rejected => flag == Flag::Rejected,
            FilterMode::Unflagged => flag == Flag::Unflagged,
        }
    }
}

/// In-memory model of "all the photos in one imported folder", plus the
/// culling/triage state layered on top of them. This is intentionally a
/// plain data structure with no GPUI dependency so it can be tested and
/// reused headlessly.
///
/// The filtered index and flag counts are cached and kept up to date
/// incrementally, because the UI reads them on every frame and recomputing
/// them is O(n) — too slow for catalogs with hundreds of thousands of
/// photos. Flags and the filter must therefore only be changed through
/// [`Catalog::set_flag`] and [`Catalog::set_filter`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    /// The first folder; kept for single-folder sidecars and as a label.
    pub folder: Option<PathBuf>,
    /// Every folder whose photos are in `assets` (empty for old sidecars).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<PathBuf>,
    pub assets: Vec<Asset>,
    #[serde(skip)]
    pub selected: usize,
    filter: FilterMode,
    /// When set, only photos at these paths are visible (an album view).
    #[serde(skip)]
    scope: Option<HashSet<PathBuf>>,
    /// Indices into `assets` that pass `filter`, ascending.
    #[serde(skip)]
    visible: Vec<usize>,
    #[serde(skip)]
    counts: CatalogCounts,
}

impl Catalog {
    pub fn new(folder: PathBuf, assets: Vec<Asset>) -> Self {
        let mut catalog = Self {
            folder: Some(folder.clone()),
            folders: vec![folder],
            assets,
            selected: 0,
            filter: FilterMode::All,
            scope: None,
            visible: Vec::new(),
            counts: CatalogCounts::default(),
        };
        catalog.rebuild_index();
        catalog
    }

    fn in_scope(scope: &Option<HashSet<PathBuf>>, asset: &Asset) -> bool {
        scope
            .as_ref()
            .map_or(true, |paths| paths.contains(&asset.path))
    }

    fn passes(&self, asset: &Asset) -> bool {
        self.filter.matches(asset.flag) && Self::in_scope(&self.scope, asset)
    }

    /// A catalog gathering photos from several folders. Asset ids must be
    /// their positions in `assets`.
    pub fn new_multi(folders: Vec<PathBuf>, assets: Vec<Asset>) -> Self {
        let mut catalog = Self::new(folders.first().cloned().unwrap_or_default(), assets);
        catalog.folder = folders.first().cloned();
        catalog.folders = folders;
        catalog
    }

    /// Whether `path` is one of the folders this catalog was built from.
    pub fn has_folder(&self, path: &std::path::Path) -> bool {
        self.folders.iter().any(|f| f == path)
            || (self.folders.is_empty() && self.folder.as_deref() == Some(path))
    }

    /// Recomputes the cached filtered index and counts from scratch.
    fn rebuild_index(&mut self) {
        let mut counts = CatalogCounts::default();
        for asset in &self.assets {
            counts.add(asset.flag);
        }
        self.counts = counts;
        self.visible = self
            .assets
            .iter()
            .enumerate()
            .filter(|(_, asset)| self.passes(asset))
            .map(|(ix, _)| ix)
            .collect();
    }

    pub fn filter(&self) -> FilterMode {
        self.filter
    }

    /// Indices into `self.assets` that pass the current filter, in display order.
    pub fn visible_indices(&self) -> &[usize] {
        &self.visible
    }

    /// Position of `index` within [`Self::visible_indices`], if it's visible.
    pub fn visible_position(&self, index: usize) -> Option<usize> {
        self.visible.binary_search(&index).ok()
    }

    pub fn selected_asset(&self) -> Option<&Asset> {
        self.assets.get(self.selected)
    }

    /// Moves the selection to the next/previous visible asset, wrapping
    /// within the current filter. Returns the newly selected absolute index.
    pub fn select_relative(&mut self, delta: i32) -> Option<usize> {
        if self.visible.is_empty() {
            return None;
        }
        let current_pos = self.visible_position(self.selected).unwrap_or(0);
        let len = self.visible.len() as i64;
        let next_pos = ((current_pos as i64 + delta as i64) % len + len) % len;
        self.selected = self.visible[next_pos as usize];
        Some(self.selected)
    }

    pub fn select(&mut self, index: usize) {
        if index < self.assets.len() {
            self.selected = index;
        }
    }

    pub fn set_flag(&mut self, index: usize, flag: Flag) {
        let Some(asset) = self.assets.get_mut(index) else {
            return;
        };
        let scoped = Self::in_scope(&self.scope, asset);
        let old = asset.flag;
        let new = if old == flag { Flag::Unflagged } else { flag };
        asset.flag = new;
        asset.record_change("Flag", old.label(), new.label());

        self.counts.remove(old);
        self.counts.add(new);
        match (
            scoped && self.filter.matches(old),
            scoped && self.filter.matches(new),
        ) {
            (true, false) => {
                if let Ok(pos) = self.visible.binary_search(&index) {
                    self.visible.remove(pos);
                }
            }
            (false, true) => {
                if let Err(pos) = self.visible.binary_search(&index) {
                    self.visible.insert(pos, index);
                }
            }
            _ => {}
        }
    }

    pub fn set_rating(&mut self, index: usize, rating: Rating) {
        if let Some(asset) = self.assets.get_mut(index) {
            let old = asset.rating;
            asset.rating = if old == rating {
                Rating::default()
            } else {
                rating
            };
            asset.record_change("Rating", stars(old), stars(asset.rating));
        }
    }

    /// Applies `label`, or clears it if the asset already has that label.
    pub fn set_color_label(&mut self, index: usize, label: ColorLabel) {
        if let Some(asset) = self.assets.get_mut(index) {
            let old = asset.color_label;
            asset.color_label = if old == label {
                ColorLabel::None
            } else {
                label
            };
            asset.record_change("Colour label", old.label(), asset.color_label.label());
        }
    }

    pub fn cycle_filter(&mut self) {
        self.set_filter(self.filter.next());
    }

    /// Switches the active filter, moving the selection to the first
    /// visible asset if the current one no longer matches.
    pub fn set_filter(&mut self, filter: FilterMode) {
        self.filter = filter;
        self.rebuild_index();
        if self.visible_position(self.selected).is_none() {
            if let Some(&first) = self.visible.first() {
                self.selected = first;
            }
        }
    }

    /// Restricts the visible photos to those at `paths` (an album), or
    /// lifts the restriction with `None`. Moves the selection onto a visible
    /// photo if the current one fell out of view.
    pub fn set_scope(&mut self, scope: Option<HashSet<PathBuf>>) {
        self.scope = scope;
        self.rebuild_index();
        if self.visible_position(self.selected).is_none() {
            if let Some(&first) = self.visible.first() {
                self.selected = first;
            }
        }
    }

    pub fn is_scoped(&self) -> bool {
        self.scope.is_some()
    }

    pub fn counts(&self) -> CatalogCounts {
        self.counts
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CatalogCounts {
    pub total: usize,
    pub picked: usize,
    pub rejected: usize,
    pub unflagged: usize,
}

impl CatalogCounts {
    fn bucket(&mut self, flag: Flag) -> &mut usize {
        match flag {
            Flag::Picked => &mut self.picked,
            Flag::Rejected => &mut self.rejected,
            Flag::Unflagged => &mut self.unflagged,
        }
    }

    fn add(&mut self, flag: Flag) {
        self.total += 1;
        *self.bucket(flag) += 1;
    }

    fn remove(&mut self, flag: Flag) {
        self.total -= 1;
        *self.bucket(flag) -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog(n: usize) -> Catalog {
        let assets = (0..n)
            .map(|i| Asset::new(i as u64, PathBuf::from(format!("{i}.jpg"))))
            .collect();
        Catalog::new(PathBuf::from("/photos"), assets)
    }

    /// The incrementally maintained index must always equal a from-scratch rebuild.
    fn assert_index_consistent(catalog: &Catalog) {
        let mut fresh = catalog.clone();
        fresh.rebuild_index();
        assert_eq!(catalog.visible, fresh.visible);
        assert_eq!(catalog.counts, fresh.counts);
    }

    #[test]
    fn flag_changes_keep_index_in_sync_under_every_filter() {
        for filter in [
            FilterMode::All,
            FilterMode::Picked,
            FilterMode::Rejected,
            FilterMode::Unflagged,
        ] {
            let mut c = catalog(10);
            c.set_filter(filter);
            for (ix, flag) in [
                (3, Flag::Picked),
                (7, Flag::Rejected),
                (3, Flag::Picked), // toggles back to unflagged
                (0, Flag::Picked),
                (9, Flag::Rejected),
                (7, Flag::Picked),
            ] {
                c.set_flag(ix, flag);
                assert_index_consistent(&c);
            }
        }
    }

    #[test]
    fn select_relative_wraps_within_filter() {
        let mut c = catalog(5);
        c.set_flag(1, Flag::Picked);
        c.set_flag(3, Flag::Picked);
        c.set_filter(FilterMode::Picked);
        assert_eq!(c.selected, 1);
        assert_eq!(c.select_relative(1), Some(3));
        assert_eq!(c.select_relative(1), Some(1));
        assert_eq!(c.select_relative(-1), Some(3));
    }
}

fn stars(rating: Rating) -> String {
    match rating.value() {
        0 => "None".to_string(),
        n => "\u{2605}".repeat(n as usize),
    }
}
