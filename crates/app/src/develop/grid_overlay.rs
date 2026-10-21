//! Composition guides drawn over the Develop photo: thirds, a finer grid,
//! the golden-ratio grid, a Fibonacci spiral, golden triangles, diagonals
//! and a centre cross. The geometry is pure (unit coordinates over the
//! photo); painting maps it onto wherever the photo is on screen.

use gpui::{
    canvas, point, prelude::*, px, AnyElement, Bounds, ContentMask, Hsla, PathBuilder, Pixels,
};

/// Which guide is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum GridOverlay {
    #[default]
    Off,
    Thirds,
    Grid,
    GoldenRatio,
    Spiral,
    Triangles,
    Diagonals,
    Cross,
}

impl GridOverlay {
    pub(crate) const ALL: [GridOverlay; 8] = [
        GridOverlay::Off,
        GridOverlay::Thirds,
        GridOverlay::Grid,
        GridOverlay::GoldenRatio,
        GridOverlay::Spiral,
        GridOverlay::Triangles,
        GridOverlay::Diagonals,
        GridOverlay::Cross,
    ];

    pub(crate) fn label(&self) -> &'static str {
        match self {
            GridOverlay::Off => "None",
            GridOverlay::Thirds => "Rule of thirds",
            GridOverlay::Grid => "Grid 4 \u{d7} 4",
            GridOverlay::GoldenRatio => "Golden ratio",
            GridOverlay::Spiral => "Fibonacci spiral",
            GridOverlay::Triangles => "Golden triangles",
            GridOverlay::Diagonals => "Diagonals",
            GridOverlay::Cross => "Centre cross",
        }
    }

    /// Whether the guide has mirrored variants to cycle through.
    pub(crate) fn has_orientations(&self) -> bool {
        matches!(self, GridOverlay::Spiral | GridOverlay::Triangles)
    }

    /// The guide as polylines in unit coordinates (0..1 across and down
    /// the photo). `aspect` is the photo's width / height, which the
    /// spiral and triangles depend on. `orientation` (0-3) mirrors the
    /// guide horizontally and/or vertically.
    pub(crate) fn lines(&self, aspect: f32, orientation: u8) -> Vec<Vec<(f32, f32)>> {
        let aspect = aspect.max(0.05);
        let across = |x: f32| vec![(x, 0.0), (x, 1.0)];
        let down = |y: f32| vec![(0.0, y), (1.0, y)];
        let mut lines: Vec<Vec<(f32, f32)>> = match self {
            GridOverlay::Off => Vec::new(),
            GridOverlay::Thirds => vec![
                across(1.0 / 3.0),
                across(2.0 / 3.0),
                down(1.0 / 3.0),
                down(2.0 / 3.0),
            ],
            GridOverlay::Grid => [0.25, 0.5, 0.75]
                .iter()
                .flat_map(|&v| [across(v), down(v)])
                .collect(),
            GridOverlay::GoldenRatio => {
                const PHI: f32 = 0.381_966;
                vec![across(PHI), across(1.0 - PHI), down(PHI), down(1.0 - PHI)]
            }
            GridOverlay::Spiral => vec![spiral()],
            GridOverlay::Triangles => triangles(aspect),
            GridOverlay::Diagonals => {
                vec![vec![(0.0, 0.0), (1.0, 1.0)], vec![(1.0, 0.0), (0.0, 1.0)]]
            }
            GridOverlay::Cross => vec![across(0.5), down(0.5)],
        };
        let (flip_x, flip_y) = (orientation & 1 != 0, orientation & 2 != 0);
        if flip_x || flip_y {
            for line in &mut lines {
                for (x, y) in line.iter_mut() {
                    if flip_x {
                        *x = 1.0 - *x;
                    }
                    if flip_y {
                        *y = 1.0 - *y;
                    }
                }
            }
        }
        lines
    }
}

