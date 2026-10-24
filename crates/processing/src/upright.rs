//! Auto upright: finds how far a photo is turned and how much its
//! verticals and horizontals converge, from the edges in the picture, so
//! the Crop section's Straighten and Vertical / Horizontal sliders can be
//! set in one click.
//!
//! Strong edges are grouped into near-vertical and near-horizontal ones.
//! For near-vertical edges the slope (`dx / dy`) at each pixel is fitted as
//! `m0 + beta * x`, where `x` is the distance from the centre: `m0` is a
//! plain turn (what Straighten fixes) and `beta` is convergence (what
//! Vertical fixes). The same is done for near-horizontal edges. Fits are
//! robust (Tukey's biweight), so a few diagonal edges or a curved horizon
//! don't drag them off.

use image::RgbImage;
use rustyedit_core::{Crop, MAX_PERSPECTIVE, MAX_STRAIGHTEN};

/// The longest edge the analysis works at.
const ANALYSIS_EDGE: u32 = 720;
/// Edges within this many degrees of vertical or horizontal count.
const TILT_WINDOW_DEG: f32 = 20.0;
/// The share of the strongest edges that vote.
const STRONG_SHARE: f32 = 0.12;
/// Fewest votes a fit needs to be trusted.
const MIN_VOTES: usize = 400;

/// What the picture needs, as the Crop section's values.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Upright {
    /// Straighten, degrees (positive turns clockwise), 0 if undetermined.
    pub angle: f32,
    /// The vertical and horizontal sliders, -100..=100.
    pub vertical: f32,
    pub horizontal: f32,
    /// 0..=1: how much of the evidence agreed, for the verticals and the
    /// horizontals.
    pub vertical_confidence: f32,
    pub horizontal_confidence: f32,
}

/// Which corrections to make.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UprightMode {
    /// Level the horizon (or the verticals): Straighten only.
    Level,
    /// Level, and make the verticals parallel.
    Vertical,
    /// Level, and make verticals and horizontals parallel.
    Full,
}

impl Upright {
    /// `crop` with this correction applied (leaving its rectangle and
    /// aspect alone); values too small to matter are left at zero.
    pub fn applied_to(&self, mut crop: Crop, mode: UprightMode) -> Crop {
        crop.angle = self.angle.clamp(-MAX_STRAIGHTEN, MAX_STRAIGHTEN);
        crop.vertical = 0.0;
        crop.horizontal = 0.0;
        if mode != UprightMode::Level && self.vertical.abs() >= 2.0 {
            crop.vertical = self.vertical.clamp(-MAX_PERSPECTIVE, MAX_PERSPECTIVE);
        }
        if mode == UprightMode::Full && self.horizontal.abs() >= 2.0 {
            crop.horizontal = self.horizontal.clamp(-MAX_PERSPECTIVE, MAX_PERSPECTIVE);
        }
        crop
    }
}

/// Analyses `photo` (any size; the geometry-only render of the oriented
/// photo, with no crop or turn applied).
pub fn detect(photo: &RgbImage) -> Upright {
    let (gray, w, h) = analysis_gray(photo);
    if w < 16 || h < 16 {
        return Upright::default();
    }
    let edges = strong_edges(&gray, w, h);
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let window = TILT_WINDOW_DEG.to_radians().tan();

    // Near-vertical edges: slope dx/dy against x. Near-horizontal: dy/dx
    // against y.
    let mut vertical = Vec::new();
    let mut horizontal = Vec::new();
    for e in &edges {
        // The edge runs perpendicular to the gradient.
        if e.gx.abs() > e.gy.abs() {
            let m = -e.gy / e.gx;
            if m.abs() < window {
                vertical.push((e.x - cx, m, e.weight));
            }
        } else if e.gy.abs() > 0.0 {
            let n = -e.gx / e.gy;
            if n.abs() < window {
                horizontal.push((e.y - cy, n, e.weight));
            }
        }
    }
    let v = fit_line(&vertical);
    let hz = fit_line(&horizontal);

    // Turn: verticals lean by m0 (top to the right is negative), horizons
    // by n0 (down to the right is positive).
    let angle_v = v.map(|f| f.intercept.atan().to_degrees());
    let angle_h = hz.map(|f| -f.intercept.atan().to_degrees());
    let angle = match (v, hz, angle_v, angle_h) {
        (Some(vf), Some(hf), Some(a), Some(b)) => {
            // Trust whichever has more evidence if they disagree.
            if (a - b).abs() > 3.0 {
                if vf.confidence >= hf.confidence { a } else { b }
            } else {
                (a * vf.confidence + b * hf.confidence) / (vf.confidence + hf.confidence)
            }
        }
        (Some(_), None, Some(a), _) => a,
        (None, Some(_), _, Some(b)) => b,
        _ => 0.0,
    };

    // Convergence. A vertical at distance a from the centre leans by
    // -a * py / cy, so py = -beta * cy; likewise for horizontals.
    let strength = |normalised: f32| (normalised / 0.35 * 100.0).clamp(-MAX_PERSPECTIVE, MAX_PERSPECTIVE);
    Upright {
        angle,
        vertical: v.map_or(0.0, |f| strength(-f.slope * cy)),
        horizontal: hz.map_or(0.0, |f| strength(-f.slope * cx)),
        vertical_confidence: v.map_or(0.0, |f| f.confidence),
        horizontal_confidence: hz.map_or(0.0, |f| f.confidence),
    }
}

