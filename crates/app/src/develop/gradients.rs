//! The Gradients section and its tools: Linear and Radial masks with their
//! own local adjustments.
//!
//! With a tool picked, clicking the photo adds a gradient there (dragging
//! sets its direction or size straight away). The selected gradient shows
//! handles: a linear one's start, end and middle (to move it); a radial
//! one's centre and its two edges. Delete (Backspace) removes it. Holding
//! Space still pans.

use gpui::{
    canvas, div, point, prelude::*, px, AnyElement, Bounds, ClickEvent, Context, Hsla, MouseButton,
    MouseDownEvent, PathBuilder, Pixels, Point, SharedString, Window,
};
use rustyedit_core::{
    to_display, to_source, DevelopSettings, Gradient, GradientKind, LocalAdjust, LocalProp,
    MAX_GRADIENTS,
};
use rustyedit_ui::{SliderSpec, SliderTrack, Theme};

use super::adjustments::wheel;
use crate::workspace::Workspace;

/// How close (in screen pixels) the pointer must be to grab a handle.
const HANDLE_REACH: f32 = 9.0;
const HANDLE_SIZE: f32 = 5.0;

/// Tool state for gradient masks.
#[derive(Default)]
pub(crate) struct GradientUi {
    /// The kind clicks on the photo add; `None` leaves the photo alone.
    pub(crate) tool: Option<GradientKind>,
    pub(crate) selected: Option<usize>,
    drag: Option<GradientDrag>,
}

