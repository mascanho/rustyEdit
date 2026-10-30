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
    /// Content-aware remove: painting a stroke over what should go, rather
    /// than clicking single spots (the spots it makes are Heal spots).
    pub(crate) remove: bool,
    /// The stroke being painted, in window coordinates.
    stroke: Option<Vec<Point<Pixels>>>,
    /// The working preview as plain RGB, for finding where spots copy
    /// from: made once per photo instead of on every click. Keyed by the
    /// photo and the preview's size.
    spot_rgb: Option<(SpotRgbKey, std::sync::Arc<image::RgbImage>)>,
}

type SpotRgbKey = (Option<u64>, u32, u32);

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
            remove: false,
            stroke: None,
            spot_rgb: None,
        }
    }
}

impl RetouchUi {
    /// Forgets the selection and any drag (another photo was selected).
    pub(crate) fn reset_selection(&mut self) {
        self.selected = None;
        self.drag = None;
        self.stroke = None;
        self.spot_rgb = None;
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
            .child(self.tool_button(SpotKind::Clone, cx))
            .when(self.settings.develop.remove_tool, |el| el.child(self.remove_button(cx)));

        let hint = match ui.tool {
            Some(_) if ui.remove => {
                "Paint over what should go; it's filled from around it \u{00B7} drag the circles to adjust"
            }
            Some(_) => "Click a blemish to cover it \u{00B7} drag the circles to adjust",
            None => "Pick Heal or Clone and click the photo, or Remove and paint over an object",
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
        let active = self.develop_ui.retouch.tool == Some(kind) && !self.develop_ui.retouch.remove;
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

    fn remove_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let active = self.develop_ui.retouch.remove && self.develop_ui.retouch.tool.is_some();
        div()
            .id("retouch-tool-remove")
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .h(sp(30.0))
            .rounded_md()
            .border_1()
            .cursor_pointer()
            .when(active, |el| el.bg(theme.accent_muted).border_color(theme.accent))
            .when(!active, |el| {
                el.border_color(theme.border_subtle)
                    .hover(|el| el.bg(theme.surface_elevated))
            })
            .child(rustyedit_ui::icon(
                rustyedit_ui::Icon::Remove,
                16.0,
                if active { theme.text } else { theme.text_muted },
            ))
            .tooltip(rustyedit_ui::tip(
                "Content-aware remove: paint over an object (Shift+Q)",
                &theme,
            ))
            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                this.toggle_photo_tool(super::tool_strip::PhotoTool::Remove, cx);
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
                format!(
                    "{} {}",
                    if spot.end.is_some() { "Remove" } else { spot.kind.label() },
                    ix + 1
                ),
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

    /// A closed hand while a spot is carried, an open one over a spot that
    /// can be picked up, else the crosshair.
    fn retouch_cursor(&self) -> gpui::CursorStyle {
        let ui = &self.develop_ui.retouch;
        if ui.drag.is_some() {
            return rustyedit_ui::GRABBING_CURSOR;
        }
        match ui.hover {
            Some(at) if self.spot_hit(at).is_some() => rustyedit_ui::GRAB_CURSOR,
            _ => gpui::CursorStyle::Crosshair,
        }
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
                // A stroke's far end, and where it copies that from.
                end: spot.end.map(|end| {
                    let offset = (spot.source.0 - spot.target.0, spot.source.1 - spot.target.1);
                    (to_screen(end), to_screen((end.0 + offset.0, end.1 + offset.1)))
                }),
                radius: spot.radius * long_edge,
                selected: ui.selected == Some(ix),
            })
            .collect();
        let brush = ui
            .hover
            .filter(|p| rect.contains(p) && ui.drag.is_none())
            .map(|p| (p, ui.size * long_edge));
        let stroke = ui
            .stroke
            .clone()
            .map(|points| (points, ui.size * long_edge));
        let theme = self.theme;

        Some(
            div()
                .id("retouch-overlay")
                .absolute()
                .inset_0()
                .cursor(self.retouch_cursor())
                .child(
                    canvas(
                        |_, _, _| {},
                        move |_, _, window, _| {
                            if let Some((points, radius)) = &stroke {
                                paint_stroke(window, points, *radius, &theme);
                            }
                            paint_marks(window, &marks, brush, &theme)
                        },
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

    /// The spot and part (its source or target circle) under `position`,
    /// the newest first so the spot drawn on top is the one picked.
    fn spot_hit(&self, position: Point<Pixels>) -> Option<(usize, SpotPart)> {
        let develop = self.catalog.selected_asset().map(|a| a.develop)?;
        let rect = self.retouch_photo_rect()?;
        if !rect.contains(&position) {
            return None;
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
        // Within `r` of the segment from `a` to `b` (a circle when they're
        // the same point).
        let within = |a: (f32, f32), b: (f32, f32), r: f32| {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len2 = dx * dx + dy * dy;
            let t = if len2 < 1e-6 {
                0.0
            } else {
                (((px_ - a.0) * dx + (py_ - a.1) * dy) / len2).clamp(0.0, 1.0)
            };
            let (cx_, cy_) = (a.0 + dx * t, a.1 + dy * t);
            (cx_ - px_).powi(2) + (cy_ - py_).powi(2) <= r * r
        };

        let spots = develop.spots.as_slice();
        for (ix, spot) in spots.iter().enumerate().rev() {
            let r = (spot.radius * long_edge).max(6.0);
            let end = spot.end.unwrap_or(spot.target);
            let offset = (spot.source.0 - spot.target.0, spot.source.1 - spot.target.1);
            let source_end = (end.0 + offset.0, end.1 + offset.1);
            if within(screen(spot.source), screen(source_end), r) {
                return Some((ix, SpotPart::Source));
            }
            if within(screen(spot.target), screen(end), r) {
                return Some((ix, SpotPart::Target));
            }
        }
        None
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
        if let Some((ix, part)) = self.spot_hit(position) {
            let ui = &mut self.develop_ui.retouch;
            ui.selected = Some(ix);
            ui.drag = Some(SpotDrag { index: ix, part });
            cx.notify();
            return;
        }
        if self.develop_ui.retouch.remove {
            self.develop_ui.retouch.selected = None;
            self.develop_ui.retouch.stroke = Some(vec![position]);
            cx.notify();
            return;
        }
        self.add_spot(self.pointer_on_photo(position, &develop), cx);
    }

    /// The working preview as RGB for source searches, cached per photo.
    fn spot_search_image(&mut self) -> Option<std::sync::Arc<image::RgbImage>> {
        let source = self.source_image.as_ref()?;
        let key = (self.source_image_for, source.width(), source.height());
        let cache = &mut self.develop_ui.retouch.spot_rgb;
        match cache {
            Some((cached, image)) if *cached == key => Some(image.clone()),
            _ => {
                let image = std::sync::Arc::new(source.to_rgb8());
                *cache = Some((key, image.clone()));
                Some(image)
            }
        }
    }

    /// Mouse up after painting a remove stroke: covers it with Heal spots
    /// copying from one matching area beside it, as one undo step.
    pub(crate) fn finish_remove_stroke(&mut self, cx: &mut Context<Self>) {
        let Some(points) = self.develop_ui.retouch.stroke.take() else {
            return;
        };
        cx.notify();
        let Some(develop) = self.catalog.selected_asset().map(|a| a.develop) else {
            return;
        };
        let Some(image) = self.spot_search_image() else {
            self.status_message = Some("The photo is still loading".into());
            return;
        };
        let path: Vec<(f32, f32)> = points
            .iter()
            .map(|&p| self.pointer_on_photo(p, &develop))
            .collect();
        let ui = &self.develop_ui.retouch;
        let (radius, feather, opacity) = (ui.size, ui.feather.min(0.4), ui.opacity);
        let room = MAX_SPOTS - develop.spots.len();
        // A little larger than the brush, so the circles a stroke is made
        // of leave no gaps between them.
        let spot_radius = (radius * 1.1).min(MAX_SIZE);
        let spot = |target: (f32, f32), source: (f32, f32), end: Option<(f32, f32)>| Spot {
            kind: SpotKind::Heal,
            target,
            source,
            end,
            radius: spot_radius,
            feather,
            opacity,
        };
        let spots: Vec<Spot> = match rustyedit_processing::plan_stroke(&image, &path, radius, room) {
            // A few straight pieces sharing one source: a spot each.
            rustyedit_processing::StrokeFill::Segments { points, offset } => points
                .windows(2)
                .map(|pair| {
                    let (a, b) = (pair[0], pair[1]);
                    spot(a, (a.0 + offset.0, a.1 + offset.1), Some(b))
                })
                .collect(),
            rustyedit_processing::StrokeFill::Circles(circles) => circles
                .into_iter()
                .map(|(target, source)| spot(target, source, None))
                .collect(),
        };
        if spots.is_empty() {
            return;
        }
        if spots.len() > room {
            self.status_message = Some(
                format!(
                    "That stroke needs {} spots and {room} of {MAX_SPOTS} are left. \
                     Try a simpler stroke, or clear some spots.",
                    spots.len()
                )
                .into(),
            );
            return;
        }
        self.edit_spots("Remove object", cx, |develop| {
            for spot in spots {
                develop.spots.push(spot);
            }
        });
        self.develop_ui.retouch.selected = None;
    }

    /// Drops a new spot covering `target`, copying from the best-matching
    /// area nearby.
    fn add_spot(&mut self, target: (f32, f32), cx: &mut Context<Self>) {
        let Some(kind) = self.develop_ui.retouch.tool else {
            return;
        };
        let ui = &self.develop_ui.retouch;
        let (radius, feather, opacity) = (ui.size, ui.feather, ui.opacity);
        let source = match self.spot_search_image() {
            Some(image) => rustyedit_processing::find_spot_source(&image, target, radius),
            None => ((target.0 + radius * 2.5).min(1.0), target.1),
        };
        let spot = Spot {
            kind,
            target,
            source,
            end: None,
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
        if let Some(points) = self.develop_ui.retouch.stroke.as_mut() {
            if !dragging {
                self.finish_remove_stroke(cx);
                return true;
            }
            // Skip tiny moves: they add points but no coverage.
            let far_enough = points.last().is_none_or(|last| {
                let (dx, dy) = (f32::from(position.x - last.x), f32::from(position.y - last.y));
                dx * dx + dy * dy >= 4.0
            });
            if far_enough {
                points.push(position);
            }
            self.develop_ui.retouch.hover = Some(position);
            cx.notify();
            return true;
        }
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
                    // A stroke moves whole, keeping its shape.
                    SpotPart::Target => {
                        let delta = (at.0 - spot.target.0, at.1 - spot.target.1);
                        spot.target = at;
                        if let Some(end) = spot.end.as_mut() {
                            *end = (end.0 + delta.0, end.1 + delta.1);
                        }
                    }
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
    /// For a stroke: its far end, and the far end of its source.
    end: Option<(Point<Pixels>, Point<Pixels>)>,
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
        let (end, source_end) = mark.end.unwrap_or((mark.target, mark.source));
        stroke_capsule(window, mark.source, source_end, mark.radius, 2.5, shadow, None);
        stroke_capsule(window, mark.source, source_end, mark.radius, 1.0, colour, Some(4.0));
        stroke_capsule(window, mark.target, end, mark.radius, 3.0, shadow, None);
        stroke_capsule(window, mark.target, end, mark.radius, 1.5, colour, None);
    }
    if let Some((centre, radius)) = brush {
        stroke_circle(window, centre, radius, 2.5, shadow, None);
        stroke_circle(window, centre, radius, 1.0, white.opacity(0.8), None);
    }
}

/// The stroke being painted: a translucent band as wide as the brush.
fn paint_stroke(window: &mut Window, points: &[Point<Pixels>], radius: f32, theme: &Theme) {
    let Some(&first) = points.first() else {
        return;
    };
    let colour = theme.accent.opacity(0.4);
    if points.len() == 1 {
        let ring: Vec<_> = (0..32)
            .map(|i| {
                let a = i as f32 / 32.0 * std::f32::consts::TAU;
                point(first.x + px(a.cos() * radius), first.y + px(a.sin() * radius))
            })
            .collect();
        let mut path = PathBuilder::fill();
        path.add_polygon(&ring, true);
        if let Ok(path) = path.build() {
            window.paint_path(path, colour);
        }
        return;
    }
    let mut path = PathBuilder::stroke(px(radius * 2.0));
    path.move_to(first);
    for &p in &points[1..] {
        path.line_to(p);
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, colour);
    }
}

/// The outline of everything within `radius` of the segment `a`-`b`: a
/// circle when they're the same point.
fn stroke_capsule(
    window: &mut Window,
    a: Point<Pixels>,
    b: Point<Pixels>,
    radius: f32,
    width: f32,
    colour: Hsla,
    dash: Option<f32>,
) {
    let (dx, dy) = (f32::from(b.x - a.x), f32::from(b.y - a.y));
    if dx * dx + dy * dy < 1.0 {
        stroke_circle(window, a, radius, width, colour, dash);
        return;
    }
    const HALF: usize = 24;
    let angle = dy.atan2(dx);
    let half = std::f32::consts::FRAC_PI_2;
    let arc = |centre: Point<Pixels>, from: f32| {
        (0..=HALF).map(move |i| {
            let a = from + i as f32 / HALF as f32 * std::f32::consts::PI;
            point(centre.x + px(a.cos() * radius), centre.y + px(a.sin() * radius))
        })
    };
    let points: Vec<_> = arc(b, angle - half).chain(arc(a, angle + half)).collect();
    let mut path = PathBuilder::stroke(px(width));
    if let Some(dash) = dash {
        path = path.dash_array(&[px(dash), px(dash)]);
    }
    path.add_polygon(&points, true);
    if let Ok(path) = path.build() {
        window.paint_path(path, colour);
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