struct Edge {
    x: f32,
    y: f32,
    gx: f32,
    gy: f32,
    weight: f32,
}

/// The photo as a grey image no larger than [`ANALYSIS_EDGE`], by box
/// averaging.
fn analysis_gray(photo: &RgbImage) -> (Vec<f32>, usize, usize) {
    let (pw, ph) = (photo.width() as usize, photo.height() as usize);
    let step = (pw.max(ph) as f32 / ANALYSIS_EDGE as f32).ceil().max(1.0) as usize;
    let (w, h) = (pw / step, ph / step);
    let mut gray = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0;
            for dy in 0..step {
                for dx in 0..step {
                    let p = photo.get_pixel((x * step + dx) as u32, (y * step + dy) as u32).0;
                    sum += 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
                }
            }
            gray[y * w + x] = sum / (step * step) as f32 / 255.0;
        }
    }
    (gray, w, h)
}

/// The strongest few percent of Sobel edges.
fn strong_edges(gray: &[f32], w: usize, h: usize) -> Vec<Edge> {
    let at = |x: usize, y: usize| gray[y * w + x];
    let mut all = Vec::with_capacity(w * h);
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let gx = (at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1));
            let gy = (at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1));
            let mag = gx.hypot(gy);
            all.push(Edge {
                x: x as f32,
                y: y as f32,
                gx,
                gy,
                weight: mag,
            });
        }
    }
    if all.is_empty() {
        return all;
    }
    let mut mags: Vec<f32> = all.iter().map(|e| e.weight).collect();
    let k = ((mags.len() as f32 * (1.0 - STRONG_SHARE)) as usize).min(mags.len() - 1);
    let (_, &mut threshold, _) = mags.select_nth_unstable_by(k, |a, b| a.total_cmp(b));
    let floor = 0.08f32.max(threshold);
    all.retain(|e| e.weight >= floor);
    all
}

#[derive(Clone, Copy)]
struct Fit {
    intercept: f32,
    slope: f32,
    /// Share of the weight that fits the line.
    confidence: f32,
}

