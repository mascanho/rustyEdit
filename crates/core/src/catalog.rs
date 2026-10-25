use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};

use crate::{
    asset::{Asset, AssetState},
    color_label::ColorLabel,
    filters::FilterSet,
    flag::Flag,
    rating::Rating,
};

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

/// What the visible photos are ordered by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SortKey {
    /// The order photos were scanned in: folder by folder, then by path.
    #[default]
    Folder,
    FileName,
    CaptureDate,
    Rating,
    Flag,
}

impl SortKey {
    pub const ALL: [SortKey; 5] = [
        SortKey::Folder,
        SortKey::FileName,
        SortKey::CaptureDate,
        SortKey::Rating,
        SortKey::Flag,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            SortKey::Folder => "Folder order",
            SortKey::FileName => "File name",
            SortKey::CaptureDate => "Capture date",
            SortKey::Rating => "Rating",
            SortKey::Flag => "Flag",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SortOrder {
    pub key: SortKey,
    pub descending: bool,
}

/// Narrows the catalog by star rating, on top of the flag filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RatingFilter {
    #[default]
    Any,
    /// No stars.
    Unrated,
    /// This many stars or more.
    AtLeast(u8),
}

impl RatingFilter {
    pub fn matches(&self, rating: Rating) -> bool {
        match self {
            RatingFilter::Any => true,
            RatingFilter::Unrated => rating.value() == 0,
            RatingFilter::AtLeast(n) => rating.value() >= *n,
        }
    }

    pub fn label(&self) -> String {
        match self {
            RatingFilter::Any => "Any rating".into(),
            RatingFilter::Unrated => "Unrated".into(),
            RatingFilter::AtLeast(5) => "\u{2605}\u{2605}\u{2605}\u{2605}\u{2605}".into(),
            RatingFilter::AtLeast(n) => format!("{}+", "\u{2605}".repeat(*n as usize)),
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
/// How a RAW and the JPEG shot with it (same name, same folder) are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PairView {
    /// Both, as two photos, told apart by their RAW / JPG badges.
    #[default]
    Both,
    /// One tile per shot, the RAW as its cover; the JPEG tucks underneath
    /// until the stack is opened.
    Stack,
    /// One photo per shot: the RAW, with its JPEG beside it unseen.
    Raw,
    /// One photo per shot: the JPEG.
    Jpeg,
}

impl PairView {
    pub const ALL: [PairView; 4] = [PairView::Both, PairView::Stack, PairView::Raw, PairView::Jpeg];

    pub fn label(&self) -> &'static str {
        match self {
            PairView::Both => "Both",
            PairView::Stack => "Stacked",
            PairView::Raw => "RAW",
            PairView::Jpeg => "JPEG",
        }
    }
}

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
    #[serde(skip)]
    rating_filter: RatingFilter,
    #[serde(skip)]
    filters: FilterSet,
    #[serde(skip)]
    sort: SortOrder,
    /// `rank[i]` is where asset `i` falls in the sort order; empty means
    /// plain index order. Fixed until the sort changes, so photos don't jump
    /// around while you cull.
    #[serde(skip)]
    rank: Vec<u32>,
    /// When set, only photos at these paths are visible (an album view).
    #[serde(skip)]
    scope: Option<HashSet<PathBuf>>,
    /// Stacks the user has collapsed; only their representative is visible.
    #[serde(skip)]
    collapsed_stacks: HashSet<u64>,
    /// Indices into `assets` that pass `filter`, ascending.
    #[serde(skip)]
    visible: Vec<usize>,
    #[serde(skip)]
    counts: CatalogCounts,
    /// Which of a RAW + JPEG pair shows.
    #[serde(skip)]
    pair_view: PairView,
    /// For a photo that is half of a RAW + JPEG pair, the other half.
    #[serde(skip)]
    companions: Vec<Option<u32>>,
    /// RAWs whose stacked pair is open, showing the JPEG too.
    #[serde(skip)]
    open_pairs: HashSet<u32>,
}

impl Catalog {
    pub fn new(folder: PathBuf, assets: Vec<Asset>) -> Self {
        let mut catalog = Self {
            folder: Some(folder.clone()),
            folders: vec![folder],
            assets,
            selected: 0,
            filter: FilterMode::All,
            rating_filter: RatingFilter::Any,
            filters: FilterSet::default(),
            sort: SortOrder::default(),
            rank: Vec::new(),
            scope: None,
            collapsed_stacks: HashSet::new(),
            visible: Vec::new(),
            counts: CatalogCounts::default(),
            pair_view: PairView::Both,
            companions: Vec::new(),
            open_pairs: HashSet::new(),
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
        !asset.missing
            && self.filter.matches(asset.flag)
            && self.rating_filter.matches(asset.rating)
            && self.filters.matches(asset)
            && Self::in_scope(&self.scope, asset)
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
            counts.total += 1;
            if asset.missing {
                counts.missing += 1;
            } else {
                *counts.bucket(asset.flag) += 1;
            }
        }
        self.counts = counts;

        let mut passing: Vec<usize> = Vec::new();
        if self.collapsed_stacks.is_empty() {
            passing.extend(
                self.assets
                    .iter()
                    .enumerate()
                    .filter(|(_, asset)| self.passes(asset))
                    .map(|(ix, _)| ix),
            );
        } else {
            // A collapsed stack shows one photo: its cover if that passes the
            // filters, otherwise its first passing member.
            let mut chosen: HashMap<u64, (usize, bool)> = HashMap::new();
            for (ix, asset) in self.assets.iter().enumerate() {
                if !self.passes(asset) {
                    continue;
                }
                match asset.stack_id {
                    Some(sid) if self.collapsed_stacks.contains(&sid) => {
                        match chosen.get_mut(&sid) {
                            None => {
                                chosen.insert(sid, (ix, asset.stack_cover));
                            }
                            Some((current, current_is_cover)) => {
                                if !*current_is_cover && asset.stack_cover {
                                    *current = ix;
                                    *current_is_cover = true;
                                }
                            }
                        }
                    }
                    _ => passing.push(ix),
                }
            }
            passing.extend(chosen.into_values().map(|(ix, _)| ix));
        }

        // One photo per RAW + JPEG shot, when asked for.
        self.companions = find_companions(&self.assets);
        if self.pair_view != PairView::Both {
            let (view, companions, assets, open) =
                (self.pair_view, &self.companions, &self.assets, &self.open_pairs);
            passing.retain(|&ix| {
                if companions.get(ix).copied().flatten().is_none() {
                    return true;
                }
                let raw = crate::asset::is_raw_path(&assets[ix].path);
                match view {
                    PairView::Raw => raw,
                    PairView::Jpeg => !raw,
                    PairView::Stack => {
                        raw || companions[ix].is_some_and(|other| open.contains(&other))
                    }
                    PairView::Both => true,
                }
            });
        }

        if self.rank.is_empty() {
            passing.sort_unstable();
        } else {
            let rank = &self.rank;
            passing.sort_unstable_by_key(|&ix| rank[ix]);
        }
        self.visible = passing;
    }

