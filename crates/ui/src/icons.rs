//! Small line icons drawn as vector paths on a 16x16 grid.
//!
//! No icon font is bundled, and Unicode symbols come from whichever
//! fallback font has them, so they render at mismatched sizes and
//! baselines. Drawing the icons ourselves keeps every one the same size,
//! centred and crisp at any scale.

use gpui::{canvas, point, prelude::*, px, Bounds, Hsla, PathBuilder, Pixels, Window};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    ChevronRight,
    ChevronDown,
    ChevronLeft,
    Folder,
    Search,
    Collections,
    Filter,
    Duplicates,
    History,
    Info,
    Adjust,
    Tag,
}

/// `icon` drawn `size` pixels square in `color`.
pub fn icon(icon: Icon, size: f32, color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| paint_icon(window, bounds, icon, color),
    )
    .size(px(size))
    .flex_shrink_0()
}

/// Strokes and fills on the 16-unit grid, scaled into `bounds`.
struct Pen {
    origin: (f32, f32),
    scale: f32,
    stroke: PathBuilder,
    fill: PathBuilder,
    has_fill: bool,
}

impl Pen {
    fn new(bounds: Bounds<Pixels>) -> Self {
        let scale = f32::from(bounds.size.width.min(bounds.size.height)) / 16.0;
        Self {
            origin: (f32::from(bounds.origin.x), f32::from(bounds.origin.y)),
            scale,
            // Thin enough to stay delicate small, never thinner than a pixel.
            stroke: PathBuilder::stroke(px((1.4 * scale).max(1.0))),
            fill: PathBuilder::fill(),
            has_fill: false,
        }
    }

    fn at(&self, x: f32, y: f32) -> gpui::Point<Pixels> {
        point(
            px(self.origin.0 + x * self.scale),
            px(self.origin.1 + y * self.scale),
        )
    }

    /// An open polyline.
    fn line(&mut self, points: &[(f32, f32)]) {
        let Some((first, rest)) = points.split_first() else {
            return;
        };
        self.stroke.move_to(self.at(first.0, first.1));
        for &(x, y) in rest {
            self.stroke.line_to(self.at(x, y));
        }
    }

    /// A closed outline.
    fn shape(&mut self, points: &[(f32, f32)]) {
        let points: Vec<_> = points.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.stroke.add_polygon(&points, true);
    }

    fn arc_points(&self, cx: f32, cy: f32, r: f32, from: f32, to: f32) -> Vec<(f32, f32)> {
        const STEPS: usize = 28;
        (0..=STEPS)
            .map(|i| {
                let a = (from + (to - from) * i as f32 / STEPS as f32).to_radians();
                (cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    }

    fn circle(&mut self, cx: f32, cy: f32, r: f32) {
        let points = self.arc_points(cx, cy, r, 0.0, 360.0);
        self.shape(&points[..points.len() - 1]);
    }

    fn arc(&mut self, cx: f32, cy: f32, r: f32, from: f32, to: f32) {
        let points = self.arc_points(cx, cy, r, from, to);
        self.line(&points);
    }

    fn dot(&mut self, cx: f32, cy: f32, r: f32) {
        let points: Vec<_> = self
            .arc_points(cx, cy, r, 0.0, 360.0)
            .into_iter()
            .map(|(x, y)| self.at(x, y))
            .collect();
        self.fill.add_polygon(&points, true);
        self.has_fill = true;
    }

    fn paint(self, window: &mut Window, color: Hsla) {
        if let Ok(path) = self.stroke.build() {
            window.paint_path(path, color);
        }
        if self.has_fill {
            if let Ok(path) = self.fill.build() {
                window.paint_path(path, color);
            }
        }
    }
}

fn paint_icon(window: &mut Window, bounds: Bounds<Pixels>, icon: Icon, color: Hsla) {
    let mut pen = Pen::new(bounds);
    match icon {
        Icon::ChevronRight => pen.line(&[(6.0, 3.5), (10.5, 8.0), (6.0, 12.5)]),
        Icon::ChevronDown => pen.line(&[(3.5, 6.0), (8.0, 10.5), (12.5, 6.0)]),
        Icon::ChevronLeft => pen.line(&[(10.0, 3.5), (5.5, 8.0), (10.0, 12.5)]),
        Icon::Folder => pen.shape(&[
            (1.5, 3.5),
            (6.0, 3.5),
            (7.5, 5.0),
            (14.5, 5.0),
            (14.5, 13.0),
            (1.5, 13.0),
        ]),
        Icon::Search => {
            pen.circle(7.0, 7.0, 4.5);
            pen.line(&[(10.3, 10.3), (14.0, 14.0)]);
        }
        Icon::Collections => {
            pen.shape(&[(1.5, 5.0), (11.5, 5.0), (11.5, 14.0), (1.5, 14.0)]);
            pen.line(&[(4.5, 2.0), (14.5, 2.0), (14.5, 11.0)]);
        }
        Icon::Filter => pen.shape(&[
            (2.0, 3.0),
            (14.0, 3.0),
            (9.5, 8.5),
            (9.5, 13.5),
            (6.5, 12.0),
            (6.5, 8.5),
        ]),
        Icon::Duplicates => {
            pen.shape(&[(1.5, 5.0), (10.5, 5.0), (10.5, 14.0), (1.5, 14.0)]);
            pen.line(&[
                (5.5, 5.0),
                (5.5, 2.0),
                (14.5, 2.0),
                (14.5, 11.0),
                (10.5, 11.0),
            ]);
        }
        Icon::History => {
            // A clock face ringed by a curved arrow.
            pen.arc(8.0, 8.0, 5.5, 200.0, 520.0);
            pen.line(&[(1.3, 4.6), (2.8, 6.1), (4.6, 4.9)]);
            pen.line(&[(8.0, 5.0), (8.0, 8.0), (10.2, 9.4)]);
        }
        Icon::Info => {
            pen.circle(8.0, 8.0, 6.5);
            pen.line(&[(8.0, 7.2), (8.0, 11.5)]);
            pen.dot(8.0, 4.9, 0.9);
        }
        Icon::Adjust => {
            for (y, knob) in [(4.0, 10.5), (8.0, 5.0), (12.0, 11.5)] {
                pen.line(&[(2.0, y), (14.0, y)]);
                pen.dot(knob, y, 1.8);
            }
        }
        Icon::Tag => {
            pen.shape(&[(2.0, 2.0), (8.5, 2.0), (14.0, 7.5), (7.5, 14.0), (2.0, 8.5)]);
            pen.dot(5.3, 5.3, 1.1);
        }
    }
    pen.paint(window, color);
}
