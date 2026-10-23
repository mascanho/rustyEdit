//! Smart culling: how sharp each photo is, which photos belong to the same
//! burst, and the suggestions that follow (the sharpest of each burst, and
//! photos that are probably soft).
//!
//! Sharpness is measured on the cached thumbnail (a 512-pixel JPEG that the
//! library decodes anyway), so it costs about a millisecond per photo and
//! is computed once, then saved with the catalog. At that size it catches
//! missed focus, camera shake and motion blur, not the last bit of
//! critical focus.

use std::collections::HashMap;

use crate::Asset;

/// Below this score a photo is flagged as probably soft on its own.
/// Scores run from 0 to about 100. Set low on purpose: sharp but sparse
/// pictures (graphics, smooth skies) score low too, and a wrong "soft"
/// warning is worse than a missed one. Within a burst the comparison is
/// fairer, so `SOFT_IN_BURST` is more sensitive.
pub const SOFT_THRESHOLD: f32 = 8.0;
/// In a burst, a frame this much less sharp than the best one is flagged
/// as soft too (the scene is the same, so the comparison is fair).
const SOFT_IN_BURST: f32 = 0.55;
/// Frames taken this close together (seconds) can be one burst.
const BURST_GAP_SECS: i64 = 2;
/// ...if they also look alike (difference-hash bits apart).
const BURST_SIMILARITY: u32 = 22;
/// The picture is split into this many cells across and down; the
/// sharpest cells decide the score, so a sharp subject on a soft
/// background still counts as sharp.
const GRID: usize = 8;
/// Cells flatter than this (standard deviation, 0-255) are left out: sky
/// and plain walls say nothing about focus.
const MIN_CELL_CONTRAST: f32 = 6.0;

/// How sharp a greyscale image is (`luma`, row by row): the edge strength
/// (Laplacian) of its sharpest quarter of cells, relative to their
/// contrast, so dim and bright photos compare fairly. `None` if the image
/// is too small to judge; `Some(0.0)` if it has no detail at all.
pub fn sharpness(luma: &[u8], width: usize, height: usize) -> Option<f32> {
    if width < 16 || height < 16 || luma.len() < width * height {
        return None;
    }
    let cell_x: Vec<usize> = (0..width).map(|x| x * GRID / width).collect();
    let mut edges = [0f64; GRID * GRID];
    let mut sums = [0f64; GRID * GRID];
    let mut squares = [0f64; GRID * GRID];
    let mut counts = [0u32; GRID * GRID];
    for y in 1..height - 1 {
        let row = y * width;
        let cell_row = (y * GRID / height) * GRID;
        for x in 1..width - 1 {
            let i = row + x;
            let c = luma[i] as i32;
            let laplacian =
                (4 * c - luma[i - 1] as i32 - luma[i + 1] as i32 - luma[i - width] as i32
                    - luma[i + width] as i32)
                    .unsigned_abs();
            let cell = cell_row + cell_x[x];
            edges[cell] += laplacian as f64;
            sums[cell] += c as f64;
            squares[cell] += (c * c) as f64;
            counts[cell] += 1;
        }
    }
    let mut scores: Vec<f32> = (0..GRID * GRID)
        .filter(|&cell| counts[cell] > 0)
        .filter_map(|cell| {
            let n = counts[cell] as f64;
            let mean = sums[cell] / n;
            let sd = (squares[cell] / n - mean * mean).max(0.0).sqrt() as f32;
            (sd >= MIN_CELL_CONTRAST).then(|| (edges[cell] / n) as f32 / (sd + 10.0))
        })
        .collect();
    if scores.is_empty() {
        return Some(0.0);
    }
    scores.sort_by(|a, b| b.total_cmp(a));
    let top = &scores[..(scores.len() / 4).max(1)];
    Some(top.iter().sum::<f32>() / top.len() as f32 * 100.0)
}

