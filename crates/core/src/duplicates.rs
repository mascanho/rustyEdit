//! Duplicate and near-duplicate detection.
//!
//! * **Exact** groups share a sampled content hash and pixel dimensions.
//! * **Similar** groups are found with a 64-bit dHash (perceptual hash):
//!   photos within a small Hamming distance are variants of the same shot
//!   (bursts, re-edits, re-exports).
//!
//! The expensive pairing runs off the UI thread; this module only does the
//! pure computation.

use std::collections::HashMap;

use crate::asset::Asset;

/// How far apart two perceptual hashes may be and still count as similar.
pub const SIMILAR_HAMMING: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateKind {
    /// Same file content.
    Exact,
    /// Visually similar (bursts, variants, different exports).
    Similar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateGroup {
    pub kind: DuplicateKind,
    /// Indices into the catalog's asset list, ascending.
    pub indices: Vec<usize>,
}

/// The hash facts needed to compare one photo, without cloning the whole
/// `Asset` (paths, history and develop settings).
#[derive(Debug, Clone, Copy)]
pub struct PhotoHash {
    pub index: usize,
    pub width: u32,
    pub height: u32,
    pub content_hash: u64,
    pub phash: u64,
}

impl From<&Asset> for PhotoHash {
    fn from(asset: &Asset) -> Self {
        Self {
            index: asset.id as usize,
            width: asset.width,
            height: asset.height,
            content_hash: asset.content_hash,
            phash: asset.phash,
        }
    }
}

/// Hamming distance between two 64-bit hashes.
pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Groups photos by exact content, then by visual similarity.
pub fn find_duplicates(assets: &[Asset]) -> Vec<DuplicateGroup> {
    let hashes: Vec<PhotoHash> = assets
        .iter()
        .enumerate()
        .map(|(index, asset)| PhotoHash {
            index,
            width: asset.width,
            height: asset.height,
            content_hash: asset.content_hash,
            phash: asset.phash,
        })
        .collect();
    find_duplicates_in(&hashes)
}

pub fn find_duplicates_in(hashes: &[PhotoHash]) -> Vec<DuplicateGroup> {
    let mut union = UnionFind::new(hashes.len());
    let mut exact = vec![false; hashes.len()];

    // Exact: same non-zero content hash and same dimensions.
    let mut by_content: HashMap<(u64, u32, u32), Vec<usize>> = HashMap::new();
    for (i, h) in hashes.iter().enumerate() {
        if h.content_hash != 0 {
            by_content
                .entry((h.content_hash, h.width, h.height))
                .or_default()
                .push(i);
        }
    }
    for group in by_content.values().filter(|g| g.len() > 1) {
        for &i in &group[1..] {
            union.union(group[0], i);
        }
        for &i in group {
            exact[i] = true;
        }
    }

    // Similar: pair within the same aspect-ratio bucket to avoid the full
    // O(n²) comparison on large catalogs. Bursts and re-exports keep their
    // aspect ratio, so this rarely misses a real match.
    let mut by_aspect: HashMap<i32, Vec<usize>> = HashMap::new();
    for (i, h) in hashes.iter().enumerate() {
        if h.phash != 0 && h.height > 0 {
            let aspect = (h.width as f32 / h.height as f32 * 50.0).round() as i32;
            by_aspect.entry(aspect).or_default().push(i);
        }
    }
    for bucket in by_aspect.values() {
        for (a, &i) in bucket.iter().enumerate() {
            for &j in &bucket[a + 1..] {
                if union.find(i) == union.find(j) {
                    continue;
                }
                if hamming(hashes[i].phash, hashes[j].phash) <= SIMILAR_HAMMING {
                    union.union(i, j);
                }
            }
        }
    }

    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..hashes.len() {
        groups.entry(union.find(i)).or_default().push(i);
    }

    let mut out: Vec<DuplicateGroup> = groups
        .into_values()
        .filter(|members| members.len() > 1)
        .map(|mut members| {
            members.sort_unstable();
            let kind = if members.iter().all(|&i| exact[i]) {
                DuplicateKind::Exact
            } else {
                DuplicateKind::Similar
            };
            let indices = members.iter().map(|&i| hashes[i].index).collect();
            DuplicateGroup { kind, indices }
        })
        .collect();
    // Exact groups first, then largest.
    out.sort_by(|a, b| {
        (a.kind == DuplicateKind::Similar)
            .cmp(&(b.kind == DuplicateKind::Similar))
            .then(b.indices.len().cmp(&a.indices.len()))
            .then(a.indices.first().cmp(&b.indices.first()))
    });
    out
}

struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, mut i: usize) -> usize {
        while self.parent[i] != i {
            self.parent[i] = self.parent[self.parent[i]];
            i = self.parent[i];
        }
        i
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[rb] = ra;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(index: usize, content_hash: u64, phash: u64) -> PhotoHash {
        PhotoHash {
            index,
            width: 1000,
            height: 800,
            content_hash,
            phash,
        }
    }

    #[test]
    fn finds_exact_and_similar_groups() {
        let hashes = vec![
            hash(0, 111, 0b1010),
            hash(1, 111, 0b1010),
            hash(2, 222, 0b1011),
            hash(3, 333, u64::MAX),
            hash(4, 444, 0),
        ];
        let groups = find_duplicates_in(&hashes);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, DuplicateKind::Similar);
        assert_eq!(groups[0].indices, vec![0, 1, 2]);
    }

    #[test]
    fn distance_is_symmetric() {
        assert_eq!(hamming(0, u64::MAX), 64);
        assert_eq!(hamming(0b1010, 0b1000), 1);
    }
}
