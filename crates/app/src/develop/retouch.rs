//! The Retouch section and its tools: Heal and Clone spots.
//!
//! With a tool picked, clicking the photo drops a spot there and picks a
//! matching area to copy from; dragging a spot's solid circle moves what's
//! covered, dragging its dashed circle moves where it copies from. Size,
//! Feather and Opacity set new spots, or change the selected one. Delete
//! (Backspace) removes the selected spot. Holding Space still pans.

use rustyedit_ui::sp;
use gpui::{
    canvas, div, point, prelude::*, px, AnyElement, Bounds, ClickEvent, Context, Hsla, MouseButton,
    MouseDownEvent, MouseMoveEvent, PathBuilder, Pixels, Point, SharedString, Window,
};
use rustyedit_core::{to_display, to_source, DevelopSettings, Spot, SpotKind, MAX_SPOTS};
use rustyedit_ui::{SliderSpec, SliderTrack, Theme};

use super::tools::list_row;
use crate::workspace::Workspace;

/// Brush size range, as fractions of the photo's long edge.
const MIN_SIZE: f32 = 0.005;
const MAX_SIZE: f32 = 0.15;

/// Tool state for spot removal.
pub(crate) struct RetouchUi {
    /// The tool clicks on the photo use; `None` leaves the photo alone.
    pub(crate) tool: Option<SpotKind>,
    /// For new spots: radius (fraction of the long edge), feather and
    /// opacity (0..=1).
    size: f32,
    feather: f32,
    opacity: f32,
    pub(crate) selected: Option<usize>,
    drag: Option<SpotDrag>,
    hover: Option<Point<Pixels>>,
}

impl Default for RetouchUi {
    fn default() -> Self {
        Self {
            tool: None,
            size: 0.03,
            feather: 0.5,
            opacity: 1.0,
            selected: None,
            drag: None,
            hover: None,
        }
    }
}

impl RetouchUi {
    /// Forgets the selection and any drag (another photo was selected).
    pub(crate) fn reset_selection(&mut self) {
        self.selected = None;
        self.drag = None;
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SpotPart {
    Target,
    Source,
}

#[derive(Clone, Copy)]
struct SpotDrag {
    index: usize,
    part: SpotPart,
}

/// The brush settings a slider edits.
#[derive(Clone, Copy)]
enum Brush {
    Size,
    Feather,
    Opacity,
}

impl Brush {
    fn label(&self) -> &'static str {
        match self {
            Brush::Size => "Size",
            Brush::Feather => "Feather",
            Brush::Opacity => "Opacity",
        }
    }

    /// Slider value (0..=100, or a percentage of the long edge for size).
    fn get(&self, spot: &Spot) -> f32 {
        match self {
            Brush::Size => spot.radius * 100.0,
            Brush::Feather => spot.feather * 100.0,
            Brush::Opacity => spot.opacity * 100.0,
        }
    }

    fn set(&self, spot: &mut Spot, value: f32) {
        match self {
            Brush::Size => spot.radius = (value / 100.0).clamp(MIN_SIZE, MAX_SIZE),
            Brush::Feather => spot.feather = (value / 100.0).clamp(0.0, 1.0),
            Brush::Opacity => spot.opacity = (value / 100.0).clamp(0.0, 1.0),
        }
    }

    fn spec(&self) -> SliderSpec {
        let (min, max, default) = match self {
            Brush::Size => (MIN_SIZE * 100.0, MAX_SIZE * 100.0, 3.0),
            Brush::Feather => (0.0, 100.0, 50.0),
            Brush::Opacity => (0.0, 100.0, 100.0),
        };
        SliderSpec {
            min,
            max,
            default,
            signed: false,
            track: SliderTrack::Accent,
        }
    }
}

impl Workspace {
    // --- Panel ----------------------------------------------------------------

    pub(super) fn retouch_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(develop) = self.catalog.selected_asset().map(|a| a.develop) else {
            return Vec::new();
        };
        let ui = &self.develop_ui.retouch;

        let tools = div()
            .flex()
            .flex_row()
            .gap_1()
            .child(self.tool_button(SpotKind::Heal, cx))
            .child(self.tool_button(SpotKind::Clone, cx));

        let hint = match ui.tool {
            Some(_) => "Click a blemish to cover it \u{00B7} drag the circles to adjust",
            None => "Pick Heal or Clone, then click the photo",
        };