/// Groups of photos that are takes of the same moment: frames shot within
/// a couple of seconds of each other on the same camera that look alike,
/// (when `by_time`), and photos the user stacked. Each group (two or more photos, catalog
/// indices) is in capture order. Virtual copies and missing photos are
/// left out.
pub fn burst_groups(assets: &[Asset], by_time: bool) -> Vec<Vec<usize>> {
    let eligible = |a: &Asset| !a.missing && !a.is_virtual_copy();
    let mut parent: Vec<usize> = (0..assets.len()).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let union = |parent: &mut Vec<usize>, a: usize, b: usize| {
        let (ra, rb) = (find(parent, a), find(parent, b));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    };

    // Stacks.
    let mut stacks: HashMap<u64, usize> = HashMap::new();
    for (i, asset) in assets.iter().enumerate() {
        if let (true, Some(id)) = (eligible(asset), asset.stack_id) {
            match stacks.get(&id) {
                Some(&first) => union(&mut parent, first, i),
                None => {
                    stacks.insert(id, i);
                }
            }
        }
    }

    // Bursts: neighbours in capture order on the same camera.
    let mut order: Vec<usize> = (0..assets.len())
        .filter(|&i| by_time && eligible(&assets[i]) && assets[i].meta.captured > 0)
        .collect();
    order.sort_by(|&a, &b| {
        let (x, y) = (&assets[a], &assets[b]);
        (&x.meta.camera, x.meta.captured, &x.path).cmp(&(&y.meta.camera, y.meta.captured, &y.path))
    });
    for pair in order.windows(2) {
        let (a, b) = (&assets[pair[0]], &assets[pair[1]]);
        let close = a.meta.camera == b.meta.camera
            && b.meta.captured - a.meta.captured <= BURST_GAP_SECS;
        let alike =
            a.phash == 0 || b.phash == 0 || (a.phash ^ b.phash).count_ones() <= BURST_SIMILARITY;
        if close && alike {
            union(&mut parent, pair[0], pair[1]);
        }
    }

    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, asset) in assets.iter().enumerate() {
        if eligible(asset) {
            let root = find(&mut parent, i);
            groups.entry(root).or_default().push(i);
        }
    }
    let mut groups: Vec<Vec<usize>> = groups.into_values().filter(|g| g.len() > 1).collect();
    for group in &mut groups {
        group.sort_by_key(|&i| (assets[i].meta.captured, i));
    }
    groups.sort_by_key(|g| g[0]);
    groups
}

/// What culling suggests for one photo.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CullMark {
    /// The sharpest frame of its burst.
    pub best: bool,
    /// Probably out of focus or blurred.
    pub soft: bool,
    /// Its place in its burst: (1-based position, burst size).
    pub burst: Option<(u16, u16)>,
}

/// Culling suggestions for a whole catalog.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CullInfo {
    /// One per asset, by catalog index.
    pub marks: Vec<CullMark>,
    pub groups: Vec<Vec<usize>>,
}

impl CullInfo {
    /// Suggestions for `assets`; `bursts_by_time` also groups frames shot
    /// within seconds of each other (stacks always group).
    pub fn analyze(assets: &[Asset], bursts_by_time: bool) -> Self {
        let mut marks = vec![CullMark::default(); assets.len()];
        for (mark, asset) in marks.iter_mut().zip(assets) {
            mark.soft = !asset.missing
                && !asset.is_virtual_copy()
                && asset.sharpness.is_some_and(|s| s < SOFT_THRESHOLD);
        }
        let groups = burst_groups(assets, bursts_by_time);
        for group in &groups {
            let len = group.len().min(u16::MAX as usize) as u16;
            for (pos, &i) in group.iter().enumerate() {
                marks[i].burst = Some(((pos + 1).min(u16::MAX as usize) as u16, len));
            }
            let measured: Vec<(usize, f32)> = group
                .iter()
                .filter_map(|&i| Some((i, assets[i].sharpness?)))
                .collect();
            if measured.len() < 2 {
                continue;
            }
            let (best, best_score) = measured
                .iter()
                .copied()
                .fold((measured[0].0, f32::MIN), |acc, (i, s)| if s > acc.1 { (i, s) } else { acc });
            marks[best].best = true;
            for &(i, s) in &measured {
                if i != best && s < best_score * SOFT_IN_BURST {
                    marks[i].soft = true;
                }
            }
        }
        Self { marks, groups }
    }

    pub fn mark(&self, index: usize) -> CullMark {
        self.marks.get(index).copied().unwrap_or_default()
    }

    /// The suggested frame of each burst.
    pub fn best(&self) -> Vec<usize> {
        self.indices(|m| m.best)
    }

    pub fn soft(&self) -> Vec<usize> {
        self.indices(|m| m.soft)
    }