impl GradientUi {
    pub(crate) fn reset_selection(&mut self) {
        self.selected = None;
        self.drag = None;
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Handle {
    /// Linear: where the effect is full.
    Start,
    /// Linear: where it has faded out.
    End,
    /// The whole gradient; holds the last pointer position on the photo.
    Move((f32, f32)),
    /// Radial: the horizontal or vertical edge.
    RadiusX,
    RadiusY,
    /// Radial, while drawing it: both radii, as a circle.
    Radius,
}

#[derive(Clone, Copy)]
struct GradientDrag {
    index: usize,
    handle: Handle,
}

impl Workspace {
    // --- Panel ----------------------------------------------------------------

    pub(super) fn gradients_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(develop) = self.catalog.selected_asset().map(|a| a.develop) else {
            return Vec::new();
        };
        let ui = &self.develop_ui.gradients;

        let tools = div()
            .flex()
            .flex_row()
            .gap_1()
            .child(self.gradient_tool_button(GradientKind::Linear, cx))
            .child(self.gradient_tool_button(GradientKind::Radial, cx));
        let hint = match ui.tool {
            Some(GradientKind::Linear) => {
                "Drag on the photo from where the effect is full to where it fades out"
            }
            Some(GradientKind::Radial) => "Drag out from the centre of the area to adjust",
            None => "Pick Linear or Radial, then drag on the photo",
        };

        let mut rows = vec![
            tools.into_any_element(),
            div()
                .text_size(px(10.0))
                .text_color(theme.text_faint)
                .child(hint)
                .into_any_element(),
        ];
        rows.extend(self.gradient_list(&develop, cx));
        if let Some((ix, gradient)) = ui
            .selected
            .and_then(|ix| develop.gradients.as_slice().get(ix).map(|g| (ix, *g)))
        {
            rows.extend(self.gradient_controls(ix, &gradient, cx));
        }
        rows
    }

    fn gradient_tool_button(&self, kind: GradientKind, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let active = self.develop_ui.gradients.tool == Some(kind);
        div()
            .id(SharedString::from(format!(
                "gradient-tool-{}",
                kind.label()
            )))
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .h(px(30.0))
            .rounded_md()
            .border_1()
            .cursor_pointer()
            .text_size(px(11.5))
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
                    GradientKind::Linear => rustyedit_ui::Icon::LinearGradient,
                    GradientKind::Radial => rustyedit_ui::Icon::RadialGradient,
                },
                16.0,
                if active { theme.text } else { theme.text_muted },
            ))
            .tooltip(rustyedit_ui::tip(
                match kind {
                    GradientKind::Linear => "Linear gradient (M)",
                    GradientKind::Radial => "Radial gradient (Shift+M)",
                },
                &theme,
            ))
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                this.toggle_photo_tool(kind.into(), cx);
            }))
            .into_any_element()
    }

    /// The gradients as layers, front-most first. Each row has an eye to
    /// show or hide it, its kind, name and opacity, and (on hover) buttons
    /// to reorder, duplicate and delete it. The topmost layer is applied last.
    fn gradient_list(&self, develop: &DevelopSettings, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let gradients = develop.gradients.as_slice();
        if gradients.is_empty() {
            return Vec::new();
        }
        let count = gradients.len();
        let mut rows = vec![div()
            .flex()
            .flex_row()
            .justify_between()
            .pt_1()
            .text_size(px(10.0))
            .text_color(theme.text_faint)
            .child(format!("LAYERS \u{00B7} {count} of {MAX_GRADIENTS}"))
            .child(
                div()
                    .id("gradients-clear")
                    .cursor_pointer()
                    .hover(|el| el.text_color(theme.danger))
                    .child("Clear all")
                    .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                        this.develop_ui.gradients.reset_selection();
                        this.edit_gradients("Clear gradients", cx, |d| d.gradients.clear());
                    })),
            )
            .into_any_element()];

        let layers: Vec<AnyElement> = gradients
            .iter()
            .enumerate()
            .rev()
            .map(|(ix, gradient)| {
                let selected = self.develop_ui.gradients.selected == Some(ix);
                let hidden = gradient.hidden;
                let group = SharedString::from(format!("gradient-layer-{ix}"));
                let action = |id: &'static str,
                              glyph: &'static str,
                              tip: &'static str,
                              danger: bool,
                              on_click: Box<dyn Fn(&mut Workspace, &mut Context<Workspace>)>,
                              cx: &mut Context<Self>|
                 -> AnyElement {
                    let entity = cx.entity();
                    div()
                        .id((id, ix))
                        .flex_shrink_0()
                        .px_1()
                        .text_size(px(10.5))
                        .text_color(theme.text_faint)
                        .cursor_pointer()
                        .hover(|el| el.text_color(if danger { theme.danger } else { theme.text }))
                        .tooltip(rustyedit_ui::tip(tip, &theme))
                        .child(glyph)
                        .on_click(
                            move |_: &ClickEvent, _w: &mut gpui::Window, cx: &mut gpui::App| {
                                cx.stop_propagation();
                                entity.update(cx, |this, cx| on_click(this, cx));
                            },
                        )
                        .into_any_element()
                };

                let eye = {
                    let entity = cx.entity();
                    div()
                        .id(("gradient-eye", ix))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(20.0))
                        .cursor_pointer()
                        .tooltip(rustyedit_ui::tip(
                            if hidden { "Show layer" } else { "Hide layer" },
                            &theme,
                        ))
                        .child(rustyedit_ui::icon(
                            rustyedit_ui::Icon::View,
                            14.0,
                            if hidden {
                                theme.text_faint.opacity(0.5)
                            } else {
                                theme.text_muted
                            },
                        ))
                        .on_click(
                            move |_: &ClickEvent, _w: &mut gpui::Window, cx: &mut gpui::App| {
                                cx.stop_propagation();
                                entity.update(cx, |this, cx| {
                                    this.edit_gradient(ix, "Show or hide gradient", cx, |g| {
                                        g.hidden = !g.hidden
                                    });
                                });
                            },
                        )
                };

                let mut note = Vec::new();
                if gradient.opacity < 99.5 {
                    note.push(format!("{:.0}%", gradient.opacity));
                }
                if gradient.invert {
                    note.push("inverted".to_string());
                }

                div()
                    .id(("gradient", ix))
                    .group(group.clone())
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .h(px(28.0))
                    .px_1()
                    .cursor_pointer()
                    .text_size(px(11.5))
                    .when(selected, |el| {
                        el.bg(theme.accent_muted).text_color(theme.text)
                    })
                    .when(!selected, |el| {
                        el.text_color(theme.text_muted)
                            .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                    })
                    .when(hidden, |el| el.opacity(0.55))
                    .child(eye)
                    .child(rustyedit_ui::icon(
                        match gradient.kind {
                            GradientKind::Linear => rustyedit_ui::Icon::LinearGradient,
                            GradientKind::Radial => rustyedit_ui::Icon::RadialGradient,
                        },
                        14.0,
                        if selected {
                            theme.text
                        } else {
                            theme.text_muted
                        },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(format!("{} {}", gradient.kind.label(), ix + 1)),
                    )
                    .when(!note.is_empty(), |el| {
                        el.child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(9.5))
                                .text_color(theme.text_faint)
                                .child(note.join(" \u{00B7} ")),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .flex_shrink_0()
                            .invisible()
                            .group_hover(group.clone(), |s| s.visible())
                            .child(action(
                                "gradient-up",
                                "\u{25B2}",
                                "Bring forward",
                                false,
                                Box::new(move |this, cx| this.move_gradient(ix, 1, cx)),
                                cx,
                            ))
                            .child(action(
                                "gradient-down",
                                "\u{25BC}",
                                "Send back",
                                false,
                                Box::new(move |this, cx| this.move_gradient(ix, -1, cx)),
                                cx,
                            ))
                            .child(action(
                                "gradient-dup",
                                "\u{29C9}",
                                "Duplicate layer",
                                false,
                                Box::new(move |this, cx| this.duplicate_gradient(ix, cx)),
                                cx,
                            ))
                            .child(action(
                                "gradient-del",
                                "\u{2715}",
                                "Delete layer",
                                true,
                                Box::new(move |this, cx| this.remove_gradient(ix, cx)),
                                cx,
                            )),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.develop_ui.gradients.selected = Some(ix);
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();
        rows.push(
            div()
                .flex()
                .flex_col()
                .bg(theme.background.opacity(0.45))
                .border_1()
                .border_color(theme.border_subtle)
                .children(layers)
                .into_any_element(),
        );
        rows
    }

    /// The selected gradient's mask options and adjustments.
    fn gradient_controls(
        &self,
        index: usize,
        gradient: &Gradient,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let theme = self.theme;
        let invert = rustyedit_ui::mini_toggle_switch(
            ("gradient-invert", index),
            gradient.invert,
            &theme,
            cx.listener(move |this, _: &ClickEvent, _w, cx| {
                this.edit_gradient(index, "Gradient invert", cx, |g| g.invert = !g.invert);
            }),
        );
        let mut rows = vec![div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .pt_2()
            .text_size(px(11.0))
            .text_color(theme.text_muted)
            .child(format!(
                "{} {} \u{00B7} invert",
                gradient.kind.label(),
                index + 1
            ))
            .child(invert)
            .into_any_element()];

        {
            let entity = cx.entity();
            rows.push(
                rustyedit_ui::labeled_slider(
                    SharedString::from(format!("gradient-{index}-opacity")),
                    "Layer opacity",
                    gradient.opacity,
                    SliderSpec {
                        min: 0.0,
                        max: 100.0,
                        default: 100.0,
                        signed: false,
                        track: SliderTrack::Accent,
                    },
                    &theme,
                    move |value, _w, cx| {
                        entity.update(cx, |this, cx| {
                            this.edit_gradient(index, "Gradient opacity", cx, |g| {
                                g.opacity = value.clamp(0.0, 100.0)
                            });
                        });
                    },
                )
                .into_any_element(),
            );
        }

        if gradient.kind == GradientKind::Radial {
            let entity = cx.entity();
            rows.push(
                rustyedit_ui::labeled_slider(
                    SharedString::from(format!("gradient-{index}-feather")),
                    "Feather",
                    gradient.feather * 100.0,
                    SliderSpec {
                        min: 0.0,
                        max: 100.0,
                        default: 50.0,
                        signed: false,
                        track: SliderTrack::Accent,
                    },
                    &theme,
                    move |value, _w, cx| {
                        entity.update(cx, |this, cx| {
                            this.edit_gradient(index, "Gradient feather", cx, |g| {
                                g.feather = (value / 100.0).clamp(0.0, 1.0)
                            });
                        });
                    },
                )
                .into_any_element(),
            );
        }

        for prop in LocalProp::ALL {
            let entity = cx.entity();
            rows.push(
                rustyedit_ui::labeled_slider(
                    SharedString::from(format!("gradient-{index}-{}", prop.label())),
                    prop.label(),
                    gradient.adjust.get(prop),
                    SliderSpec {
                        min: -100.0,
                        max: 100.0,
                        default: 0.0,
                        signed: true,
                        track: local_track(prop),
                    },
                    &theme,
                    move |value, _w, cx| {
                        entity.update(cx, |this, cx| {
                            let label = format!("Gradient {}", prop.label().to_lowercase());
                            this.edit_gradient(index, &label, cx, |g| g.adjust.set(prop, value));
                        });
                    },
                )
                .into_any_element(),
            );
        }
        rows
    }

    fn edit_gradients(
        &mut self,
        label: &str,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut DevelopSettings),
    ) {
        self.edit_develop(
            label,
            edit,
            |develop| {
                (
                    String::new(),
                    format!("{} gradients", develop.gradients.len()),
                )
            },
            cx,
        );
    }

    fn edit_gradient(
        &mut self,
        index: usize,
        label: &str,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut Gradient),
    ) {
        self.edit_gradients(label, cx, |develop| {
            if let Some(gradient) = develop.gradients.get_mut(index) {
                edit(gradient);
            }
        });
    }

    /// Moves a layer towards the front (`1`) or back (`-1`); the selection
    /// follows it.
    fn move_gradient(&mut self, index: usize, delta: i32, cx: &mut Context<Self>) {
        let mut moved_to = None;
        self.edit_gradients("Reorder gradient layers", cx, |d| {
            moved_to = d.gradients.move_by(index, delta);
        });
        if let Some(to) = moved_to {
            let ui = &mut self.develop_ui.gradients;
            ui.selected = match ui.selected {
                Some(s) if s == index => Some(to),
                Some(s) if s == to => Some(index),
                other => other,
            };
        }
    }

    /// Copies a layer in front of itself and selects the copy.
    fn duplicate_gradient(&mut self, index: usize, cx: &mut Context<Self>) {
        let mut copy = None;
        self.edit_gradients("Duplicate gradient layer", cx, |d| {
            copy = d.gradients.duplicate(index);
        });
        match copy {
            Some(ix) => self.develop_ui.gradients.selected = Some(ix),
            None => {
                self.status_message = Some(
                    format!("A photo can have at most {MAX_GRADIENTS} gradient layers.").into(),
                )
            }
        }
        cx.notify();
    }

    fn remove_gradient(&mut self, index: usize, cx: &mut Context<Self>) {
        let ui = &mut self.develop_ui.gradients;
        ui.drag = None;
        ui.selected = match ui.selected {
            Some(s) if s == index => None,
            Some(s) if s > index => Some(s - 1),
            other => other,
        };
        self.edit_gradients("Remove gradient", cx, |d| d.gradients.remove(index));
    }

    /// Delete / Backspace in Develop: removes the selected gradient while
    /// its tool is out. Returns whether it did.
    pub(crate) fn delete_selected_gradient(&mut self, cx: &mut Context<Self>) -> bool {
        match self.develop_ui.gradients.selected {
            Some(ix) if self.develop_ui.gradients.tool.is_some() => {
                self.remove_gradient(ix, cx);
                true
            }
            _ => false,
        }
    }

    // --- On the photo ---------------------------------------------------------

    /// Screen ↔ photo mapping for the current layout.
    fn gradient_space(&self) -> Option<(Bounds<Pixels>, DevelopSettings)> {
        let develop = self.catalog.selected_asset()?.develop;
        let rect = self
            .develop_ui
            .zoom
            .photo_rect(self.develop_image_size()?)?;
        Some((rect, develop))
    }

    pub(super) fn render_gradient_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ui = &self.develop_ui.gradients;
        if ui.tool.is_none() || self.develop_ui.zoom.space_held {
            return None;
        }
        let (rect, develop) = self.gradient_space()?;
        let shapes: Vec<_> = develop
            .gradients
            .as_slice()
            .iter()
            .enumerate()
            .map(|(ix, g)| Shape::new(&g.on_display(&develop), rect, ui.selected == Some(ix)))
            .collect();
        let theme = self.theme;

        Some(
            div()
                .id("gradient-overlay")
                .absolute()
                .inset_0()
                .cursor_crosshair()
                .child(
                    canvas(
                        |_, _, _| {},
                        move |_, _, window, _| {
                            for shape in &shapes {
                                shape.paint(window, &theme);
                            }
                        },
                    )
                    .size_full(),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        this.gradient_pointer_down(event.position, cx);
                    }),
                )
                .into_any_element(),
        )
    }

    /// Grabs a handle of the selected gradient under the pointer, selects a
    /// gradient clicked inside, or adds a new one.
    fn gradient_pointer_down(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some((rect, develop)) = self.gradient_space() else {
            return;
        };
        if !rect.contains(&position) {
            return;
        }
        let on_photo = display_point(position, rect);
        let gradients = develop.gradients.as_slice();

        if let Some(ix) = self.develop_ui.gradients.selected {
            if let Some(g) = gradients.get(ix) {
                let shape = Shape::new(&g.on_display(&develop), rect, true);
                if let Some(handle) = shape.handle_at(position, on_photo) {
                    self.develop_ui.gradients.drag = Some(GradientDrag { index: ix, handle });
                    cx.notify();
                    return;
                }
            }
        }
        for (ix, g) in gradients.iter().enumerate().rev() {
            let shape = Shape::new(&g.on_display(&develop), rect, false);
            if let Some(handle) = shape.handle_at(position, on_photo) {
                let ui = &mut self.develop_ui.gradients;
                ui.selected = Some(ix);
                ui.drag = Some(GradientDrag { index: ix, handle });
                cx.notify();
                return;
            }
        }
        self.add_gradient(position, rect, &develop, cx);
    }

    /// Adds a gradient at `position` with a sensible size, ready for the
    /// drag that follows to set its direction or radius.
    fn add_gradient(
        &mut self,
        position: Point<Pixels>,
        rect: Bounds<Pixels>,
        develop: &DevelopSettings,
        cx: &mut Context<Self>,
    ) {
        let Some(kind) = self.develop_ui.gradients.tool else {
            return;
        };
        let at = display_point(position, rect);
        // Start from the selected gradient's adjustments, or a gentle
        // default that shows straight away what the mask covers.
        let adjust = self
            .develop_ui
            .gradients
            .selected
            .and_then(|ix| develop.gradients.as_slice().get(ix))
            .map(|g| g.adjust)
            .unwrap_or(LocalAdjust {
                exposure: if kind == GradientKind::Linear {
                    -25.0
                } else {
                    25.0
                },
                ..Default::default()
            });
        let gradient = Gradient {
            kind,
            start: to_source(at, develop),
            end: to_source((at.0, (at.1 + 0.25).min(1.0)), develop),
            radius: (0.15, 0.15),
            feather: 0.5,
            invert: false,
            adjust,
            ..Gradient::default()
        };
        let mut added = None;
        self.edit_gradients(&format!("Add {} gradient", kind.label()), cx, |d| {
            added = d.gradients.push(gradient);
        });
        match added {
            Some(ix) => {
                let ui = &mut self.develop_ui.gradients;
                ui.selected = Some(ix);
                ui.drag = Some(GradientDrag {
                    index: ix,
                    handle: match kind {
                        GradientKind::Linear => Handle::End,
                        GradientKind::Radial => Handle::Radius,
                    },
                });
            }
            None => {
                self.status_message =
                    Some(format!("A photo can have up to {MAX_GRADIENTS} gradients").into())
            }
        }
    }

    /// Follows a handle drag; `true` while one is in progress.
    pub(super) fn gradient_drag_move(
        &mut self,
        position: Point<Pixels>,
        dragging: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.develop_ui.gradients.drag else {
            return false;
        };
        if !dragging {
            self.develop_ui.gradients.drag = None;
            cx.notify();
            return true;
        }
        let Some((rect, develop)) = self.gradient_space() else {
            return true;
        };
        let at = display_point(position, rect);
        let long_edge = f32::from(rect.size.width.max(rect.size.height)).max(1.0);
        let quarter_turned = ((develop.rotation_degrees / 90.0).round() as i32).rem_euclid(2) == 1;

        if let Handle::Move(last) = drag.handle {
            self.develop_ui.gradients.drag = Some(GradientDrag {
                handle: Handle::Move(at),
                ..drag
            });
            let delta = (at.0 - last.0, at.1 - last.1);
            self.edit_gradient(drag.index, "Move gradient", cx, |g| {
                let shift = |p: (f32, f32)| {
                    let (x, y) = to_display(p, &develop);
                    to_source((x + delta.0, y + delta.1), &develop)
                };
                g.start = shift(g.start);
                g.end = shift(g.end);
            });
            return true;
        }

        self.edit_gradient(drag.index, "Shape gradient", cx, |g| match drag.handle {
            Handle::Start => g.start = to_source(at, &develop),
            Handle::End => g.end = to_source(at, &develop),
            Handle::Radius => {
                let centre = to_display(g.start, &develop);
                let dx = (at.0 - centre.0) * f32::from(rect.size.width);
                let dy = (at.1 - centre.1) * f32::from(rect.size.height);
                let radius = ((dx * dx + dy * dy).sqrt() / long_edge).clamp(0.01, 1.5);
                g.radius = (radius, radius);
            }
            Handle::RadiusX | Handle::RadiusY => {
                let centre = to_display(g.start, &develop);
                let horizontal = matches!(drag.handle, Handle::RadiusX);
                let distance = if horizontal {
                    (at.0 - centre.0).abs() * f32::from(rect.size.width)
                } else {
                    (at.1 - centre.1).abs() * f32::from(rect.size.height)
                };
                let radius = (distance / long_edge).clamp(0.01, 1.5);
                // Radii are kept on the original photo, whose axes swap
                // with a quarter turn.
                if horizontal != quarter_turned {
                    g.radius.0 = radius;
                } else {
                    g.radius.1 = radius;
                }
            }
            Handle::Move(_) => {}
        });
        true
    }

    pub(super) fn end_gradient_drag(&mut self) {
        self.develop_ui.gradients.drag = None;
    }
}