    /// The other half of `index`'s RAW + JPEG pair, if it is in one.
    pub fn companion_of(&self, index: usize) -> Option<usize> {
        self.companions.get(index).copied().flatten().map(|c| c as usize)
    }

    /// Whether any photo is half of a RAW + JPEG pair.
    /// "RAW" or "JPG" on each half of a pair, so the two are told apart.
    pub fn pair_tag(&self, ix: usize) -> Option<&'static str> {
        self.companion_of(ix)?;
        let raw = crate::asset::is_raw_path(&self.assets[ix].path);
        if self.pair_view == PairView::Stack && raw && !self.open_pairs.contains(&(ix as u32)) {
            return Some("RAW+JPG");
        }
        Some(if crate::asset::is_raw_path(&self.assets[ix].path) {
            "RAW"
        } else {
            "JPG"
        })
    }

    /// Whether the stacked pair with RAW `ix` is open.
    pub fn is_pair_open(&self, ix: usize) -> bool {
        self.open_pairs.contains(&(ix as u32))
    }

    /// Opens or closes the stacked pair whose RAW is `ix`.
    pub fn toggle_pair_open(&mut self, ix: usize) {
        if !self.open_pairs.remove(&(ix as u32)) {
            self.open_pairs.insert(ix as u32);
        }
        self.rebuild_index();
    }

    /// Whether `ix` is a RAW with a JPEG stacked under it right now.
    pub fn is_pair_cover(&self, ix: usize) -> bool {
        self.pair_view == PairView::Stack
            && self.companion_of(ix).is_some()
            && crate::asset::is_raw_path(&self.assets[ix].path)
    }

    pub fn has_pairs(&self) -> bool {
        self.companions.iter().any(Option::is_some)
    }

    pub fn pair_view(&self) -> PairView {
        self.pair_view
    }

