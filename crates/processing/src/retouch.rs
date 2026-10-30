//! Spot removal on the original photo, before geometry and every other
//! adjustment.
//!
//! - Clone copies the source circle over the target, fading out at the
//!   feathered edge.
//! - Heal copies the source's texture but shifts its colour and brightness
//!   to match the target's surroundings: the difference between the two
//!   circles is measured all round their edges and blended inwards, so the
//!   patch meets its border seamlessly, a cheap stand-in for a full
//!   gradient-domain solve.

use image::RgbImage;
use rustyedit_core::{Spot, SpotKind, Spots};

/// Points sampled around a circle's edge.
const RING_SAMPLES: usize = 32;

/// Applies every spot in order, so later spots can build on earlier ones.
pub(crate) fn apply_spots(image: &mut RgbImage, spots: &Spots) {
    let (w, h) = (image.width() as f32, image.height() as f32);
    for spot in spots.as_slice() {
        // A stroke is applied as circles along it.
        for circle in spot.circles(w, h) {
            apply_spot(image, &circle);
        }
    }
}

/// A spot measured in pixels on `image`.
struct Circle {
    target: (f32, f32),
    source: (f32, f32),
    radius: f32,
}

impl Circle {
    fn new(image: &RgbImage, spot: &Spot) -> Self {
        let (w, h) = (image.width() as f32, image.height() as f32);
        Self {
            target: (spot.target.0 * w, spot.target.1 * h),
            source: (spot.source.0 * w, spot.source.1 * h),
            radius: (spot.radius * w.max(h)).max(1.0),
        }
    }
}