/// A screen position as a fraction of the displayed photo.
fn display_point(position: Point<Pixels>, rect: Bounds<Pixels>) -> (f32, f32) {
    let x = f32::from(position.x - rect.origin.x) / f32::from(rect.size.width).max(1.0);
    let y = f32::from(position.y - rect.origin.y) / f32::from(rect.size.height).max(1.0);
    (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0))
}

/// A gradient in screen coordinates, for drawing and picking handles.
struct Shape {
    kind: GradientKind,
    start: Point<Pixels>,
    end: Point<Pixels>,
    radius: (f32, f32),
    feather: f32,
    selected: bool,
}

impl Shape {
    /// `g` must already be on the display.
    fn new(g: &Gradient, rect: Bounds<Pixels>, selected: bool) -> Self {
        let (w, h) = (f32::from(rect.size.width), f32::from(rect.size.height));
        let at = |(x, y): (f32, f32)| point(rect.origin.x + px(x * w), rect.origin.y + px(y * h));
        let long = w.max(h);
        Self {
            kind: g.kind,
            start: at(g.start),
            end: at(g.end),
            radius: (g.radius.0 * long, g.radius.1 * long),
            feather: g.feather,
            selected,
        }
    }

    fn handles(&self) -> Vec<(Point<Pixels>, Handle)> {
        match self.kind {
            GradientKind::Linear => vec![
                (self.start, Handle::Start),
                (self.end, Handle::End),
                (midpoint(self.start, self.end), Handle::Move((0.0, 0.0))),
            ],
            GradientKind::Radial => vec![
                (self.start, Handle::Move((0.0, 0.0))),
                (
                    self.start + point(px(self.radius.0), px(0.0)),
                    Handle::RadiusX,
                ),
                (
                    self.start + point(px(0.0), px(self.radius.1)),
                    Handle::RadiusY,
                ),
            ],
        }
    }