        // The sliders edit the selected spot, or the settings for new ones.
        let subject = ui
            .selected
            .and_then(|ix| develop.spots.as_slice().get(ix).copied())
            .unwrap_or(Spot {
                radius: ui.size,
                feather: ui.feather,
                opacity: ui.opacity,
                ..Default::default()
            });
        let sliders = [Brush::Size, Brush::Feather, Brush::Opacity].map(|brush| {
            let entity = cx.entity();
            rustyedit_ui::labeled_slider(
                SharedString::from(format!("retouch-{}", brush.label())),
                brush.label(),
                brush.get(&subject),
                brush.spec(),
                &theme,
                move |value, _w, cx| {
                    entity.update(cx, |this, cx| this.set_brush(brush, value, cx));
                },
            )
            .into_any_element()
        });

        let mut rows = vec![
            tools.into_any_element(),
            div()
                .text_size(sp(10.0))
                .text_color(theme.text_faint)
                .child(hint)
                .into_any_element(),
        ];
        rows.extend(sliders);
        rows.extend(self.spot_list(&develop, cx));
        rows
    }

    fn tool_button(&self, kind: SpotKind, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let active = self.develop_ui.retouch.tool == Some(kind);
        div()
            .id(SharedString::from(format!("retouch-tool-{}", kind.label())))
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .h(sp(30.0))
            .rounded_md()
            .border_1()
            .cursor_pointer()
            .text_size(sp(11.5))
            .when(active, |el| {
                el.bg(theme.accent_muted)
                    .border_color(theme.accent)
                    .text_color(theme.text)
            })
            .when(!active, |el| {
                el.border_color(theme.border_subtle)
                    .text_color(theme.text_muted)
                    .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
            })
            .child(rustyedit_ui::icon(
                match kind {
                    SpotKind::Heal => rustyedit_ui::Icon::Heal,
                    SpotKind::Clone => rustyedit_ui::Icon::Clone,
                },
                16.0,
                if active { theme.text } else { theme.text_muted },
            ))
            .tooltip(rustyedit_ui::tip(
                match kind {
                    SpotKind::Heal => "Heal spots (Q)",
                    SpotKind::Clone => "Clone spots",
                },
                &theme,
            ))
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                this.toggle_photo_tool(kind.into(), cx);
            }))
            .into_any_element()
    }

    /// The photo's spots, to select or delete, and "Clear all".
    fn spot_list(&self, develop: &DevelopSettings, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let spots = develop.spots.as_slice();
        if spots.is_empty() {
            return Vec::new();
        }
        let mut rows = vec![div()
            .flex()
            .flex_row()
            .justify_between()
            .pt_1()
            .text_size(sp(10.0))
            .text_color(theme.text_faint)
            .child(format!("{} of {MAX_SPOTS} spots", spots.len()))
            .child(
                div()
                    .id("retouch-clear")
                    .cursor_pointer()
                    .hover(|el| el.text_color(theme.danger))
                    .child("Clear all")
                    .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                        this.develop_ui.retouch.reset_selection();
                        this.edit_spots("Clear spots", cx, |develop| develop.spots.clear());
                    })),
            )
            .into_any_element()];
        for (ix, spot) in spots.iter().enumerate() {
            rows.push(list_row(
                ("retouch-spot", ix),
                &theme,
                format!("{} {}", spot.kind.label(), ix + 1),
                Some(format!("{:.1}%", spot.radius * 100.0)),
                self.develop_ui.retouch.selected == Some(ix),
                cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.develop_ui.retouch.selected = Some(ix);
                    cx.notify();
                }),
                Some(Box::new(cx.listener(
                    move |this, _: &ClickEvent, _w, cx| {
                        this.remove_spot(ix, cx);
                    },
                ))),
            ));
        }
        rows
    }

    fn set_brush(&mut self, brush: Brush, value: f32, cx: &mut Context<Self>) {
        match self.develop_ui.retouch.selected {
            Some(ix) => self.edit_spots("Spot settings", cx, |develop| {
                if let Some(spot) = develop.spots.get_mut(ix) {
                    brush.set(spot, value);
                }
            }),
            None => {
                let ui = &mut self.develop_ui.retouch;
                let mut defaults = Spot {
                    radius: ui.size,
                    feather: ui.feather,
                    opacity: ui.opacity,
                    ..Default::default()
                };
                brush.set(&mut defaults, value);
                (ui.size, ui.feather, ui.opacity) =
                    (defaults.radius, defaults.feather, defaults.opacity);
                cx.notify();
            }
        }
    }

    /// Edits the selected photo's spots as one undoable, logged change.
    fn edit_spots(
        &mut self,
        label: &str,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut DevelopSettings),
    ) {
        self.edit_develop(
            label,
            edit,
            |develop| (String::new(), format!("{} spots", develop.spots.len())),
            cx,
        );
    }

    fn remove_spot(&mut self, index: usize, cx: &mut Context<Self>) {
        let ui = &mut self.develop_ui.retouch;
        ui.drag = None;
        ui.selected = match ui.selected {
            Some(s) if s == index => None,
            Some(s) if s > index => Some(s - 1),
            other => other,
        };
        self.edit_spots("Remove spot", cx, |develop| develop.spots.remove(index));
    }

    /// Delete / Backspace in Develop: removes the selected spot. Returns
    /// whether there was one (otherwise the key keeps its usual meaning).
    pub(crate) fn delete_selected_spot(&mut self, cx: &mut Context<Self>) -> bool {
        match self.develop_ui.retouch.selected {
            Some(ix) if self.develop_ui.retouch.tool.is_some() => {
                self.remove_spot(ix, cx);
                true
            }
            _ => false,
        }
    }

    // --- On the photo ---------------------------------------------------------

    /// Where the photo is on screen, if it's laid out.
    fn retouch_photo_rect(&self) -> Option<Bounds<Pixels>> {
        let size = self.develop_image_size()?;
        self.develop_ui.zoom.photo_rect(size)
    }

    /// A window position as a point on the original photo, if over it.
    fn pointer_on_photo(&self, position: Point<Pixels>, develop: &DevelopSettings) -> (f32, f32) {
        let rect = self.retouch_photo_rect().unwrap_or_default();
        let x = f32::from(position.x - rect.origin.x) / f32::from(rect.size.width).max(1.0);
        let y = f32::from(position.y - rect.origin.y) / f32::from(rect.size.height).max(1.0);
        to_source((x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)), develop)
    }

    /// The overlay drawing the spots and taking clicks, while a tool is
    /// picked and Space isn't held (Space pans instead).
    pub(super) fn render_retouch_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ui = &self.develop_ui.retouch;
        if ui.tool.is_none() || self.develop_ui.zoom.space_held {
            return None;
        }
        let develop = self.catalog.selected_asset()?.develop;
        let rect = self.retouch_photo_rect()?;
        let long_edge = f32::from(rect.size.width.max(rect.size.height));
        let to_screen = move |(u, v): (f32, f32)| {
            let (x, y) = to_display((u, v), &develop);
            point(
                rect.origin.x + px(x * f32::from(rect.size.width)),
                rect.origin.y + px(y * f32::from(rect.size.height)),
            )
        };
        let marks: Vec<_> = develop
            .spots
            .as_slice()
            .iter()
            .enumerate()
            .map(|(ix, spot)| SpotMarks {
                target: to_screen(spot.target),
                source: to_screen(spot.source),
                radius: spot.radius * long_edge,
                selected: ui.selected == Some(ix),
            })
            .collect();
        let brush = ui
            .hover
            .filter(|p| rect.contains(p) && ui.drag.is_none())
            .map(|p| (p, ui.size * long_edge));
        let theme = self.theme;

        Some(
            div()
                .id("retouch-overlay")
                .absolute()
                .inset_0()
                .cursor_crosshair()
                .child(
                    canvas(
                        |_, _, _| {},
                        move |_, _, window, _| paint_marks(window, &marks, brush, &theme),
                    )
                    .size_full(),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        this.retouch_pointer_down(event.position, cx);
                    }),
                )
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _w, cx| {
                    this.develop_ui.retouch.hover = Some(event.position);
                    cx.notify();
                }))
                .into_any_element(),
        )
    }

    /// Picks up a spot's circle under the pointer, or drops a new spot.
    fn retouch_pointer_down(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(develop) = self.catalog.selected_asset().map(|a| a.develop) else {
            return;
        };
        let Some(rect) = self.retouch_photo_rect() else {
            return;
        };
        if !rect.contains(&position) {
            return;
        }
        let long_edge = f32::from(rect.size.width.max(rect.size.height));
        let screen = |p: (f32, f32)| {
            let (x, y) = to_display(p, &develop);
            (
                f32::from(rect.origin.x) + x * f32::from(rect.size.width),
                f32::from(rect.origin.y) + y * f32::from(rect.size.height),
            )
        };
        let (px_, py_) = (f32::from(position.x), f32::from(position.y));
        let within = |centre: (f32, f32), r: f32| {
            (centre.0 - px_).powi(2) + (centre.1 - py_).powi(2) <= r * r
        };

        // Newest first, so the spot drawn on top is the one picked.
        let spots = develop.spots.as_slice();
        for (ix, spot) in spots.iter().enumerate().rev() {
            let r = (spot.radius * long_edge).max(6.0);
            let part = if within(screen(spot.source), r) {
                Some(SpotPart::Source)
            } else if within(screen(spot.target), r) {
                Some(SpotPart::Target)
            } else {
                None
            };
            if let Some(part) = part {
                let ui = &mut self.develop_ui.retouch;
                ui.selected = Some(ix);
                ui.drag = Some(SpotDrag { index: ix, part });
                cx.notify();
                return;
            }
        }
        self.add_spot(self.pointer_on_photo(position, &develop), cx);
    }

    /// Drops a new spot covering `target`, copying from the best-matching
    /// area nearby.
    fn add_spot(&mut self, target: (f32, f32), cx: &mut Context<Self>) {
        let Some(kind) = self.develop_ui.retouch.tool else {
            return;
        };
        let ui = &self.develop_ui.retouch;
        let (radius, feather, opacity) = (ui.size, ui.feather, ui.opacity);
        let source = match &self.source_image {
            Some(image) => rustyedit_processing::find_spot_source(&image.to_rgb8(), target, radius),
            None => ((target.0 + radius * 2.5).min(1.0), target.1),
        };
        let spot = Spot {
            kind,
            target,
            source,
            radius,
            feather,
            opacity,
        };
        let mut added = None;
        self.edit_spots(&format!("Add {} spot", kind.label()), cx, |develop| {
            added = develop.spots.push(spot);
        });
        match added {
            Some(ix) => self.develop_ui.retouch.selected = Some(ix),
            None => {
                self.status_message =
                    Some(format!("A photo can have up to {MAX_SPOTS} spots").into())
            }
        }
    }

    /// Follows a circle drag; `true` while one is in progress.
    pub(super) fn retouch_drag_move(
        &mut self,
        position: Point<Pixels>,
        dragging: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.develop_ui.retouch.drag else {
            return false;
        };
        if !dragging {
            self.develop_ui.retouch.drag = None;
            cx.notify();
            return true;
        }
        let Some(develop) = self.catalog.selected_asset().map(|a| a.develop) else {
            return true;
        };
        let at = self.pointer_on_photo(position, &develop);
        self.develop_ui.retouch.hover = Some(position);
        self.edit_spots("Move spot", cx, |develop| {
            if let Some(spot) = develop.spots.get_mut(drag.index) {
                match drag.part {
                    SpotPart::Target => spot.target = at,
                    SpotPart::Source => spot.source = at,
                }
            }
        });
        true
    }

    pub(super) fn end_retouch_drag(&mut self) {
        self.develop_ui.retouch.drag = None;
    }
}