/// The golden spiral through successively smaller golden cuts of the photo,
/// as one polyline.
fn spiral() -> Vec<(f32, f32)> {
    const PHI: f32 = 0.618_034;
    const STEPS: usize = 24;
    let (mut x0, mut y0, mut x1, mut y1) = (0.0f32, 0.0f32, 1.0f32, 1.0f32);
    let mut points: Vec<(f32, f32)> = Vec::new();
    let mut previous_end: Option<(f32, f32)> = None;

    for step in 0..9 {
        let (w, h) = (x1 - x0, y1 - y0);
        // The piece cut off, the corner of it that touches what's left, and
        // what's left. Cuts go left, top, right, bottom around the photo.
        let (piece, centre, rest) = match step % 4 {
            0 => {
                let r = x0 + w * PHI;
                ((x0, y0, r, y1), (r, y1), (r, y0, x1, y1))
            }
            1 => {
                let b = y0 + h * PHI;
                ((x0, y0, x1, b), (x0, b), (x0, b, x1, y1))
            }
            2 => {
                let l = x1 - w * PHI;
                ((l, y0, x1, y1), (l, y0), (x0, y0, l, y1))
            }
            _ => {
                let t = y1 - h * PHI;
                ((x0, t, x1, y1), (x1, t), (x0, y0, x1, t))
            }
        };
        let (px0, py0, px1, py1) = piece;
        // The two corners of the piece next to the centre corner.
        let along_x = (if centre.0 == px0 { px1 } else { px0 }, centre.1);
        let along_y = (centre.0, if centre.1 == py0 { py1 } else { py0 });
        let (start, end) = match previous_end {
            Some(prev) if near(prev, along_y) => (along_y, along_x),
            Some(prev) if near(prev, along_x) => (along_x, along_y),
            _ => (along_x, along_y),
        };
        let (a, b) = (
            (start.0 - centre.0, start.1 - centre.1),
            (end.0 - centre.0, end.1 - centre.1),
        );
        for i in 0..=STEPS {
            let t = i as f32 / STEPS as f32 * std::f32::consts::FRAC_PI_2;
            points.push((
                centre.0 + a.0 * t.cos() + b.0 * t.sin(),
                centre.1 + a.1 * t.cos() + b.1 * t.sin(),
            ));
        }
        previous_end = Some(end);
        (x0, y0, x1, y1) = rest;
    }
    points
}

fn near(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
}

/// The "golden triangles": the main diagonal and the two lines dropped
/// from the other corners square onto it.
fn triangles(aspect: f32) -> Vec<Vec<(f32, f32)>> {
    // Work in real proportions (width = aspect, height = 1) so the lines
    // are truly perpendicular, then squeeze back to unit coordinates.
    let (dx, dy) = (aspect, 1.0f32);
    let len2 = dx * dx + dy * dy;
    let foot = |px: f32, py: f32| {
        let t = (px * dx + py * dy) / len2;
        (t * dx / aspect, t * dy)
    };
    vec![
        vec![(0.0, 0.0), (1.0, 1.0)],
        vec![(1.0, 0.0), foot(aspect, 0.0)],
        vec![(0.0, 1.0), foot(0.0, 1.0)],
    ]
}

/// Paints the guide over `photo` (window coordinates), clipped to `clip`.
pub(crate) fn overlay_canvas(
    lines: Vec<Vec<(f32, f32)>>,
    photo: Bounds<Pixels>,
    clip: Bounds<Pixels>,
    colour: Hsla,
) -> AnyElement {
    canvas(
        |_, _, _| {},
        move |_, _, window, _| {
            let map = |(x, y): (f32, f32)| {
                point(
                    photo.origin.x + photo.size.width * x,
                    photo.origin.y + photo.size.height * y,
                )
            };
            window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
                // A dark stroke under a light one keeps the guide visible on
                // both bright and dark parts of the photo.
                for (width, paint) in [(3.0, gpui::black().opacity(0.35)), (1.0, colour)] {
                    let mut path = PathBuilder::stroke(px(width));
                    for line in &lines {
                        let Some((first, rest)) = line.split_first() else {
                            continue;
                        };
                        path.move_to(map(*first));
                        for p in rest {
                            path.line_to(map(*p));
                        }
                    }
                    if let Ok(path) = path.build() {
                        window.paint_path(path, paint);
                    }
                }
            });
        },
    )
    .absolute()
    .size_full()
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thirds_has_four_lines_inside_the_photo() {
        let lines = GridOverlay::Thirds.lines(1.5, 0);
        assert_eq!(lines.len(), 4);
        assert!(lines
            .iter()
            .flatten()
            .all(|(x, y)| (0.0..=1.0).contains(x) && (0.0..=1.0).contains(y)));
    }

    #[test]
    fn spiral_is_continuous_and_stays_in_the_photo() {
        let spiral = GridOverlay::Spiral.lines(1.5, 0);
        let points = &spiral[0];
        assert!(points.len() > 100);
        for pair in points.windows(2) {
            let d = ((pair[0].0 - pair[1].0).powi(2) + (pair[0].1 - pair[1].1).powi(2)).sqrt();
            assert!(d < 0.1, "gap in the spiral: {d}");
        }
        assert!(points
            .iter()
            .all(|(x, y)| (-0.001..=1.001).contains(x) && (-0.001..=1.001).contains(y)));
    }

    #[test]
    fn mirroring_flips_the_guide() {
        let a = GridOverlay::Diagonals.lines(1.0, 0);
        let b = GridOverlay::Diagonals.lines(1.0, 1);
        assert_eq!(a[0][0], (0.0, 0.0));
        assert_eq!(b[0][0], (1.0, 0.0));
    }

    #[test]
    fn triangle_lines_meet_the_diagonal_at_right_angles() {
        let aspect = 1.5;
        let lines = GridOverlay::Triangles.lines(aspect, 0);
        let (c, foot) = (lines[1][0], lines[1][1]);
        // Back in real proportions.
        let d = (aspect, 1.0f32);
        let v = ((foot.0 - c.0) * aspect, foot.1 - c.1);
        assert!((v.0 * d.0 + v.1 * d.1).abs() < 1e-3);
    }
}