    /// The handle under `position`; `on_photo` seeds a move's start.
    fn handle_at(&self, position: Point<Pixels>, on_photo: (f32, f32)) -> Option<Handle> {
        let near = |p: Point<Pixels>| {
            let (dx, dy) = (f32::from(p.x - position.x), f32::from(p.y - position.y));
            dx * dx + dy * dy <= HANDLE_REACH * HANDLE_REACH
        };
        let handle = self
            .handles()
            .into_iter()
            .find(|(p, _)| near(*p))
            .map(|(_, h)| h)
            // Grabbing inside a radial gradient moves it too.
            .or_else(|| {
                (self.kind == GradientKind::Radial && self.contains(position))
                    .then_some(Handle::Move((0.0, 0.0)))
            })?;
        Some(match handle {
            Handle::Move(_) => Handle::Move(on_photo),
            other => other,
        })
    }

    fn contains(&self, position: Point<Pixels>) -> bool {
        let dx = f32::from(position.x - self.start.x) / self.radius.0.max(1.0);
        let dy = f32::from(position.y - self.start.y) / self.radius.1.max(1.0);
        dx * dx + dy * dy <= 1.0
    }

    fn paint(&self, window: &mut Window, theme: &Theme) {
        let colour = if self.selected {
            theme.accent
        } else {
            gpui::white().opacity(0.85)
        };
        let shadow = gpui::black().opacity(0.5);
        let mut both = |path: Vec<Point<Pixels>>, closed: bool, width: f32, dash: bool| {
            stroke(window, &path, closed, width + 1.5, shadow, false);
            stroke(window, &path, closed, width, colour, dash);
        };
        match self.kind {
            GradientKind::Linear => {
                // Lines across the photo at the start, middle and end,
                // perpendicular to the gradient's direction.
                let (dx, dy) = (
                    f32::from(self.end.x - self.start.x),
                    f32::from(self.end.y - self.start.y),
                );
                let length = (dx * dx + dy * dy).sqrt().max(1.0);
                let across = point(px(-dy / length * 4000.0), px(dx / length * 4000.0));
                let line = |at: Point<Pixels>| vec![at - across, at + across];
                both(line(self.start), false, 1.25, false);
                both(line(midpoint(self.start, self.end)), false, 1.0, true);
                both(line(self.end), false, 1.25, false);
                both(vec![self.start, self.end], false, 1.0, false);
            }
            GradientKind::Radial => {
                both(ellipse(self.start, self.radius), true, 1.25, false);
                let inner = 1.0 - self.feather.clamp(0.0, 1.0);
                if inner > 0.02 {
                    let r = (self.radius.0 * inner, self.radius.1 * inner);
                    both(ellipse(self.start, r), true, 1.0, true);
                }
            }
        }
        if self.selected {
            for (p, _) in self.handles() {
                stroke(
                    window,
                    &ellipse(p, (HANDLE_SIZE, HANDLE_SIZE)),
                    true,
                    3.0,
                    shadow,
                    false,
                );
                stroke(
                    window,
                    &ellipse(p, (HANDLE_SIZE, HANDLE_SIZE)),
                    true,
                    1.5,
                    colour,
                    false,
                );
            }
        }
    }
}