fn apply_spot(image: &mut RgbImage, spot: &Spot) {
    let circle = Circle::new(image, spot);
    let (width, height) = image.dimensions();
    let r = circle.radius;
    let x0 = (circle.target.0 - r).floor().max(0.0) as u32;
    let y0 = (circle.target.1 - r).floor().max(0.0) as u32;
    let x1 = ((circle.target.0 + r).ceil() as u32).min(width.saturating_sub(1));
    let y1 = ((circle.target.1 + r).ceil() as u32).min(height.saturating_sub(1));
    if x0 > x1 || y0 > y1 {
        return;
    }

    let edges = match spot.kind {
        SpotKind::Heal => Some(EdgeDifference::measure(image, &circle)),
        SpotKind::Clone => None,
    };
    let inner = 1.0 - spot.feather.clamp(0.0, 1.0);
    let offset = (
        circle.source.0 - circle.target.0,
        circle.source.1 - circle.target.1,
    );

    // Read the source from a copy of just its area as it was before this
    // spot, in case the source and target circles overlap.
    let source = SourcePatch::copy(image, circle.source, r);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = x as f32 + 0.5 - circle.target.0;
            let dy = y as f32 + 0.5 - circle.target.1;
            let d = (dx * dx + dy * dy).sqrt() / r;
            if d >= 1.0 {
                continue;
            }
            let mask = if d <= inner {
                1.0
            } else {
                let t = ((d - inner) / (1.0 - inner).max(1e-3)).clamp(0.0, 1.0);
                1.0 - t * t * (3.0 - 2.0 * t)
            };
            let weight = mask * spot.opacity.clamp(0.0, 1.0);
            if weight <= 0.0 {
                continue;
            }

            let src = source.sample(x as f32 + 0.5 + offset.0, y as f32 + 0.5 + offset.1);
            let patched = match &edges {
                Some(edges) => {
                    let delta = edges.at(dy.atan2(dx), d);
                    [src[0] + delta[0], src[1] + delta[1], src[2] + delta[2]]
                }
                None => src,
            };
            let dst = image.get_pixel(x, y).0;
            let out = image.get_pixel_mut(x, y);
            for c in 0..3 {
                let value = dst[c] as f32 + (patched[c] - dst[c] as f32) * weight;
                out.0[c] = value.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// A copy of the pixels around a spot's source circle.
struct SourcePatch {
    pixels: RgbImage,
    origin: (u32, u32),
}

impl SourcePatch {
    fn copy(image: &RgbImage, centre: (f32, f32), radius: f32) -> Self {
        let (w, h) = image.dimensions();
        let clamp_x = |v: f32| (v.max(0.0) as u32).min(w - 1);
        let clamp_y = |v: f32| (v.max(0.0) as u32).min(h - 1);
        let (x0, y0) = (
            clamp_x((centre.0 - radius - 1.0).floor()),
            clamp_y((centre.1 - radius - 1.0).floor()),
        );
        let (x1, y1) = (
            clamp_x((centre.0 + radius + 1.0).ceil()),
            clamp_y((centre.1 + radius + 1.0).ceil()),
        );
        let pixels = image::imageops::crop_imm(image, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image();
        Self {
            pixels,
            origin: (x0, y0),
        }
    }

    /// The photo pixel at (`x`, `y`), clamped to the copied area.
    fn sample(&self, x: f32, y: f32) -> [f32; 3] {
        sample(
            &self.pixels,
            x - self.origin.0 as f32,
            y - self.origin.1 as f32,
        )
    }
}

/// How the target's surroundings differ from the source's, all the way
/// round, for healing.
struct EdgeDifference {
    ring: [[f32; 3]; RING_SAMPLES],
    mean: [f32; 3],
}

impl EdgeDifference {
    fn measure(image: &RgbImage, circle: &Circle) -> Self {
        let mut ring = [[0.0; 3]; RING_SAMPLES];
        let mut mean = [0.0; 3];
        // Just outside the circle, so the blemish itself isn't sampled.
        let r = circle.radius * 1.08 + 1.0;
        for (i, diff) in ring.iter_mut().enumerate() {
            let angle = i as f32 / RING_SAMPLES as f32 * std::f32::consts::TAU;
            let (cos, sin) = (angle.cos(), angle.sin());
            let t = average(image, circle.target.0 + cos * r, circle.target.1 + sin * r);
            let s = average(image, circle.source.0 + cos * r, circle.source.1 + sin * r);
            for c in 0..3 {
                diff[c] = t[c] - s[c];
                mean[c] += diff[c] / RING_SAMPLES as f32;
            }
        }
        Self { ring, mean }
    }

    /// The colour shift at `angle`, a fraction `d` of the way out from the
    /// centre: the average shift in the middle, the local edge shift at
    /// the rim.
    fn at(&self, angle: f32, d: f32) -> [f32; 3] {
        let position =
            angle.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU * RING_SAMPLES as f32;
        let i = position.floor() as usize % RING_SAMPLES;
        let j = (i + 1) % RING_SAMPLES;
        let t = position.fract();
        std::array::from_fn(|c| {
            let edge = self.ring[i][c] * (1.0 - t) + self.ring[j][c] * t;
            self.mean[c] + (edge - self.mean[c]) * d * d
        })
    }
}

/// The pixel at (`x`, `y`), clamped to the image, as floats.
fn sample(image: &RgbImage, x: f32, y: f32) -> [f32; 3] {
    let px = (x.floor().max(0.0) as u32).min(image.width() - 1);
    let py = (y.floor().max(0.0) as u32).min(image.height() - 1);
    let p = image.get_pixel(px, py).0;
    [p[0] as f32, p[1] as f32, p[2] as f32]
}

/// A 3x3 average around (`x`, `y`), so noise doesn't skew an edge sample.
fn average(image: &RgbImage, x: f32, y: f32) -> [f32; 3] {
    let mut sum = [0.0; 3];
    for oy in -1..=1 {
        for ox in -1..=1 {
            let p = sample(image, x + ox as f32, y + oy as f32);
            for c in 0..3 {
                sum[c] += p[c] / 9.0;
            }
        }
    }
    sum
}

/// Picks where a new spot at `target` (fractions of the photo) with
/// `radius` (fraction of the long edge) should copy from: of a ring of
/// nearby candidates that fit inside the photo, the one whose surroundings
/// look most like the target's.
pub fn find_spot_source(image: &RgbImage, target: (f32, f32), radius: f32) -> (f32, f32) {
    let (w, h) = (image.width() as f32, image.height() as f32);
    let r = (radius * w.max(h)).max(1.0);
    let t = (target.0 * w, target.1 * h);
    let fits = |(x, y): (f32, f32)| x - r >= 0.0 && y - r >= 0.0 && x + r <= w && y + r <= h;

    // Two rings around a candidate (just outside the spot, and further
    // out), so structure running through the spot (a branch, a horizon, a
    // grain of texture) has to line up as well as the average colour.
    let ring = |centre: (f32, f32)| -> Vec<[f32; 3]> {
        [1.25f32, 1.8]
            .into_iter()
            .flat_map(|scale| {
                (0..24).map(move |i| {
                    let a = i as f32 / 24.0 * std::f32::consts::TAU;
                    (a, r * scale)
                })
            })
            .map(|(a, rr)| average(image, centre.0 + a.cos() * rr, centre.1 + a.sin() * rr))
            .collect()
    };
    let target_ring = ring(t);

    let mut best: Option<((f32, f32), f32)> = None;
    for distance in [2.2, 2.8, 3.6, 4.6, 6.0] {
        for i in 0..24 {
            let a = i as f32 / 24.0 * std::f32::consts::TAU;
            let candidate = (t.0 + a.cos() * r * distance, t.1 + a.sin() * r * distance);
            if !fits(candidate) {
                continue;
            }
            // Nearer sources are likelier to share the light and the
            // subject, so a far one has to be clearly better.
            let score: f32 = ring(candidate)
                .iter()
                .zip(&target_ring)
                .map(|(a, b)| (0..3).map(|c| (a[c] - b[c]).powi(2)).sum::<f32>())
                .sum::<f32>()
                * (1.0 + 0.04 * distance);
            if best.is_none_or(|(_, s)| score < s) {
                best = Some((candidate, score));
            }
        }
    }
    let (x, y) = best.map(|(p, _)| p).unwrap_or_else(|| {
        // Nothing fits (a huge spot, or one at an edge): just alongside.
        let x = if t.0 + r * 2.5 + r <= w {
            t.0 + r * 2.5
        } else {
            t.0 - r * 2.5
        };
        (x, t.1)
    });
    ((x / w).clamp(0.0, 1.0), (y / h).clamp(0.0, 1.0))
}

/// Places the spots that remove a painted stroke: `path` (fractions of the
/// photo, in painting order) is covered by overlapping circles of `radius`
/// (fraction of the long edge), and every circle copies from the same
/// offset, so texture running along the stroke stays continuous. The
/// offset is the one whose surroundings best match the stroke's, among
/// those whose sources stay inside the photo and clear of the stroke.
///
/// Returns `(target, source)` pairs; empty when `path` is.
/// How a remove stroke is filled: as a few straight segments sharing one
/// source offset (stored as stroke spots), or, when no shared offset fits,
/// as separate circles each with its own source.
#[derive(Debug, Clone, PartialEq)]
pub enum StrokeFill {
    /// A polyline (fractions of the photo) and the offset every point
    /// copies from.
    Segments {
        points: Vec<(f32, f32)>,
        offset: (f32, f32),
    },
    /// (target, source) circles.
    Circles(Vec<((f32, f32), (f32, f32))>),
}

/// Plans the fill for a painted stroke (see [`stroke_spots`]), its path
/// simplified so it takes at most `max_items` spots where it can: the
/// line is straightened until it fits, never further than the brush
/// radius from what was painted.
pub fn plan_stroke(
    image: &RgbImage,
    path: &[(f32, f32)],
    radius: f32,
    max_items: usize,
) -> StrokeFill {
    let circles = stroke_spots(image, path, radius);
    let Some(&(t0, s0)) = circles.first() else {
        return StrokeFill::Circles(Vec::new());
    };
    let offset = (s0.0 - t0.0, s0.1 - t0.1);
    let shared = circles.iter().all(|(t, s)| {
        ((s.0 - t.0) - offset.0).abs() < 1e-4 && ((s.1 - t.1) - offset.1).abs() < 1e-4
    });
    if !shared || circles.len() == 1 {
        return StrokeFill::Circles(circles);
    }
    let (w, h) = (image.width() as f32, image.height() as f32);
    let r = (radius * w.max(h)).max(1.0);
    let px: Vec<(f32, f32)> = path.iter().map(|&(u, v)| (u * w, v * h)).collect();
    let mut tolerance = r * 0.25;
    let mut points = simplify(&px, tolerance);
    while points.len().saturating_sub(1) > max_items.max(1) && tolerance < r {
        tolerance *= 1.5;
        points = simplify(&px, tolerance.min(r));
    }
    StrokeFill::Segments {
        points: points.into_iter().map(|(x, y)| (x / w, y / h)).collect(),
        offset,
    }
}

/// Douglas-Peucker: the fewest points that keep the line within `epsilon`
/// pixels of `points`.
fn simplify(points: &[(f32, f32)], epsilon: f32) -> Vec<(f32, f32)> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let (a, b) = (points[0], points[points.len() - 1]);
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = (dx * dx + dy * dy).sqrt();
    let distance = |p: (f32, f32)| {
        if length < 1e-3 {
            ((p.0 - a.0).powi(2) + (p.1 - a.1).powi(2)).sqrt()
        } else {
            ((p.0 - a.0) * dy - (p.1 - a.1) * dx).abs() / length
        }
    };
    let (index, farthest) = points[1..points.len() - 1]
        .iter()
        .enumerate()
        .map(|(i, &p)| (i + 1, distance(p)))
        .fold((0, 0.0f32), |best, x| if x.1 > best.1 { x } else { best });
    if farthest <= epsilon {
        return vec![a, b];
    }
    let mut left = simplify(&points[..=index], epsilon);
    let right = simplify(&points[index..], epsilon);
    left.pop();
    left.extend(right);
    left
}

pub fn stroke_spots(
    image: &RgbImage,
    path: &[(f32, f32)],
    radius: f32,
) -> Vec<((f32, f32), (f32, f32))> {
    let (w, h) = (image.width() as f32, image.height() as f32);
    let r = (radius * w.max(h)).max(1.0);
    let to_px = |(u, v): (f32, f32)| (u * w, v * h);
    let targets = spaced_along(&path.iter().map(|&p| to_px(p)).collect::<Vec<_>>(), r);
    if targets.is_empty() {
        return Vec::new();
    }

    let ring = |centre: (f32, f32)| -> Vec<[f32; 3]> {
        (0..16)
            .map(|i| {
                let a = i as f32 / 16.0 * std::f32::consts::TAU;
                average(image, centre.0 + a.cos() * r * 1.3, centre.1 + a.sin() * r * 1.3)
            })
            .collect()
    };
    // Only the ring samples that fall outside the stroke describe what
    // should be there; inside is the object being removed.
    let clear_of_stroke = |p: (f32, f32), margin: f32| {
        targets
            .iter()
            .all(|t| (t.0 - p.0).powi(2) + (t.1 - p.1).powi(2) > (r + margin).powi(2))
    };
    let target_rings: Vec<Vec<Option<[f32; 3]>>> = targets
        .iter()
        .map(|&t| {
            ring(t)
                .into_iter()
                .enumerate()
                .map(|(i, c)| {
                    let a = i as f32 / 16.0 * std::f32::consts::TAU;
                    let at = (t.0 + a.cos() * r * 1.3, t.1 + a.sin() * r * 1.3);
                    clear_of_stroke(at, 0.0).then_some(c)
                })
                .collect()
        })
        .collect();

    let fits = |(x, y): (f32, f32)| x - r >= 0.0 && y - r >= 0.0 && x + r <= w && y + r <= h;
    let mut best: Option<((f32, f32), f32)> = None;
    for distance in [2.2f32, 2.8, 3.6, 4.6, 6.0, 8.0, 11.0] {
        for i in 0..24 {
            let a = i as f32 / 24.0 * std::f32::consts::TAU;
            let offset = (a.cos() * r * distance, a.sin() * r * distance);
            let sources: Vec<_> = targets.iter().map(|t| (t.0 + offset.0, t.1 + offset.1)).collect();
            // A source over the stroke would copy the object back in.
            if !sources.iter().all(|&s| fits(s) && clear_of_stroke(s, r * 0.9)) {
                continue;
            }
            let mut score = 0.0f32;
            let mut samples = 0usize;
            for (s, target_ring) in sources.iter().zip(&target_rings) {
                for (a, b) in ring(*s).iter().zip(target_ring) {
                    if let Some(b) = b {
                        score += (0..3).map(|c| (a[c] - b[c]).powi(2)).sum::<f32>();
                        samples += 1;
                    }
                }
            }
            let score = score / samples.max(1) as f32 * (1.0 + 0.04 * distance);
            if best.is_none_or(|(_, s)| score < s) {
                best = Some((offset, score));
            }
        }
    }

    let to_frac = |(x, y): (f32, f32)| ((x / w).clamp(0.0, 1.0), (y / h).clamp(0.0, 1.0));
    targets
        .iter()
        .map(|&t| {
            let target = to_frac(t);
            let source = match best {
                Some((offset, _)) => to_frac((t.0 + offset.0, t.1 + offset.1)),
                // No shared offset fits (a stroke across most of the
                // photo): each spot finds its own.
                None => find_spot_source(image, target, radius),
            };
            (target, source)
        })
        .collect()
}

/// Points along `path` (pixels) 1.2 `radius` apart: circles of that radius
/// still cover most of the stroke's width between them (and the app draws
/// them a little larger). A click (one point) gives one.
fn spaced_along(path: &[(f32, f32)], radius: f32) -> Vec<(f32, f32)> {
    let Some(&first) = path.first() else {
        return Vec::new();
    };
    let step = radius * 1.2;
    let mut out = vec![first];
    let mut last = first;
    let mut carried = 0.0f32;
    for pair in path.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let length = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        if length <= f32::EPSILON {
            continue;
        }
        let mut along = step - carried;
        while along <= length {
            let t = along / length;
            last = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            out.push(last);
            along += step;
        }
        carried = length - (along - step);
    }
    // The stroke's end, unless the last circle already reaches it.
    let end = *path.last().unwrap_or(&first);
    if ((end.0 - last.0).powi(2) + (end.1 - last.1).powi(2)).sqrt() > radius * 0.35 {
        out.push(end);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spot(kind: SpotKind) -> Spot {
        Spot {
            kind,
            target: (0.25, 0.5),
            source: (0.75, 0.5),
            radius: 0.1,
            feather: 0.0,
            opacity: 1.0,
            end: None,
        }
    }

    #[test]
    fn clone_copies_the_source_over_the_target() {
        // Left half dark, right half light.
        let mut image = RgbImage::from_fn(100, 100, |x, _| {
            image::Rgb([if x < 50 { 20 } else { 220 }; 3])
        });
        let mut spots = Spots::default();
        spots.push(spot(SpotKind::Clone));
        apply_spots(&mut image, &spots);
        assert_eq!(image.get_pixel(25, 50)[0], 220);
        assert_eq!(
            image.get_pixel(5, 5)[0],
            20,
            "outside the spot is untouched"
        );
    }

    #[test]
    fn heal_matches_the_target_surroundings() {
        // Same as above, plus a white blemish in the dark half.
        let mut image = RgbImage::from_fn(100, 100, |x, y| {
            let blemish = (x as i32 - 25).pow(2) + (y as i32 - 50).pow(2) < 16;
            image::Rgb(
                [if blemish {
                    255
                } else if x < 50 {
                    20
                } else {
                    220
                }; 3],
            )
        });
        let mut spots = Spots::default();
        spots.push(spot(SpotKind::Heal));
        apply_spots(&mut image, &spots);
        // Healed to the dark surroundings, not the light source.
        let centre = image.get_pixel(25, 50)[0];
        assert!(centre < 40, "centre is {centre}");
    }

    #[test]
    fn auto_source_follows_the_structure_through_the_spot() {
        // Horizontal bands: the source has to sit in the same band, i.e.
        // level with the spot, whatever is above or below it.
        let image = RgbImage::from_fn(400, 300, |_, y| {
            let band = (y / 25) % 2;
            image::Rgb([if band == 0 { 40 } else { 210 }; 3])
        });
        let (x, y) = find_spot_source(&image, (0.5, 0.5 + 12.0 / 300.0), 0.02);
        let (dy, dx) = ((y - (0.5 + 12.0 / 300.0)) * 300.0, (x - 0.5) * 400.0);
        // Same band: the source's centre is a whole number of band pairs
        // (50 rows) away.
        assert!((dy.rem_euclid(50.0) < 6.0 || dy.rem_euclid(50.0) > 44.0) && dx.abs() > 8.0, "dx {dx}, dy {dy}");
    }

    #[test]
    fn auto_source_stays_inside_the_photo() {
        let image = RgbImage::from_pixel(200, 100, image::Rgb([90, 90, 90]));
        let (x, y) = find_spot_source(&image, (0.05, 0.5), 0.05);
        let r = 0.05 * 200.0;
        assert!(x * 200.0 - r >= 0.0 && y * 100.0 - r >= 0.0);
    }

    #[test]
    fn a_stroke_is_covered_by_spots_copying_from_clear_of_it() {
        // Grey photo with a dark horizontal wire across the middle.
        let image = RgbImage::from_fn(200, 200, |_, y| {
            image::Rgb([if (98..=102).contains(&y) { 10 } else { 150 }; 3])
        });
        let path = [(0.2, 0.5), (0.5, 0.5), (0.8, 0.5)];
        let spots = stroke_spots(&image, &path, 0.02);
        // 120 px long, 4 px radius: a circle every 4.8 px.
        assert!((25..=27).contains(&spots.len()), "{}", spots.len());
        let offset = (spots[0].1 .0 - spots[0].0 .0, spots[0].1 .1 - spots[0].0 .1);
        for (target, source) in &spots {
            // One shared offset, off the wire.
            assert!((source.0 - target.0 - offset.0).abs() < 1e-3);
            assert!((source.1 - target.1 - offset.1).abs() < 1e-3);
            assert!((source.1 - 0.5).abs() > 0.03, "{source:?}");
        }
        assert!(stroke_spots(&image, &[], 0.02).is_empty());
        assert_eq!(stroke_spots(&image, &[(0.5, 0.5)], 0.02).len(), 1);
    }

    #[test]
    fn a_straight_stroke_is_one_segment_that_covers_like_the_circles() {
        let image = RgbImage::from_fn(200, 200, |_, y| {
            image::Rgb([if (98..=102).contains(&y) { 10 } else { 150 }; 3])
        });
        // Painted slightly wobbly, but straight within the brush.
        let path: Vec<(f32, f32)> = (0..=30)
            .map(|i| (0.2 + i as f32 * 0.02, 0.5 + if i % 2 == 0 { 0.001 } else { -0.001 }))
            .collect();
        match plan_stroke(&image, &path, 0.02, 32) {
            StrokeFill::Segments { points, .. } => assert_eq!(points.len(), 2, "{points:?}"),
            other => panic!("{other:?}"),
        }
        // Applied, the stroke spot removes the wire.
        let StrokeFill::Segments { points, offset } = plan_stroke(&image, &path, 0.02, 32) else {
            unreachable!()
        };
        let mut spots = Spots::default();
        spots.push(Spot {
            kind: SpotKind::Heal,
            target: points[0],
            source: (points[0].0 + offset.0, points[0].1 + offset.1),
            end: Some(points[1]),
            radius: 0.022,
            feather: 0.3,
            opacity: 1.0,
        });
        let mut healed = image.clone();
        apply_spots(&mut healed, &spots);
        assert!(healed.get_pixel(100, 100)[0] > 120, "{:?}", healed.get_pixel(100, 100));
    }
}
