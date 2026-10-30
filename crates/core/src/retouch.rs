//! Spot removal: circles that cover a blemish with pixels from elsewhere
//! in the photo, either copied as they are (clone) or blended into their
//! new surroundings (heal).
//!
//! Positions are fractions of the *original* photo's width and height,
//! before rotation and flips, so spots stay on their blemish when the photo
//! is turned later. [`to_source`] and [`to_display`] convert between that
//! and the photo as shown. Sizes are fractions of the long edge, which
//! rotating doesn't change.
//!
//! Spots live in a fixed-size array so `DevelopSettings` stays `Copy`.

use serde::{Deserialize, Serialize};

use crate::DevelopSettings;

/// Most spots a photo can have.
pub const MAX_SPOTS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SpotKind {
    /// Blend the source's texture into the target's colour and brightness.
    #[default]
    Heal,
    /// Copy the source over the target unchanged.
    Clone,
}

impl SpotKind {
    pub fn label(&self) -> &'static str {
        match self {
            SpotKind::Heal => "Heal",
            SpotKind::Clone => "Clone",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Spot {
    pub kind: SpotKind,
    /// Centre of the area being covered.
    pub target: (f32, f32),
    /// Centre of the area copied from.
    pub source: (f32, f32),
    /// Radius as a fraction of the photo's long edge.
    pub radius: f32,
    /// 0..=1: how much of the radius fades out at the edge.
    pub feather: f32,
    /// 0..=1.
    pub opacity: f32,
    /// A stroke, not a circle: the spot covers everything within `radius`
    /// of the line from `target` to here, all copied from the same offset
    /// (`source` is where `target` copies from). Painted by the remove
    /// tool, so one stroke takes a few spots instead of dozens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<(f32, f32)>,
}

impl Spot {
    /// The circles this spot is applied as, on a `width` x `height` photo:
    /// itself, or for a stroke, circles along it close enough to leave no
    /// gaps (spaced 1.2 radii, as the remove tool paints them).
    pub fn circles(&self, width: f32, height: f32) -> Vec<Spot> {
        let Some(end) = self.end else {
            return vec![*self];
        };
        let r = (self.radius * width.max(height)).max(1.0);
        let (ax, ay) = (self.target.0 * width, self.target.1 * height);
        let (bx, by) = (end.0 * width, end.1 * height);
        let length = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
        let steps = (length / (r * 1.2)).ceil().max(1.0) as usize;
        let offset = (self.source.0 - self.target.0, self.source.1 - self.target.1);
        (0..=steps)
            .map(|i| {
                let t = i as f32 / steps as f32;
                let target = (
                    self.target.0 + (end.0 - self.target.0) * t,
                    self.target.1 + (end.1 - self.target.1) * t,
                );
                Spot {
                    target,
                    source: (target.0 + offset.0, target.1 + offset.1),
                    end: None,
                    ..*self
                }
            })
            .collect()
    }
}

/// A photo's spots, oldest first (later spots apply on top).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Spots {
    items: [Spot; MAX_SPOTS],
    len: u8,
}

impl Default for Spots {
    fn default() -> Self {
        Self {
            items: [Spot::default(); MAX_SPOTS],
            len: 0,
        }
    }
}

impl Spots {
    pub fn as_slice(&self) -> &[Spot] {
        &self.items[..(self.len as usize).min(MAX_SPOTS)]
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut Spot> {
        let len = self.len();
        self.items[..len].get_mut(index)
    }

    /// Adds a spot and returns its index, or `None` when full.
    pub fn push(&mut self, spot: Spot) -> Option<usize> {
        let len = self.len();
        if len >= MAX_SPOTS {
            return None;
        }
        self.items[len] = spot;
        self.len += 1;
        Some(len)
    }

    pub fn remove(&mut self, index: usize) {
        let len = self.len();
        if index >= len {
            return;
        }
        self.items.copy_within(index + 1..len, index);
        self.items[len - 1] = Spot::default();
        self.len -= 1;
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Clockwise quarter turns and flips, as the pipeline applies them: flips
/// first, then the rotation.
fn geometry(settings: &DevelopSettings) -> (u8, bool, bool) {
    let quarters = ((settings.rotation_degrees / 90.0).round() as i32).rem_euclid(4) as u8;
    (quarters, settings.flip_horizontal, settings.flip_vertical)
}

/// A point on the original photo → the same point on the photo as shown.
pub fn to_display((u, v): (f32, f32), settings: &DevelopSettings) -> (f32, f32) {
    let (quarters, flip_h, flip_v) = geometry(settings);
    let u = if flip_h { 1.0 - u } else { u };
    let v = if flip_v { 1.0 - v } else { v };
    match quarters {
        1 => (1.0 - v, u),
        2 => (1.0 - u, 1.0 - v),
        3 => (v, 1.0 - u),
        _ => (u, v),
    }
}

/// A point on the photo as shown → the same point on the original photo.
pub fn to_source((x, y): (f32, f32), settings: &DevelopSettings) -> (f32, f32) {
    let (quarters, flip_h, flip_v) = geometry(settings);
    let (u, v) = match quarters {
        1 => (y, 1.0 - x),
        2 => (1.0 - x, 1.0 - y),
        3 => (1.0 - y, x),
        _ => (x, y),
    };
    let u = if flip_h { 1.0 - u } else { u };
    let v = if flip_v { 1.0 - v } else { v };
    (u, v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_source_round_trip_through_every_orientation() {
        for quarters in 0..4 {
            for (flip_h, flip_v) in [(false, false), (true, false), (false, true), (true, true)] {
                let settings = DevelopSettings {
                    rotation_degrees: quarters as f32 * 90.0,
                    flip_horizontal: flip_h,
                    flip_vertical: flip_v,
                    ..Default::default()
                };
                let point = (0.2, 0.7);
                let back = to_source(to_display(point, &settings), &settings);
                assert!((back.0 - point.0).abs() < 1e-6 && (back.1 - point.1).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn quarter_turn_moves_the_top_left_to_the_top_right() {
        let settings = DevelopSettings {
            rotation_degrees: 90.0,
            ..Default::default()
        };
        assert_eq!(to_display((0.0, 0.0), &settings), (1.0, 0.0));
    }

    #[test]
    fn push_and_remove_keep_order() {
        let mut spots = Spots::default();
        for i in 0..3 {
            spots.push(Spot {
                radius: i as f32,
                ..Default::default()
            });
        }
        spots.remove(1);
        let radii: Vec<f32> = spots.as_slice().iter().map(|s| s.radius).collect();
        assert_eq!(radii, vec![0.0, 2.0]);
    }
}