    fn indices(&self, pick: impl Fn(&CullMark) -> bool) -> Vec<usize> {
        self.marks
            .iter()
            .enumerate()
            .filter(|(_, m)| pick(m))
            .map(|(i, _)| i)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A detailed test picture: overlapping circles and stripes, with
    /// hard edges, like an in-focus photo.
    fn scene(width: usize, height: usize) -> Vec<u8> {
        let mut out = vec![0u8; width * height];
        for y in 0..height {
            for x in 0..width {
                let (fx, fy) = (x as f32, y as f32);
                let mut v: f32 = 90.0;
                for k in 0..12 {
                    let cx = (k * 97 % width) as f32;
                    let cy = (k * 61 % height) as f32;
                    let r = 10.0 + (k * 7 % 40) as f32;
                    if (fx - cx).hypot(fy - cy) < r {
                        v += if k % 2 == 0 { 70.0 } else { -50.0 };
                    }
                }
                if (x / 9 + y / 13) % 2 == 0 {
                    v += 25.0;
                }
                out[y * width + x] = v.clamp(0.0, 255.0) as u8;
            }
        }
        out
    }

    /// Box blur of radius `r`, run three times (close to a Gaussian).
    fn blur(img: &[u8], width: usize, height: usize, r: usize) -> Vec<u8> {
        let mut cur: Vec<f32> = img.iter().map(|&v| v as f32).collect();
        for _ in 0..3 {
            let mut tmp = cur.clone();
            for y in 0..height {
                for x in 0..width {
                    let (lo, hi) = (x.saturating_sub(r), (x + r).min(width - 1));
                    tmp[y * width + x] =
                        (lo..=hi).map(|i| cur[y * width + i]).sum::<f32>() / (hi - lo + 1) as f32;
                }
            }
            for y in 0..height {
                for x in 0..width {
                    let (lo, hi) = (y.saturating_sub(r), (y + r).min(height - 1));
                    cur[y * width + x] =
                        (lo..=hi).map(|i| tmp[i * width + x]).sum::<f32>() / (hi - lo + 1) as f32;
                }
            }
        }
        cur.iter().map(|&v| v.round() as u8).collect()
    }

    #[test]
    fn blur_lowers_sharpness_past_the_threshold() {
        let (w, h) = (512, 384);
        let sharp = scene(w, h);
        let s0 = sharpness(&sharp, w, h).unwrap();
        let s1 = sharpness(&blur(&sharp, w, h, 1), w, h).unwrap();
        let s3 = sharpness(&blur(&sharp, w, h, 3), w, h).unwrap();
        assert!(s0 > s1 && s1 > s3, "{s0} {s1} {s3}");
        assert!(s0 > SOFT_THRESHOLD, "sharp scored {s0}");
        assert!(s3 < SOFT_THRESHOLD, "blurred scored {s3}");
        // Brightness and contrast don't decide it.
        let dim: Vec<u8> = sharp.iter().map(|&v| v / 2).collect();
        let s_dim = sharpness(&dim, w, h).unwrap();
        assert!(s_dim > SOFT_THRESHOLD, "dim scored {s_dim}");
    }

    #[test]
    fn flat_and_tiny_images() {
        assert_eq!(sharpness(&[128; 64 * 64], 64, 64), Some(0.0));
        assert_eq!(sharpness(&[0; 10], 5, 2), None);
    }

    fn shot(i: u64, captured: i64, phash: u64, sharpness: f32) -> Asset {
        let mut a = Asset::new(i, PathBuf::from(format!("{i}.jpg")));
        a.meta.captured = captured;
        a.meta.camera = "Cam".into();
        a.phash = phash;
        a.sharpness = Some(sharpness);
        a
    }

    #[test]
    fn bursts_group_close_alike_frames_and_pick_the_sharpest() {
        let assets = vec![
            shot(0, 1000, 0b1111, 40.0),
            shot(1, 1001, 0b1110, 60.0),
            shot(2, 1002, 0b1111, 20.0),
            // Ten seconds later: a new scene.
            shot(3, 1012, 0b1111, 50.0),
            // Same second, but a completely different picture.
            shot(4, 1012, !0b1111, 50.0),
        ];
        let info = CullInfo::analyze(&assets, true);
        // Without time-based bursts only stacks group.
        assert!(CullInfo::analyze(&assets, false).groups.is_empty());
        assert_eq!(info.groups, vec![vec![0, 1, 2]]);
        assert!(info.mark(1).best);
        assert_eq!(info.mark(0).burst, Some((1, 3)));
        // 20 is under 55% of 60: soft for this burst.
        assert!(info.mark(2).soft && !info.mark(0).soft);
        assert_eq!(info.best(), vec![1]);
        assert!(info.mark(3).burst.is_none());
    }

    #[test]
    fn stacks_count_as_bursts_and_copies_are_left_out() {
        let mut a = shot(0, 0, 0, 30.0);
        let mut b = shot(1, 0, 0, 35.0);
        a.stack_id = Some(7);
        b.stack_id = Some(7);
        let mut copy = b.clone();
        copy.id = 2;
        copy.copy_name = Some("Copy 1".into());
        copy.sharpness = Some(99.0);
        let info = CullInfo::analyze(&[a, b, copy], false);
        assert_eq!(info.groups, vec![vec![0, 1]]);
        assert!(info.mark(1).best && !info.mark(2).best);
    }

    #[test]
    fn soft_photos_are_flagged_on_their_own() {
        let info = CullInfo::analyze(&[shot(0, 0, 0, 5.0), shot(1, 100, 0, 40.0)], true);
        assert_eq!(info.soft(), vec![0]);
    }
}
