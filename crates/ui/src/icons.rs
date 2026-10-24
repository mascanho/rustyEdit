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
    /// A camera: snapshots of a photo's edits.
    Camera,
    Info,
    Adjust,
    /// An eye: view options.
    View,
    /// A 3x3 grid: composition guides.
    Grid,
    /// A cross: close / remove.
    Close,
    /// A pencil: rename.
    Edit,
    /// A tick.
    Check,
    /// A bin: delete.
    Trash,
    /// An arrow down into a tray: import.
    Download,
    /// An arrow up out of a tray: export.
    Upload,
    /// A film frame: film recipes.
    Film,
    /// A bookmark with a plus: save the current look.
    SaveLook,
    /// A photo split in two: before / after.
    BeforeAfter,
    /// Stacked cards: saved looks.
    Presets,
    Tag,
    /// Filled corner triangles: the histogram's clipping warnings.
    ClipShadows,
    ClipHighlights,
    /// A circular arrow: put back to the default.
    Reset,
    /// Six dots: something that can be dragged to reorder it.
    Grip,
    /// Develop tools: spot healing (a plaster), cloning (a stamp), linear
    /// and radial gradients, clipping warnings, and zoom.
    Heal,
    Clone,
    LinearGradient,
    RadialGradient,
    Clipping,
    ZoomIn,
    Minus,
    Plus,
    /// A pipette: pick white balance from the photo.
    Eyedropper,
    /// A wand with sparkles: an automatic adjustment.
    Auto,
    /// A landscape picture with a plus: a preset made from a photo.
    PresetFromPhoto,
    /// Two pictures with an arrow between: match one photo's look to
    /// another's.
    MatchLook,
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

    fn solid(&mut self, points: &[(f32, f32)]) {
        let points: Vec<_> = points.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.fill.add_polygon(&points, true);
        self.has_fill = true;
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
        Icon::Camera => {
            // A body with a viewfinder hump on top, and the lens.
            pen.shape(&[
                (1.5, 5.0),
                (5.0, 5.0),
                (6.0, 3.0),
                (10.0, 3.0),
                (11.0, 5.0),
                (14.5, 5.0),
                (14.5, 13.0),
                (1.5, 13.0),
            ]);
            pen.circle(8.0, 9.0, 2.5);
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
        Icon::BeforeAfter => {
            pen.shape(&[(2.0, 3.0), (14.0, 3.0), (14.0, 13.0), (2.0, 13.0)]);
            pen.line(&[(8.0, 1.5), (8.0, 14.5)]);
            // The "after" half is filled in.
            pen.solid(&[(8.0, 3.0), (14.0, 3.0), (14.0, 13.0), (8.0, 13.0)]);
        }
        Icon::Close => {
            pen.line(&[(4.0, 4.0), (12.0, 12.0)]);
            pen.line(&[(12.0, 4.0), (4.0, 12.0)]);
        }
        Icon::Edit => {
            pen.line(&[(3.0, 13.0), (3.6, 10.2), (11.0, 2.8), (13.2, 5.0), (5.8, 12.4), (3.0, 13.0)]);
            pen.line(&[(9.4, 4.4), (11.6, 6.6)]);
        }
        Icon::Trash => {
            pen.line(&[(2.5, 4.0), (13.5, 4.0)]);
            pen.line(&[(6.0, 4.0), (6.0, 2.5), (10.0, 2.5), (10.0, 4.0)]);
            pen.line(&[(3.8, 4.0), (4.6, 13.5), (11.4, 13.5), (12.2, 4.0)]);
            pen.line(&[(6.6, 6.5), (6.8, 11.5)]);
            pen.line(&[(9.4, 6.5), (9.2, 11.5)]);
        }
        Icon::Check => pen.line(&[(3.0, 8.5), (6.5, 12.0), (13.0, 4.5)]),
        Icon::Download => {
            pen.line(&[(8.0, 2.0), (8.0, 10.0)]);
            pen.line(&[(4.5, 6.8), (8.0, 10.2), (11.5, 6.8)]);
            pen.line(&[(2.5, 10.5), (2.5, 13.5), (13.5, 13.5), (13.5, 10.5)]);
        }
        Icon::Upload => {
            pen.line(&[(8.0, 10.0), (8.0, 2.0)]);
            pen.line(&[(4.5, 5.2), (8.0, 1.8), (11.5, 5.2)]);
            pen.line(&[(2.5, 10.5), (2.5, 13.5), (13.5, 13.5), (13.5, 10.5)]);
        }
        Icon::Film => {
            pen.shape(&[(2.0, 3.0), (14.0, 3.0), (14.0, 13.0), (2.0, 13.0)]);
            pen.line(&[(5.0, 3.0), (5.0, 13.0)]);
            pen.line(&[(11.0, 3.0), (11.0, 13.0)]);
            for y in [5.5, 8.0, 10.5] {
                pen.dot(3.5, y, 0.7);
                pen.dot(12.5, y, 0.7);
            }
        }
        Icon::SaveLook => {
            pen.shape(&[(4.0, 2.0), (12.0, 2.0), (12.0, 14.0), (8.0, 11.0), (4.0, 14.0)]);
            pen.line(&[(8.0, 4.0), (8.0, 8.0)]);
            pen.line(&[(6.0, 6.0), (10.0, 6.0)]);
        }
        Icon::Grid => {
            pen.shape(&[(2.0, 2.0), (14.0, 2.0), (14.0, 14.0), (2.0, 14.0)]);
            pen.line(&[(6.0, 2.0), (6.0, 14.0)]);
            pen.line(&[(10.0, 2.0), (10.0, 14.0)]);
            pen.line(&[(2.0, 6.0), (14.0, 6.0)]);
            pen.line(&[(2.0, 10.0), (14.0, 10.0)]);
        }
        Icon::View => {
            // An eye: almond outline with a pupil.
            pen.line(&[(1.0, 8.0), (4.0, 4.6), (8.0, 3.2), (12.0, 4.6), (15.0, 8.0)]);
            pen.line(&[(1.0, 8.0), (4.0, 11.4), (8.0, 12.8), (12.0, 11.4), (15.0, 8.0)]);
            pen.dot(8.0, 8.0, 2.2);
        }
        Icon::Presets => {
            // Two cards, one offset behind the other, with a spark.
            pen.shape(&[(5.0, 2.0), (14.0, 2.0), (14.0, 10.0), (5.0, 10.0)]);
            pen.shape(&[(2.0, 5.0), (11.0, 5.0), (11.0, 14.0), (2.0, 14.0)]);
            pen.dot(6.5, 9.5, 1.2);
        }
        Icon::Tag => {
            pen.shape(&[(2.0, 2.0), (8.5, 2.0), (14.0, 7.5), (7.5, 14.0), (2.0, 8.5)]);
            pen.dot(5.3, 5.3, 1.1);
        }
        Icon::ClipShadows => pen.solid(&[(2.0, 2.0), (14.0, 2.0), (2.0, 14.0)]),
        Icon::ClipHighlights => pen.solid(&[(2.0, 2.0), (14.0, 2.0), (14.0, 14.0)]),
        Icon::Reset => {
            pen.arc(8.0, 8.0, 5.0, -60.0, 230.0);
            pen.line(&[(10.0, 1.5), (10.6, 3.7), (8.3, 4.6)]);
        }
        Icon::Grip => {
            for y in [4.0, 8.0, 12.0] {
                pen.dot(6.0, y, 1.2);
                pen.dot(10.0, y, 1.2);
            }
        }
        Icon::Heal => {
            // A plaster lying diagonally, with its pad in the middle.
            let rotate = |(x, y): (f32, f32)| {
                let (s, c) = (
                    std::f32::consts::FRAC_1_SQRT_2,
                    std::f32::consts::FRAC_1_SQRT_2,
                );
                let (dx, dy) = (x - 8.0, y - 8.0);
                (8.0 + dx * c - dy * s, 8.0 + dx * s + dy * c)
            };
            let band = [(1.5, 5.5), (14.5, 5.5), (14.5, 10.5), (1.5, 10.5)].map(rotate);
            pen.shape(&band);
            pen.shape(&[(6.0, 6.5), (10.0, 6.5), (10.0, 9.5), (6.0, 9.5)].map(rotate));
        }
        Icon::Clone => {
            pen.circle(8.0, 4.0, 2.3);
            pen.line(&[(7.0, 6.3), (7.0, 9.0)]);
            pen.line(&[(9.0, 6.3), (9.0, 9.0)]);
            pen.shape(&[(2.5, 9.0), (13.5, 9.0), (13.5, 12.0), (2.5, 12.0)]);
            pen.line(&[(3.5, 14.2), (12.5, 14.2)]);
        }
        Icon::LinearGradient => {
            pen.shape(&[(2.0, 2.0), (14.0, 2.0), (14.0, 14.0), (2.0, 14.0)]);
            pen.line(&[(2.0, 5.5), (14.0, 5.5)]);
            pen.line(&[(2.0, 8.0), (14.0, 8.0)]);
            pen.line(&[(2.0, 10.5), (14.0, 10.5)]);
        }
        Icon::RadialGradient => {
            pen.shape(&[(2.0, 2.0), (14.0, 2.0), (14.0, 14.0), (2.0, 14.0)]);
            pen.circle(8.0, 8.0, 4.0);
            pen.dot(8.0, 8.0, 1.1);
        }
        Icon::Clipping => {
            pen.shape(&[(2.0, 2.0), (14.0, 2.0), (14.0, 14.0), (2.0, 14.0)]);
            pen.solid(&[(2.0, 2.0), (7.5, 2.0), (2.0, 7.5)]);
            pen.solid(&[(14.0, 2.0), (8.5, 2.0), (14.0, 7.5)]);
        }
        Icon::ZoomIn => {
            pen.circle(7.0, 7.0, 4.8);
            pen.line(&[(10.5, 10.5), (14.2, 14.2)]);
            pen.line(&[(4.8, 7.0), (9.2, 7.0)]);
            pen.line(&[(7.0, 4.8), (7.0, 9.2)]);
        }
        Icon::Minus => pen.line(&[(3.5, 8.0), (12.5, 8.0)]),
        Icon::Eyedropper => {
            // A pipette lying diagonally: solid bulb top right, a collar,
            // then the glass tube down to its tip at the bottom left.
            pen.solid(&[(10.6, 3.0), (12.4, 1.2), (14.8, 3.6), (13.0, 5.4)]);
            pen.line(&[(9.2, 4.0), (12.0, 6.8)]);
            pen.shape(&[(10.0, 5.4), (10.6, 6.0), (4.2, 12.4), (3.6, 11.8)]);
            pen.line(&[(3.9, 12.1), (2.0, 14.0)]);
        }
        Icon::PresetFromPhoto => {
            pen.shape(&[(1.5, 3.5), (12.0, 3.5), (12.0, 12.5), (1.5, 12.5)]);
            pen.line(&[(2.5, 11.0), (5.5, 7.5), (7.5, 9.5), (8.8, 8.2), (11.0, 10.5)]);
            pen.dot(8.8, 5.8, 0.9);
            pen.line(&[(13.0, 11.5), (13.0, 15.5)]);
            pen.line(&[(11.0, 13.5), (15.0, 13.5)]);
        }
        Icon::MatchLook => {
            pen.shape(&[(1.0, 4.5), (6.5, 4.5), (6.5, 11.5), (1.0, 11.5)]);
            pen.shape(&[(9.5, 4.5), (15.0, 4.5), (15.0, 11.5), (9.5, 11.5)]);
            pen.line(&[(2.0, 10.5), (3.8, 8.0), (5.5, 10.5)]);
            pen.line(&[(10.5, 10.5), (12.3, 8.0), (14.0, 10.5)]);
            pen.line(&[(7.2, 8.0), (8.8, 8.0)]);
            pen.line(&[(8.0, 7.0), (8.8, 8.0), (8.0, 9.0)]);
        }
        Icon::Auto => {
            // A wand from bottom left, with a star and two sparkles.
            pen.line(&[(2.0, 14.0), (9.5, 6.5)]);
            pen.line(&[(11.5, 1.5), (11.5, 5.5)]);
            pen.line(&[(9.5, 3.5), (13.5, 3.5)]);
            pen.dot(5.0, 3.5, 0.9);
            pen.dot(13.0, 10.0, 0.9);
        }
        Icon::Plus => {
            pen.line(&[(3.5, 8.0), (12.5, 8.0)]);
            pen.line(&[(8.0, 3.5), (8.0, 12.5)]);
        }
    }
    pen.paint(window, color);
}