    /// Chooses which of each RAW + JPEG pair shows.
    pub fn set_pair_view(&mut self, view: PairView) {
        if self.pair_view != view {
            self.pair_view = view;
            self.rebuild_index();
        }
    }

    /// Rebuilds the visible index after a bulk edit that can affect filters
    /// or stack representatives (keywords, metadata, stacking).
    pub fn refresh(&mut self) {
        self.rebuild_index();
        if self.visible_position(self.selected).is_none() {
            if let Some(&first) = self.visible.first() {
                self.selected = first;
            }
        }
    }

    /// Where asset `ix` sorts; the key `visible` is ordered by.
    fn rank_of(&self, ix: usize) -> u32 {
        self.rank.get(ix).copied().unwrap_or(ix as u32)
    }

    pub fn sort(&self) -> SortOrder {
        self.sort
    }

    /// Re-orders the visible photos. Cheap for the default order.
    pub fn set_sort(&mut self, order: SortOrder) {
        self.sort = order;
        let n = self.assets.len();
        let mut indices: Vec<usize> = (0..n).collect();
        let anchors = self.copy_anchors();
        let has_copies = anchors.iter().enumerate().any(|(ix, a)| a.0 != ix);
        let flag_rank = |flag: Flag| match flag {
            Flag::Picked => 0,
            Flag::Unflagged => 1,
            Flag::Rejected => 2,
        };
        let names: Vec<String> = if order.key == SortKey::FileName {
            self.assets
                .iter()
                .map(|a| a.file_name().to_lowercase())
                .collect()
        } else {
            Vec::new()
        };
        let primary = |a: usize, b: usize| -> std::cmp::Ordering {
            let (x, y) = (&self.assets[a], &self.assets[b]);
            match order.key {
                SortKey::Folder => anchors[a].0.cmp(&anchors[b].0),
                SortKey::FileName => names[a].cmp(&names[b]),
                SortKey::CaptureDate => x.meta.captured.cmp(&y.meta.captured),
                SortKey::Rating => x.rating.value().cmp(&y.rating.value()),
                SortKey::Flag => flag_rank(x.flag).cmp(&flag_rank(y.flag)),
            }
        };
        // Ties keep catalog order, with each virtual copy right after its
        // original, whichever way the key runs.
        indices.sort_by(|&a, &b| {
            let key = primary(a, b);
            let key = if order.descending { key.reverse() } else { key };
            key.then(anchors[a].cmp(&anchors[b]))
        });
        let identity = order.key == SortKey::Folder && !order.descending && !has_copies;
        self.rank = if identity {
            Vec::new()
        } else {
            let mut rank = vec![0u32; n];
            for (pos, ix) in indices.into_iter().enumerate() {
                rank[ix] = pos as u32;
            }
            rank
        };
        self.rebuild_index();
    }

    pub fn filters(&self) -> &FilterSet {
        &self.filters
    }

    /// Replaces the facet filters and text search, moving the selection
    /// onto a visible photo if the current one dropped out.
    pub fn set_filters(&mut self, filters: FilterSet) {
        self.filters = filters;
        self.rebuild_index();
        if self.visible_position(self.selected).is_none() {
            if let Some(&first) = self.visible.first() {
                self.selected = first;
            }
        }
    }

    pub fn rating_filter(&self) -> RatingFilter {
        self.rating_filter
    }

    /// Adds or removes `index` from the visible list to match whether it
    /// currently passes every filter.
    fn sync_visibility(&mut self, index: usize) {
        if !self.collapsed_stacks.is_empty() {
            // Which member represents a collapsed stack can change with the
            // edit, so rebuild rather than adjusting one entry.
            self.rebuild_index();
            return;
        }
        let Some(asset) = self.assets.get(index) else {
            return;
        };
        let passes = self.passes(asset);
        let key = self.rank_of(index);
        let found = self
            .visible
            .binary_search_by_key(&key, |&ix| self.rank_of(ix));
        match (passes, found) {
            (true, Err(pos)) => self.visible.insert(pos, index),
            (false, Ok(pos)) => {
                self.visible.remove(pos);
            }
            _ => {}
        }
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
        self.visible
            .binary_search_by_key(&self.rank_of(index), |&ix| self.rank_of(ix))
            .ok()
    }

    pub fn selected_asset(&self) -> Option<&Asset> {
        self.assets
            .get(self.selected)
            .filter(|asset| !asset.missing)
    }