/// A robust weighted fit of `m = intercept + slope * x` to `(x, m, weight)`
/// samples, or `None` without enough of them.
fn fit_line(samples: &[(f32, f32, f32)]) -> Option<Fit> {
    if samples.len() < MIN_VOTES {
        return None;
    }
    let total: f32 = samples.iter().map(|s| s.2).sum();
    let mut robust = vec![1.0f32; samples.len()];
    let (mut intercept, mut slope) = (0.0f32, 0.0f32);
    for round in 0..8 {
        let (mut sw, mut sx, mut sm, mut sxx, mut sxm) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for (s, r) in samples.iter().zip(&robust) {
            let w = (s.2 * r) as f64;
            let (x, m) = (s.0 as f64, s.1 as f64);
            sw += w;
            sx += w * x;
            sm += w * m;
            sxx += w * x * x;
            sxm += w * x * m;
        }
        if sw <= 0.0 {
            return None;
        }
        let det = sw * sxx - sx * sx;
        if det.abs() < 1e-9 {
            intercept = (sm / sw) as f32;
            slope = 0.0;
        } else {
            slope = ((sw * sxm - sx * sm) / det) as f32;
            intercept = ((sm - slope as f64 * sx) / sw) as f32;
        }
        // Reweigh by residual, Tukey-style, with a scale from the median.
        let mut residuals: Vec<f32> = samples
            .iter()
            .map(|s| (s.1 - intercept - slope * s.0).abs())
            .collect();
        residuals.sort_by(|a, b| a.total_cmp(b));
        let scale = (residuals[residuals.len() / 2] * 1.4826).max(0.004) * 2.5;
        for (s, r) in samples.iter().zip(robust.iter_mut()) {
            let u = (s.1 - intercept - slope * s.0).abs() / (scale * (1.0 + 2.0 / (round + 1) as f32));
            *r = if u < 1.0 { (1.0 - u * u).powi(2) } else { 0.0 };
        }
    }
    let inliers: f32 = samples
        .iter()
        .zip(&robust)
        .filter(|(_, r)| **r > 0.1)
        .map(|(s, _)| s.2)
        .sum();
    Some(Fit {
        intercept,
        slope,
        confidence: (inliers / total).clamp(0.0, 1.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// A scene of buildings: a grid of vertical and horizontal edges with
    /// some texture, so there's plenty to find.
    fn scene() -> RgbImage {
        let (w, h) = (900u32, 700u32);
        let mut img = RgbImage::from_pixel(w, h, Rgb([200, 200, 205]));
        // Windows: dark rectangles on a regular grid, each a slightly
        // different tone.
        let mut seed = 12345u32;
        let mut next = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 16) as f32 / 65536.0
        };
        for gy in 0..11 {
            for gx in 0..14 {
                let (x0, y0) = (40 + gx * 60, 30 + gy * 60);
                let tone = (40.0 + next() * 90.0) as u8;
                for y in y0..y0 + 38 {
                    for x in x0..x0 + 34 {
                        if x < w && y < h {
                            img.put_pixel(x, y, Rgb([tone, tone, tone.saturating_add(10)]));
                        }
                    }
                }
            }
        }
        img
    }

    fn warped(crop: Crop) -> RgbImage {
        crate::crop::straighten_and_crop(scene(), &Crop { rect: (0.0, 0.0, 1.0, 1.0), ..crop })
    }

    #[test]
    fn a_straight_scene_needs_nothing() {
        let found = detect(&scene());
        assert!(found.angle.abs() < 0.3, "{found:?}");
        assert!(found.vertical.abs() < 6.0 && found.horizontal.abs() < 6.0, "{found:?}");
    }

    #[test]
    fn finds_how_far_a_photo_is_turned() {
        for turn in [-4.0f32, 2.5, 6.0] {
            let found = detect(&warped(Crop { angle: turn, ..Default::default() }));
            assert!((found.angle + turn).abs() < 0.7, "turned {turn}: {found:?}");
        }
    }

    #[test]
    fn finds_converging_verticals() {
        for v in [-40.0f32, 30.0, 60.0] {
            let found = detect(&warped(Crop { vertical: v, ..Default::default() }));
            assert!(
                (found.vertical + v).abs() < 10.0,
                "vertical {v}: {found:?}"
            );
            assert!(found.horizontal.abs() < 12.0, "{found:?}");
        }
    }

    #[test]
    fn finds_converging_horizontals() {
        let found = detect(&warped(Crop { horizontal: 40.0, ..Default::default() }));
        assert!((found.horizontal + 40.0).abs() < 10.0, "{found:?}");
    }

    #[test]
    fn applying_the_answer_straightens_the_picture() {
        let skewed = Crop { angle: 3.0, vertical: 35.0, ..Default::default() };
        let picture = warped(skewed);
        let found = detect(&picture);
        let fix = found.applied_to(Crop::default(), UprightMode::Full);
        let corrected = crate::crop::straighten_and_crop(picture, &Crop { rect: (0.0, 0.0, 1.0, 1.0), ..fix });
        let after = detect(&corrected);
        assert!(after.angle.abs() < 1.0, "{after:?}");
        assert!(after.vertical.abs() < 12.0, "{after:?}");
    }

    #[test]
    fn a_picture_without_lines_is_left_alone() {
        let mut seed = 99u32;
        let noise = RgbImage::from_fn(300, 200, |_, _| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let v = (seed >> 24) as u8;
            Rgb([v, v, v])
        });
        let found = detect(&noise);
        let fix = found.applied_to(Crop::default(), UprightMode::Level);
        assert!(fix.angle.abs() < 2.0, "{found:?}");
    }

    #[test]
    fn modes_choose_which_corrections_apply() {
        let found = Upright { angle: 2.0, vertical: 30.0, horizontal: -20.0, ..Default::default() };
        let level = found.applied_to(Crop::default(), UprightMode::Level);
        assert_eq!((level.vertical, level.horizontal), (0.0, 0.0));
        let vertical = found.applied_to(Crop::default(), UprightMode::Vertical);
        assert_eq!((vertical.vertical, vertical.horizontal), (30.0, 0.0));
        let full = found.applied_to(Crop::default(), UprightMode::Full);
        assert_eq!((full.vertical, full.horizontal), (30.0, -20.0));
        assert_eq!(full.angle, 2.0);
    }
}
