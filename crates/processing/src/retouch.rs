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
    for spot in spots.as_slice() {
        apply_spot(image, spot);
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

    let ring = |centre: (f32, f32)| -> Vec<[f32; 3]> {
        (0..24)
            .map(|i| {
                let a = i as f32 / 24.0 * std::f32::consts::TAU;
                let rr = r * 1.3;
                average(image, centre.0 + a.cos() * rr, centre.1 + a.sin() * rr)
            })
            .collect()
    };
    let target_ring = ring(t);

    let mut best: Option<((f32, f32), f32)> = None;
    for distance in [2.2, 3.2] {
        for i in 0..12 {
            let a = i as f32 / 12.0 * std::f32::consts::TAU;
            let candidate = (t.0 + a.cos() * r * distance, t.1 + a.sin() * r * distance);
            if !fits(candidate) {
                continue;
            }
            let score: f32 = ring(candidate)
                .iter()
                .zip(&target_ring)
                .map(|(a, b)| (0..3).map(|c| (a[c] - b[c]).powi(2)).sum::<f32>())
                .sum();
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
    fn auto_source_stays_inside_the_photo() {
        let image = RgbImage::from_pixel(200, 100, image::Rgb([90, 90, 90]));
        let (x, y) = find_spot_source(&image, (0.05, 0.5), 0.05);
        let r = 0.05 * 200.0;
        assert!(x * 200.0 - r >= 0.0 && y * 100.0 - r >= 0.0);
    }
}