    /// Indices of photos whose file is missing from disk.
    pub fn missing_indices(&self) -> Vec<usize> {
        self.assets
            .iter()
            .enumerate()
            .filter(|(_, asset)| asset.missing)
            .map(|(ix, _)| ix)
            .collect()
    }

    /// Drops every photo at `paths` from the catalog, renumbering the
    /// remaining assets so ids stay equal to their positions. Returns how
    /// many were removed.
    pub fn remove_assets(&mut self, paths: &HashSet<PathBuf>) -> usize {
        if paths.is_empty() {
            return 0;
        }
        let moved = self.remove_where(|asset| paths.contains(&asset.path));
        moved.iter().filter(|new| new.is_none()).count()
    }

    /// Drops every photo `remove` picks, renumbering the rest so ids stay
    /// equal to their positions. Returns where each old index went (`None`
    /// for removed photos), for anything that holds on to indices.
    pub fn remove_where(&mut self, remove: impl Fn(&Asset) -> bool) -> Vec<Option<usize>> {
        let mut moved = Vec::with_capacity(self.assets.len());
        let mut next = 0;
        for asset in &self.assets {
            if remove(asset) {
                moved.push(None);
            } else {
                moved.push(Some(next));
                next += 1;
            }
        }
        if next == self.assets.len() {
            return moved;
        }
        let selected = moved.get(self.selected).copied().flatten();
        self.assets.retain(|asset| !remove(asset));
        for (ix, asset) in self.assets.iter_mut().enumerate() {
            asset.id = ix as u64;
        }
        let order = self.sort;
        self.set_sort(order);
        self.selected = selected.unwrap_or(0);
        if self.assets.is_empty() {
            self.selected = 0;
        } else if self.visible_position(self.selected).is_none() {
            self.selected = self.visible.first().copied().unwrap_or(0);
        }
        moved
    }

    // --- Virtual copies ------------------------------------------------------

    /// For each photo, the index of its original and its place among that
    /// original's copies (0 for the original itself). Photos without copies
    /// are their own anchor.
    fn copy_anchors(&self) -> Vec<(usize, u32)> {
        let mut anchors: Vec<(usize, u32)> = (0..self.assets.len()).map(|ix| (ix, 0)).collect();
        if !self.assets.iter().any(Asset::is_virtual_copy) {
            return anchors;
        }
        let mut masters: HashMap<&std::path::Path, usize> = HashMap::new();
        for (ix, asset) in self.assets.iter().enumerate() {
            if !asset.is_virtual_copy() {
                masters.entry(asset.path.as_path()).or_insert(ix);
            }
        }
        let mut copies_seen: HashMap<usize, u32> = HashMap::new();
        for (ix, asset) in self.assets.iter().enumerate() {
            if !asset.is_virtual_copy() {
                continue;
            }
            if let Some(&master) = masters.get(asset.path.as_path()) {
                let seen = copies_seen.entry(master).or_insert(0);
                *seen += 1;
                anchors[ix] = (master, *seen);
            }
        }
        anchors
    }

    /// The original photo of `index`'s file (itself, if it isn't a copy or
    /// its original is no longer in the catalog).
    pub fn master_of(&self, index: usize) -> usize {
        let Some(asset) = self.assets.get(index) else {
            return index;
        };
        if !asset.is_virtual_copy() {
            return index;
        }
        self.assets
            .iter()
            .position(|a| !a.is_virtual_copy() && a.path == asset.path)
            .unwrap_or(index)
    }