/// A spot as drawn: both circles in window coordinates.
#[derive(Clone, Copy)]
struct SpotMarks {
    target: Point<Pixels>,
    source: Point<Pixels>,
    radius: f32,
    selected: bool,
}

fn paint_marks(
    window: &mut Window,
    marks: &[SpotMarks],
    brush: Option<(Point<Pixels>, f32)>,
    theme: &Theme,
) {
    let shadow = gpui::black().opacity(0.55);
    let white = gpui::white().opacity(0.95);
    for mark in marks {
        let colour = if mark.selected { theme.accent } else { white };
        // A dark halo under each line keeps it visible on light photos.
        stroke_line(window, mark.source, mark.target, 2.5, shadow, None);
        stroke_line(window, mark.source, mark.target, 1.0, colour, None);
        stroke_circle(window, mark.source, mark.radius, 2.5, shadow, None);
        stroke_circle(window, mark.source, mark.radius, 1.0, colour, Some(4.0));
        stroke_circle(window, mark.target, mark.radius, 3.0, shadow, None);
        stroke_circle(window, mark.target, mark.radius, 1.5, colour, None);
    }
    if let Some((centre, radius)) = brush {
        stroke_circle(window, centre, radius, 2.5, shadow, None);
        stroke_circle(window, centre, radius, 1.0, white.opacity(0.8), None);
    }
}

fn stroke_circle(
    window: &mut Window,
    centre: Point<Pixels>,
    radius: f32,
    width: f32,
    colour: Hsla,
    dash: Option<f32>,
) {
    const STEPS: usize = 64;
    let points: Vec<_> = (0..STEPS)
        .map(|i| {
            let a = i as f32 / STEPS as f32 * std::f32::consts::TAU;
            point(
                centre.x + px(a.cos() * radius),
                centre.y + px(a.sin() * radius),
            )
        })
        .collect();
    let mut path = PathBuilder::stroke(px(width));
    if let Some(dash) = dash {
        path = path.dash_array(&[px(dash), px(dash)]);
    }
    path.add_polygon(&points, true);
    if let Ok(path) = path.build() {
        window.paint_path(path, colour);
    }
}

fn stroke_line(
    window: &mut Window,
    from: Point<Pixels>,
    to: Point<Pixels>,
    width: f32,
    colour: Hsla,
    dash: Option<f32>,
) {
    let mut path = PathBuilder::stroke(px(width));
    if let Some(dash) = dash {
        path = path.dash_array(&[px(dash), px(dash)]);
    }
    path.move_to(from);
    path.line_to(to);
    if let Ok(path) = path.build() {
        window.paint_path(path, colour);
    }
}