fn midpoint(a: Point<Pixels>, b: Point<Pixels>) -> Point<Pixels> {
    point((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
}

fn ellipse(centre: Point<Pixels>, (rx, ry): (f32, f32)) -> Vec<Point<Pixels>> {
    const STEPS: usize = 72;
    (0..STEPS)
        .map(|i| {
            let a = i as f32 / STEPS as f32 * std::f32::consts::TAU;
            centre + point(px(a.cos() * rx), px(a.sin() * ry))
        })
        .collect()
}

fn stroke(
    window: &mut Window,
    points: &[Point<Pixels>],
    closed: bool,
    width: f32,
    colour: Hsla,
    dashed: bool,
) {
    let mut path = PathBuilder::stroke(px(width));
    if dashed {
        path = path.dash_array(&[px(5.0), px(4.0)]);
    }
    path.add_polygon(points, closed);
    if let Ok(path) = path.build() {
        window.paint_path(path, colour);
    }
}

/// Colour ramps for the temperature and tint sliders, as on the globals.
fn local_track(prop: LocalProp) -> SliderTrack {
    match prop {
        LocalProp::Temperature => {
            SliderTrack::Gradient(wheel(215.0, 0.6, 0.55), wheel(48.0, 0.8, 0.55))
        }
        LocalProp::Tint => SliderTrack::Gradient(wheel(120.0, 0.5, 0.5), wheel(300.0, 0.5, 0.55)),
        _ => SliderTrack::Accent,
    }
}