    /// Adds a virtual copy of photo `index`: a new entry for the same file
    /// starting with its edits, flags and metadata, named "Copy N". The
    /// original and its copies are stacked together (and the stack
    /// expanded). Returns the copy's index, which is the end of the list,
    /// so no existing index changes.
    pub fn add_virtual_copy(&mut self, index: usize) -> Option<usize> {
        let source = self.assets.get(index)?;
        if source.missing {
            return None;
        }
        let path = source.path.clone();
        let number = self
            .assets
            .iter()
            .filter(|a| a.path == path)
            .filter_map(|a| a.copy_name.as_deref()?.strip_prefix("Copy ")?.parse::<u32>().ok())
            .max()
            .unwrap_or(0)
            + 1;

        let mut copy = source.clone();
        let new_index = self.assets.len();
        copy.id = new_index as u64;
        copy.copy_name = Some(format!("Copy {number}"));
        copy.snapshots.clear();
        copy.history.clear();
        let from = source.display_name();
        copy.record_develop_change("Virtual copy", from, format!("Copy {number}"));

        let master = self.master_of(index);
        let stack = match self.assets[master].stack_id {
            Some(id) => id,
            None => {
                let id = self.max_stack_id() + 1;
                self.assets[master].stack_id = Some(id);
                self.assets[master].stack_cover = true;
                id
            }
        };
        copy.stack_id = Some(stack);
        copy.stack_cover = false;
        self.assets.push(copy);
        self.collapsed_stacks.remove(&stack);

        let order = self.sort;
        self.set_sort(order);
        self.selected = new_index;
        Some(new_index)
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

    /// Applies `flag`, or clears it if the asset already has that flag.
    pub fn set_flag(&mut self, index: usize, flag: Flag) {
        let Some(asset) = self.assets.get(index) else {
            return;
        };
        let new = if asset.flag == flag {
            Flag::Unflagged
        } else {
            flag
        };
        self.assign_flag(index, new);
    }

    /// Sets the flag exactly, keeping counts and the visible index in sync.
    pub fn assign_flag(&mut self, index: usize, new: Flag) {
        let Some(asset) = self.assets.get_mut(index) else {
            return;
        };
        let old = asset.flag;
        if old == new {
            return;
        }
        asset.flag = new;
        asset.record_change("Flag", old.label(), new.label());

        self.counts.remove(old);
        self.counts.add(new);
        self.sync_visibility(index);
    }

    pub fn set_rating(&mut self, index: usize, rating: Rating) {
        let Some(asset) = self.assets.get(index) else {
            return;
        };
        let new = if asset.rating == rating {
            Rating::default()
        } else {
            rating
        };
        self.assign_rating(index, new);
    }

    pub fn assign_rating(&mut self, index: usize, new: Rating) {
        if let Some(asset) = self.assets.get_mut(index) {
            let old = asset.rating;
            asset.rating = new;
            asset.record_change("Rating", stars(old), stars(new));
        }
        self.sync_visibility(index);
    }

    /// Applies `label`, or clears it if the asset already has that label.
    pub fn set_color_label(&mut self, index: usize, label: ColorLabel) {
        let Some(asset) = self.assets.get(index) else {
            return;
        };
        let new = if asset.color_label == label {
            ColorLabel::None
        } else {
            label
        };
        self.assign_color_label(index, new);
    }

    pub fn assign_color_label(&mut self, index: usize, new: ColorLabel) {
        if let Some(asset) = self.assets.get_mut(index) {
            let old = asset.color_label;
            asset.color_label = new;
            asset.record_change("Colour label", old.label(), new.label());
        }
    }

    /// Puts an asset's editable state back exactly as captured (for undo and
    /// redo), keeping the derived indexes consistent.
    pub fn restore_state(&mut self, index: usize, state: &AssetState) {
        self.assign_flag(index, state.flag);
        self.assign_rating(index, state.rating);
        self.assign_color_label(index, state.color_label);
        if let Some(asset) = self.assets.get_mut(index) {
            asset.develop = state.develop;
            asset.title = state.title.clone();
            asset.caption = state.caption.clone();
            asset.creator = state.creator.clone();
            asset.copyright = state.copyright.clone();
            asset.keywords = state.keywords.clone();
            asset.stack_id = state.stack_id;
            asset.stack_cover = state.stack_cover;
            asset.applied_preset = state.applied_preset.clone();
            asset.marked_delete = state.marked_delete;
        }
        self.refresh();
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

    /// Filters by star rating, moving the selection onto a visible photo if
    /// the current one no longer matches.
    pub fn set_rating_filter(&mut self, filter: RatingFilter) {
        self.rating_filter = filter;
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

    // --- Stacks -------------------------------------------------------------

    /// Every asset that belongs to stack `id`, in catalog order.
    pub fn stack_members(&self, id: u64) -> Vec<usize> {
        self.assets
            .iter()
            .enumerate()
            .filter(|(_, asset)| asset.stack_id == Some(id))
            .map(|(ix, _)| ix)
            .collect()
    }

    pub fn stack_count(&self, id: u64) -> usize {
        self.assets
            .iter()
            .filter(|asset| asset.stack_id == Some(id))
            .count()
    }

    pub fn is_stack_collapsed(&self, id: u64) -> bool {
        self.collapsed_stacks.contains(&id)
    }

    /// Collapses or expands stack `id`, rebuilding the visible list.
    pub fn toggle_stack_collapsed(&mut self, id: u64) {
        if !self.collapsed_stacks.remove(&id) {
            self.collapsed_stacks.insert(id);
        }
        self.refresh();
    }

    /// Collapses an expanded stack and expands a collapsed one. Returns the
    /// new collapsed state.
    pub fn set_stack_collapsed(&mut self, id: u64, collapsed: bool) {
        let changed = if collapsed {
            self.collapsed_stacks.insert(id)
        } else {
            self.collapsed_stacks.remove(&id)
        };
        if changed {
            self.refresh();
        }
    }

    /// The highest stack id in use, or 0 when there are no stacks.
    pub fn max_stack_id(&self) -> u64 {
        self.assets
            .iter()
            .filter_map(|asset| asset.stack_id)
            .max()
            .unwrap_or(0)
    }
}

/// Pairs each RAW with the JPEG (or other plain image) shot with it: the
/// same name, in the same folder, with exactly one RAW among them.
fn find_companions(assets: &[Asset]) -> Vec<Option<u32>> {
    let mut groups: HashMap<(PathBuf, String), Vec<usize>> = HashMap::new();
    for (ix, asset) in assets.iter().enumerate() {
        if asset.missing || asset.is_virtual_copy() {
            continue;
        }
        let (Some(parent), Some(stem)) = (asset.path.parent(), asset.path.file_stem()) else {
            continue;
        };
        groups
            .entry((parent.to_path_buf(), stem.to_string_lossy().to_lowercase()))
            .or_default()
            .push(ix);
    }
    let mut out = vec![None; assets.len()];
    for members in groups.values().filter(|m| m.len() >= 2) {
        let (raws, others): (Vec<usize>, Vec<usize>) = members
            .iter()
            .partition(|&&ix| crate::asset::is_raw_path(&assets[ix].path));
        if let ([raw], [other, ..]) = (raws.as_slice(), others.as_slice()) {
            out[*raw] = Some(*other as u32);
            for &o in &others {
                out[o] = Some(*raw as u32);
            }
        }
    }
    out
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CatalogCounts {
    pub total: usize,
    /// Photos whose files could not be found during the last scan.
    pub missing: usize,
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

    fn shot(names: &[&str]) -> Catalog {
        let assets = names
            .iter()
            .enumerate()
            .map(|(i, n)| Asset::new(i as u64, PathBuf::from(format!("/p/{n}"))))
            .collect();
        Catalog::new(PathBuf::from("/p"), assets)
    }

    #[test]
    fn a_raw_and_its_jpeg_are_a_pair_and_one_half_can_be_shown() {
        let mut c = shot(&["A.RAF", "A.JPG", "B.RAF", "C.JPG"]);
        // Both: everything shows.
        assert_eq!(c.visible_indices(), &[0, 1, 2, 3]);
        assert!(c.has_pairs());
        assert_eq!((c.companion_of(0), c.companion_of(1)), (Some(1), Some(0)));
        assert_eq!((c.companion_of(2), c.companion_of(3)), (None, None));
        // RAW: the JPEG half of the pair hides; singles stay.
        c.set_pair_view(PairView::Raw);
        assert_eq!(c.visible_indices(), &[0, 2, 3]);
        // JPEG: the RAW half hides.
        c.set_pair_view(PairView::Jpeg);
        assert_eq!(c.visible_indices(), &[1, 2, 3]);
    }

    #[test]
    fn stacked_pairs_show_the_raw_until_opened() {
        let mut c = shot(&["A.RAF", "A.JPG", "B.RAF"]);
        c.set_pair_view(PairView::Stack);
        assert_eq!(c.visible_indices(), &[0, 2]);
        assert_eq!(c.pair_tag(0), Some("RAW+JPG"));
        assert!(c.is_pair_cover(0) && !c.is_pair_cover(2));
        c.toggle_pair_open(0);
        assert_eq!(c.visible_indices(), &[0, 1, 2]);
        assert_eq!((c.pair_tag(0), c.pair_tag(1)), (Some("RAW"), Some("JPG")));
        c.toggle_pair_open(0);
        assert_eq!(c.visible_indices(), &[0, 2]);
    }

    #[test]
    fn names_match_without_regard_to_case_but_not_across_folders() {
        let mut c = Catalog::new(
            PathBuf::from("/p"),
            vec![
                Asset::new(0, PathBuf::from("/p/img_1.cr3")),
                Asset::new(1, PathBuf::from("/p/IMG_1.jpg")),
                Asset::new(2, PathBuf::from("/q/img_1.cr3")),
            ],
        );
        c.set_pair_view(PairView::Raw);
        assert_eq!(c.companion_of(0), Some(1));
        assert_eq!(c.companion_of(2), None);
        assert_eq!(c.visible_indices(), &[0, 2]);
    }

    #[test]
    fn two_raws_or_two_jpegs_with_a_name_are_not_a_pair() {
        let c = shot(&["A.RAF", "A.ARW", "B.JPG", "B.png"]);
        assert!(!c.has_pairs());
    }

    #[test]
    fn virtual_copies_sit_after_their_original_and_stack_with_it() {
        let mut c = catalog(3);
        let copy = c.add_virtual_copy(0).unwrap();
        assert_eq!(copy, 3);
        assert_eq!(c.assets[3].copy_name.as_deref(), Some("Copy 1"));
        assert_eq!(c.assets[3].path, c.assets[0].path);
        assert_eq!(c.selected, 3);
        // Folder order: right after the original, nothing else moved.
        assert_eq!(c.visible_indices(), &[0, 3, 1, 2]);
        let stack = c.assets[0].stack_id.unwrap();
        assert_eq!(c.assets[3].stack_id, Some(stack));
        assert!(c.assets[0].stack_cover && !c.assets[3].stack_cover);
        assert_index_consistent(&c);

        // A copy of the copy is numbered on and joins the same stack.
        let second = c.add_virtual_copy(3).unwrap();
        assert_eq!(c.assets[second].copy_name.as_deref(), Some("Copy 2"));
        assert_eq!(c.assets[second].stack_id, Some(stack));
        assert_eq!(c.visible_indices(), &[0, 3, 4, 1, 2]);
        assert_eq!(c.master_of(4), 0);

        // Descending keeps copies after their original too.
        c.set_sort(SortOrder {
            key: SortKey::Folder,
            descending: true,
        });
        assert_eq!(c.visible_indices(), &[2, 1, 0, 3, 4]);
    }

    #[test]
    fn removing_a_copy_keeps_the_original_and_maps_indices() {
        let mut c = catalog(3);
        c.add_virtual_copy(1).unwrap();
        c.select(2);
        let moved = c.remove_where(|a| a.is_virtual_copy());
        assert_eq!(moved, vec![Some(0), Some(1), Some(2), None]);
        assert_eq!(c.assets.len(), 3);
        assert!(c.assets.iter().all(|a| !a.is_virtual_copy()));
        assert_eq!(c.selected, 2);
        assert_index_consistent(&c);
    }

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
    fn sorting_reorders_visible_and_survives_edits() {
        let mut c = catalog(4);
        c.assign_rating(0, Rating::new(1));
        c.assign_rating(1, Rating::new(5));
        c.assign_rating(2, Rating::new(3));
        c.set_sort(SortOrder {
            key: SortKey::Rating,
            descending: true,
        });
        assert_eq!(c.visible_indices(), &[1, 2, 0, 3]);
        assert_eq!(c.visible_position(2), Some(1));
        assert_index_consistent(&c);

        // Editing keeps the order stable (no jumping), and filtering keeps
        // the visible list sorted.
        c.assign_rating(3, Rating::new(4));
        assert_eq!(c.visible_indices(), &[1, 2, 0, 3]);
        c.set_rating_filter(RatingFilter::AtLeast(3));
        assert_eq!(c.visible_indices(), &[1, 2, 3]);
        c.assign_rating(0, Rating::new(5));
        assert_eq!(c.visible_indices(), &[1, 2, 0, 3]);

        c.set_sort(SortOrder::default());
        assert_eq!(c.visible_indices(), &[0, 1, 2, 3]);
    }

    #[test]
    fn facet_and_search_filters_combine() {
        let mut c = catalog(3);
        c.assets[0].meta.camera = "SONY ILCE-7M2".into();
        c.assets[1].meta.camera = "SONY ILCE-7M2".into();
        c.assets[2].meta.camera = "Canon EOS R".into();
        c.assets[1].color_label = ColorLabel::Red;

        let mut f = FilterSet::default();
        f.camera = Some("SONY ILCE-7M2".into());
        c.set_filters(f.clone());
        assert_eq!(c.visible_indices(), &[0, 1]);

        f.label = Some(ColorLabel::Red);
        c.set_filters(f.clone());
        assert_eq!(c.visible_indices(), &[1]);

        f = FilterSet {
            search: "canon 2.jpg".into(),
            ..Default::default()
        };
        c.set_filters(f);
        assert_eq!(c.visible_indices(), &[2]);
    }

    #[test]
    fn year_from_timestamp() {
        let meta = crate::asset::PhotoMeta {
            captured: 1_491_360_260,
            ..Default::default()
        }; // 2017-04-05
        assert_eq!(meta.year(), Some(2017));
        assert_eq!(crate::asset::PhotoMeta::default().year(), None);
    }

    #[test]
    fn rating_filter_tracks_rating_changes() {
        let mut c = catalog(4);
        c.assign_rating(1, Rating::new(4));
        c.assign_rating(2, Rating::new(2));
        c.set_rating_filter(RatingFilter::AtLeast(3));
        assert_eq!(c.visible_indices(), &[1]);
        assert_index_consistent(&c);

        // Rating a hidden photo up brings it into view; down removes it.
        c.assign_rating(2, Rating::new(5));
        assert_eq!(c.visible_indices(), &[1, 2]);
        c.assign_rating(1, Rating::new(1));
        assert_eq!(c.visible_indices(), &[2]);
        assert_index_consistent(&c);

        c.set_rating_filter(RatingFilter::Unrated);
        assert_eq!(c.visible_indices(), &[0, 3]);
        assert_index_consistent(&c);
    }

    #[test]
    fn restore_state_round_trips_and_keeps_index_in_sync() {
        let mut c = catalog(5);
        c.set_filter(FilterMode::Picked);
        let before = c.assets[2].state();
        c.assign_flag(2, Flag::Picked);
        c.assign_rating(2, Rating::new(4));
        let after = c.assets[2].state();
        assert!(c.visible_indices().contains(&2));
        assert_index_consistent(&c);

        c.restore_state(2, &before);
        assert_eq!(c.assets[2].state(), before);
        assert!(!c.visible_indices().contains(&2));
        assert_index_consistent(&c);

        c.restore_state(2, &after);
        assert_eq!(c.assets[2].state(), after);
        assert_index_consistent(&c);
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
    fn collapsed_stack_shows_its_cover() {
        let mut c = catalog(4);
        for ix in 0..3 {
            c.assets[ix].stack_id = Some(7);
        }
        c.assets[1].stack_cover = true;
        c.refresh();
        assert_eq!(c.visible_indices(), &[0, 1, 2, 3]);
        assert_eq!(c.stack_count(7), 3);

        c.toggle_stack_collapsed(7);
        assert!(c.is_stack_collapsed(7));
        assert_eq!(c.visible_indices(), &[1, 3]);

        c.selected = 2;
        c.refresh();
        // The hidden member hands the selection to a visible photo.
        assert_eq!(c.selected, 1);

        c.toggle_stack_collapsed(7);
        assert_eq!(c.visible_indices(), &[0, 1, 2, 3]);
    }

    #[test]
    fn keyword_facet_and_search_include_metadata() {
        let mut c = catalog(2);
        c.assets[0].keywords = vec!["Travel".into(), "Landscape".into()];
        c.assets[0].caption = "Sunset over the fjord".into();
        c.assets[1].keywords = vec!["Portrait".into()];

        let filters = FilterSet {
            keyword: Some("travel".into()),
            ..Default::default()
        };
        c.set_filters(filters);
        assert_eq!(c.visible_indices(), &[0]);

        c.set_filters(FilterSet {
            search: "fjord".into(),
            ..Default::default()
        });
        assert_eq!(c.visible_indices(), &[0]);

        c.set_filters(FilterSet {
            search: "portrait".into(),
            ..Default::default()
        });
        assert_eq!(c.visible_indices(), &[1]);
    }

    #[test]
    fn missing_assets_are_hidden_and_removable() {
        let mut c = catalog(3);
        c.assets[1].missing = true;
        c.refresh();
        assert_eq!(c.visible_indices(), &[0, 2]);
        assert_eq!(c.counts().missing, 1);
        assert_eq!(c.missing_indices(), vec![1]);
        assert!(c.selected_asset().is_some());

        c.selected = 1;
        c.refresh();
        assert_eq!(c.selected, 0);

        let paths: HashSet<PathBuf> = [PathBuf::from("1.jpg")].into_iter().collect();
        assert_eq!(c.remove_assets(&paths), 1);
        assert_eq!(c.assets.len(), 2);
        assert_eq!(c.assets[1].id, 1);
        assert_eq!(c.counts().missing, 0);
        assert_eq!(c.visible_indices(), &[0, 1]);
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
