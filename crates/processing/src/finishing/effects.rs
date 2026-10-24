//! Effects: Dehaze, Vignette and Grain.

use rustyedit_core::DevelopSettings;

use super::{luma, smoothstep};

/// Removes (or adds) atmospheric haze with a per-pixel dark-channel
/// estimate: the dimmest channel approximates how much haze veils a pixel.
pub(crate) struct Dehaze(f32);

impl Dehaze {
    pub(crate) fn new(amount: f32) -> Option<Self> {
        (amount.abs() > 0.5).then(|| Self(amount / 100.0))
    }

    /// The strength, -1..=1.
    pub(crate) fn amount(&self) -> f32 {
        self.0
    }

    pub(crate) fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let amount = self.0;
        if amount > 0.0 {
            let dark = rgb[0].min(rgb[1]).min(rgb[2]);
            let transmission = (1.0 - amount * 0.7 * dark).max(0.35);
            rgb.map(|c| (c - 1.0) / transmission + 1.0)
        } else {
            // Negative: veil the photo in a soft, light haze.
            rgb.map(|c| c + (0.9 - c) * -amount * 0.5)
        }
    }
}

/// Darkens (or lightens) towards the corners, in an ellipse matching the
/// photo's proportions.
pub(crate) struct Vignette {
    pub(crate) amount: f32,
    pub(crate) centre: (f32, f32),
    pub(crate) inner: f32,
    pub(crate) outer: f32,
    /// 0..=1: how far the ellipse is made into a circle.
    pub(crate) round: f32,
}

impl Vignette {
    pub(crate) fn new(settings: &DevelopSettings, width: u32, height: u32) -> Option<Self> {
        if settings.vignette.abs() <= 0.5 {
            return None;
        }
        let inner = 0.1 + settings.vignette_midpoint / 100.0 * 0.8;
        let feather = 0.05 + settings.vignette_feather / 100.0 * 0.9;
        Some(Self {
            amount: settings.vignette / 100.0,
            centre: (width as f32 / 2.0, height as f32 / 2.0),
            inner,
            outer: inner + feather,
            round: (settings.vignette_roundness / 100.0).clamp(0.0, 1.0),
        })
    }

    /// What distances from the centre are measured against, across and
    /// down: the photo's half sizes for an ellipse, both the longer one
    /// for a circle.
    pub(crate) fn reach(&self) -> (f32, f32) {
        let (cx, cy) = self.centre;
        let longer = cx.max(cy);
        (
            (cx + (longer - cx) * self.round).max(1.0),
            (cy + (longer - cy) * self.round).max(1.0),
        )
    }

    pub(crate) fn apply(&self, rgb: [f32; 3], x: u32, y: u32) -> [f32; 3] {
        let (cx, cy) = self.centre;
        let (nx, ny) = self.reach();
        let dx = (x as f32 + 0.5 - cx) / nx;
        let dy = (y as f32 + 0.5 - cy) / ny;
        // 0 at the centre, 1 in the corners.
        let r = (dx * dx + dy * dy).sqrt() / std::f32::consts::SQRT_2;
        let v = smoothstep(self.inner, self.outer, r);
        if self.amount < 0.0 {
            rgb.map(|c| c * (1.0 + self.amount * v))
        } else {
            rgb.map(|c| c + (1.0 - c) * self.amount * v)
        }
    }
}

/// Film-like monochrome grain, strongest in the midtones. The pattern is a
/// fixed hash of the pixel position, so it doesn't crawl between renders,
/// and its cell size scales with the image so the preview and a full-size
/// export look alike.
pub(crate) struct Grain {
    pub(crate) strength: f32,
    pub(crate) cell: u32,
}

impl Grain {
    pub(crate) fn new(settings: &DevelopSettings, width: u32, height: u32) -> Option<Self> {
        if settings.grain <= 0.5 {
            return None;
        }
        let long_edge = width.max(height) as f32;
        let cell = (1.0 + settings.grain_size / 100.0 * 3.0) * (long_edge / 2000.0).max(0.5);
        Some(Self {
            strength: settings.grain / 100.0 * 0.15,
            cell: cell.round().max(1.0) as u32,
        })
    }

    pub(crate) fn apply(&self, rgb: [f32; 3], x: u32, y: u32) -> [f32; 3] {
        // Coarse cells for the grain's size, with a little per-pixel
        // noise on top so it doesn't look blocky.
        let noise = 0.7 * hash(x / self.cell, y / self.cell) + 0.3 * hash(x ^ 0x9e37, y ^ 0x79b9);
        let l = luma(rgb);
        let weight = 0.4 + 2.4 * l * (1.0 - l);
        let delta = noise * self.strength * weight;
        rgb.map(|c| c + delta)
    }
}

/// A well-mixed integer hash of a position, as a value in `-1..=1`.
fn hash(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(0x8da6_b343) ^ y.wrapping_mul(0xd816_3841);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^= h >> 15;
    (h as f32 / u32::MAX as f32) * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vignette(roundness: f32) -> Vignette {
        let settings = DevelopSettings {
            vignette: -100.0,
            vignette_roundness: roundness,
            ..Default::default()
        };
        Vignette::new(&settings, 400, 200).unwrap()
    }

    #[test]
    fn roundness_turns_the_ellipse_into_a_circle() {
        // On a wide photo, the middle of the top edge is as far out as the
        // middle of the left edge for an ellipse (both at 1), but nearer
        // the centre than it for a circle.
        let ellipse = vignette(0.0).reach();
        assert_eq!(ellipse, (200.0, 100.0));
        let circle = vignette(100.0).reach();
        assert_eq!(circle, (200.0, 200.0));
        let half = vignette(50.0).reach();
        assert_eq!(half, (200.0, 150.0));
    }

    #[test]
    fn a_round_vignette_leaves_the_short_edges_lighter() {
        let mid_top = |v: &Vignette| v.apply([1.0; 3], 200, 0)[0];
        assert!(mid_top(&vignette(100.0)) > mid_top(&vignette(0.0)));
    }
}
