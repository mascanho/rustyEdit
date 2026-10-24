//! The Gradients section and its tools: Linear and Radial masks with their
//! own local adjustments.
//!
//! With a tool picked, clicking the photo adds a gradient there (dragging
//! sets its direction or size straight away). The selected gradient shows
//! handles: a linear one's start, end and middle (to move it); a radial
//! one's centre and its two edges. Delete (Backspace) removes it. Holding
//! Space still pans.

use rustyedit_ui::sp;
use gpui::{
    canvas, div, point, prelude::*, px, AnyElement, Bounds, ClickEvent, Context, Hsla, MouseButton,
    MouseDownEvent, PathBuilder, Pixels, Point, SharedString, Window,
};
use rustyedit_core::{
    to_display, to_source, Dab, DevelopSettings, Gradient, GradientKind, LocalAdjust, LocalProp,
    DAB_SPACING, MAX_DABS, MAX_GRADIENTS,
};
use rustyedit_ui::{SliderSpec, SliderTrack, Theme};

use super::adjustments::wheel;
use crate::workspace::Workspace;

/// How close (in screen pixels) the pointer must be to grab a handle.
const HANDLE_REACH: f32 = 9.0;
const HANDLE_SIZE: f32 = 5.0;

/// Brush size when none has been chosen: a fraction of the long edge.
const DEFAULT_BRUSH_SIZE: f32 = 0.05;

/// Tool state for gradient masks.
pub(crate) struct GradientUi {
    /// Radius of new brush dabs, as a fraction of the photo's long edge.
    pub(crate) brush_size: f32,
    /// Paint the selected layer's mask over the photo in red.
    pub(crate) show_mask: bool,
    /// The range picker: which layer it sets and whether it picks the
    /// colour (else the brightness). `pending_pick` carries it into the tool.
    pub(crate) range_pick: Option<(usize, bool)>,
    pub(crate) pending_pick: Option<(usize, bool)>,
    /// The kind clicks on the photo add; `None` leaves the photo alone.
    pub(crate) tool: Option<GradientKind>,
    pub(crate) selected: Option<usize>,
    drag: Option<GradientDrag>,
    /// Where a press on empty photo started. A gradient is only added once
    /// the pointer is dragged from here, so a plain click changes nothing.
    pending: Option<Point<Pixels>>,
}

impl Default for GradientUi {
    fn default() -> Self {
        Self {
            brush_size: DEFAULT_BRUSH_SIZE,
            show_mask: false,
            range_pick: None,
            pending_pick: None,
            tool: None,
            selected: None,
            drag: None,
            pending: None,
        }
    }
}

impl GradientUi {
    pub(crate) fn reset_selection(&mut self) {
        self.selected = None;
        self.drag = None;
        self.pending = None;
    }
}

/// How far the pointer must move before a press becomes a new gradient.
const ADD_DRAG_THRESHOLD: f32 = 4.0;

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
    /// Brush: painting; holds the last dab's place on the photo and
    /// whether the stroke erases.
    Paint((f32, f32), bool),
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
            .child(self.gradient_tool_button(GradientKind::Radial, cx))
            .child(self.gradient_tool_button(GradientKind::Brush, cx))
            .child(self.gradient_tool_button(GradientKind::Range, cx));
        let hint = match ui.tool {
            Some(GradientKind::Linear) => {
                "Drag on the photo from where the effect is full to where it fades out"
            }
            Some(GradientKind::Radial) => "Drag out from the centre of the area to adjust",
            Some(GradientKind::Brush) => "Paint over the area to adjust \u{00B7} hold Alt to erase",
            Some(GradientKind::Range) => "",
            None => "Pick a mask, then drag on the photo (Range needs no dragging)",
        };

        let blur_button = div()
            .id("gradient-background-blur")
            .flex()
            .items_center()
            .justify_center()
            .h(sp(24.0))
            .rounded_md()
            .border_1()
            .border_color(theme.border_subtle)
            .cursor_pointer()
            .text_size(sp(11.0))
            .text_color(theme.text_muted)
            .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
            .tooltip(rustyedit_ui::tip(
                "Keep the middle sharp and blur the area around it, as a wide lens would",
                &theme,
            ))
            .child("Blur background")
            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.add_background_blur(cx)))
            .into_any_element();
        let mut rows = vec![
            tools.into_any_element(),
            blur_button,
            div()
                .text_size(sp(10.0))
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
                    GradientKind::Linear => rustyedit_ui::Icon::LinearGradient,
                    GradientKind::Radial => rustyedit_ui::Icon::RadialGradient,
                    GradientKind::Brush => rustyedit_ui::Icon::Brush,
                    GradientKind::Range => rustyedit_ui::Icon::RangeMask,
                },
                16.0,
                if active { theme.text } else { theme.text_muted },
            ))
            .tooltip(rustyedit_ui::tip(
                match kind {
                    GradientKind::Linear => "Linear gradient (M)",
                    GradientKind::Radial => "Radial gradient (Shift+M)",
                    GradientKind::Brush => "Brush: paint a mask (Alt erases)",
                    GradientKind::Range => "Range: select by brightness and colour",
                },
                &theme,
            ))
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                if kind == GradientKind::Range {
                    this.add_range_layer(cx);
                } else {
                    this.toggle_photo_tool(kind.into(), cx);
                }
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
            .text_size(sp(10.0))
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
                        .text_size(sp(10.5))
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
                        .size(sp(20.0))
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
                if gradient.kind != GradientKind::Range && gradient.has_range() {
                    note.push("range".to_string());
                }

                div()
                    .id(("gradient", ix))
                    .group(group.clone())
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .h(sp(28.0))
                    .px_1()
                    .cursor_pointer()
                    .text_size(sp(11.5))
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
                            GradientKind::Brush => rustyedit_ui::Icon::Brush,
                            GradientKind::Range => rustyedit_ui::Icon::RangeMask,
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
                            .min_w(sp(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(format!("{} {}", gradient.kind.label(), ix + 1)),
                    )
                    .when(!note.is_empty(), |el| {
                        el.child(
                            div()
                                .flex_shrink_0()
                                .text_size(sp(9.5))
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
            .text_size(sp(11.0))
            .text_color(theme.text_muted)
            .child(format!(
                "{} {} \u{00B7} invert",
                gradient.kind.label(),
                index + 1
            ))
            .child(invert)
            .into_any_element()];
        rows.push(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .text_size(sp(11.0))
                .text_color(theme.text_muted)
                .child("Show mask")
                .child(rustyedit_ui::mini_toggle_switch(
                    ("gradient-show-mask", index),
                    self.develop_ui.gradients.show_mask,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| {
                        let ui = &mut this.develop_ui.gradients;
                        ui.show_mask = !ui.show_mask;
                        cx.notify();
                    }),
                ))
                .into_any_element(),
        );

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
                        track: SliderTrack::Amount,
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

        if gradient.kind == GradientKind::Brush {
            rows.extend(self.brush_controls(index, gradient, cx));
        }
        if matches!(gradient.kind, GradientKind::Radial | GradientKind::Brush) {
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

        rows.extend(self.range_controls(index, gradient, cx));

        for prop in LocalProp::ALL {
            let entity = cx.entity();
            let (min, max) = prop.range();
            rows.push(
                rustyedit_ui::labeled_slider(
                    SharedString::from(format!("gradient-{index}-{}", prop.label())),
                    prop.label(),
                    gradient.adjust.get(prop) * prop.display_sign(),
                    SliderSpec {
                        min,
                        max,
                        default: 0.0,
                        signed: min < 0.0,
                        track: local_track(prop),
                    },
                    &theme,
                    move |value, _w, cx| {
                        entity.update(cx, |this, cx| {
                            let label = format!("Gradient {}", prop.label().to_lowercase());
                            this.edit_gradient(index, &label, cx, |g| {
                                g.adjust.set(prop, value * prop.display_sign())
                            });
                        });
                    },
                )
                .into_any_element(),
            );
        }

        // Colour grading for the layer: a tint per tonal range.
        rows.push(
            div()
                .pt_2()
                .text_size(sp(10.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(theme.text_faint)
                .child("COLOUR GRADING")
                .into_any_element(),
        );
        for zone in rustyedit_core::ToneZone::ALL {
            let grade = *gradient.adjust.grading.zone(zone);
            let (entity_h, entity_s) = (cx.entity(), cx.entity());
            rows.push(
                div()
                    .text_size(sp(10.5))
                    .text_color(theme.text_muted)
                    .child(zone.label())
                    .into_any_element(),
            );
            rows.push(
                rustyedit_ui::labeled_slider(
                    SharedString::from(format!("gradient-{index}-{}-hue", zone.label())),
                    "Hue",
                    grade.hue,
                    SliderSpec {
                        min: 0.0,
                        max: 360.0,
                        default: 0.0,
                        signed: false,
                        track: SliderTrack::Hue,
                    },
                    &theme,
                    move |value, _w, cx| {
                        entity_h.update(cx, |this, cx| {
                            this.edit_gradient(index, "Gradient grading hue", cx, |g| {
                                g.adjust.grading.zone_mut(zone).hue = value.clamp(0.0, 360.0)
                            });
                        });
                    },
                )
                .into_any_element(),
            );
            rows.push(
                rustyedit_ui::labeled_slider(
                    SharedString::from(format!("gradient-{index}-{}-sat", zone.label())),
                    "Saturation",
                    grade.saturation,
                    SliderSpec {
                        min: 0.0,
                        max: 100.0,
                        default: 0.0,
                        signed: false,
                        track: SliderTrack::Gradient(
                            gpui::hsla(0.0, 0.0, 0.5, 1.0),
                            wheel(grade.hue, 0.8, 0.5),
                        ),
                    },
                    &theme,
                    move |value, _w, cx| {
                        entity_s.update(cx, |this, cx| {
                            this.edit_gradient(index, "Gradient grading saturation", cx, |g| {
                                g.adjust.grading.zone_mut(zone).saturation =
                                    value.clamp(0.0, 100.0)
                            });
                        });
                    },
                )
                .into_any_element(),
            );
        }
        {
            let entity = cx.entity();
            rows.push(
                rustyedit_ui::labeled_slider(
                    SharedString::from(format!("gradient-{index}-grade-balance")),
                    "Balance",
                    gradient.adjust.grading.balance,
                    SliderSpec {
                        min: -100.0,
                        max: 100.0,
                        default: 0.0,
                        signed: true,
                        track: SliderTrack::Accent,
                    },
                    &theme,
                    move |value, _w, cx| {
                        entity.update(cx, |this, cx| {
                            this.edit_gradient(index, "Gradient grading balance", cx, |g| {
                                g.adjust.grading.balance = value.clamp(-100.0, 100.0)
                            });
                        });
                    },
                )
                .into_any_element(),
            );
        }
        rows
    }

    /// A slider on one field of layer `index`.
    fn gradient_slider(
        &self,
        index: usize,
        id: &'static str,
        label: &'static str,
        value: f32,
        spec: SliderSpec,
        set: fn(&mut Gradient, f32),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entity = cx.entity();
        rustyedit_ui::labeled_slider(
            SharedString::from(format!("gradient-{index}-{id}")),
            label,
            value,
            spec,
            &self.theme,
            move |value, _w, cx| {
                entity.update(cx, |this, cx| {
                    this.edit_gradient(index, &format!("Gradient {id}"), cx, |g| set(g, value));
                });
            },
        )
        .into_any_element()
    }

    /// The brush's size (for new strokes) and a way to start over.
    fn brush_controls(
        &self,
        index: usize,
        gradient: &Gradient,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let theme = self.theme;
        let entity = cx.entity();
        let size = rustyedit_ui::labeled_slider(
            "gradient-brush-size",
            "Brush size",
            self.develop_ui.gradients.brush_size * 100.0,
            SliderSpec {
                min: 0.5,
                max: 30.0,
                default: DEFAULT_BRUSH_SIZE * 100.0,
                signed: false,
                track: SliderTrack::Accent,
            },
            &theme,
            move |value, _w, cx| {
                entity.update(cx, |this, cx| {
                    this.develop_ui.gradients.brush_size = (value / 100.0).clamp(0.005, 0.3);
                    cx.notify();
                });
            },
        )
        .into_any_element();
        let used = gradient.dab_count();
        let footer = div()
            .flex()
            .flex_row()
            .justify_between()
            .text_size(sp(10.0))
            .text_color(theme.text_faint)
            .child(format!("{used} of {MAX_DABS} dabs"))
            .child(
                div()
                    .id(("gradient-brush-clear", index))
                    .cursor_pointer()
                    .hover(|el| el.text_color(theme.danger))
                    .child("Clear strokes")
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.edit_gradient(index, "Clear brush", cx, |g| {
                            while g.dab_count() > 0 {
                                g.pop_dab();
                            }
                        });
                    })),
            )
            .into_any_element();
        vec![size, footer]
    }

    /// Brightness and colour ranges, which narrow any layer to the tones
    /// and colours chosen.
    fn range_controls(
        &self,
        index: usize,
        gradient: &Gradient,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let theme = self.theme;
        let amount = |max: f32, default: f32| SliderSpec {
            min: 0.0,
            max,
            default,
            signed: false,
            track: SliderTrack::Accent,
        };
        let mut rows = vec![div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .pt_2()
            .child(
                div()
                    .text_size(sp(10.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.text_faint)
                    .child("RANGE"),
            )
            .child(self.range_picker_button(index, false, "Pick the brightness from the photo", cx))
            .into_any_element()];
        rows.push(self.gradient_slider(
            index,
            "luma-min",
            "Brightness from",
            gradient.luma_range.0 * 100.0,
            amount(100.0, 0.0),
            |g, v| g.luma_range.0 = (v / 100.0).clamp(0.0, g.luma_range.1.max(0.0)),
            cx,
        ));
        rows.push(self.gradient_slider(
            index,
            "luma-max",
            "Brightness to",
            gradient.luma_range.1 * 100.0,
            amount(100.0, 100.0),
            |g, v| g.luma_range.1 = (v / 100.0).clamp(g.luma_range.0, 1.0),
            cx,
        ));
        rows.push(self.gradient_slider(
            index,
            "luma-feather",
            "Brightness feather",
            gradient.luma_feather * 100.0,
            amount(100.0, 15.0),
            |g, v| g.luma_feather = (v / 100.0).clamp(0.0, 1.0),
            cx,
        ));

        let on = gradient.hue_on;
        rows.push(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .pt_1()
                .text_size(sp(11.0))
                .text_color(theme.text_muted)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child("Colour range")
                        .child(div().size(sp(12.0)).rounded_sm().bg(wheel(gradient.hue, 0.8, 0.5)))
                        .child(self.range_picker_button(index, true, "Pick the colour from the photo", cx)),
                )
                .child(rustyedit_ui::mini_toggle_switch(
                    ("gradient-hue-on", index),
                    on,
                    &theme,
                    cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.edit_gradient(index, "Gradient colour range", cx, |g| {
                            g.hue_on = !g.hue_on
                        });
                    }),
                ))
                .into_any_element(),
        );
        if on {
            let hue = {
                let entity = cx.entity();
                rustyedit_ui::labeled_slider(
                    SharedString::from(format!("gradient-{index}-hue")),
                    "Hue",
                    gradient.hue,
                    SliderSpec {
                        min: 0.0,
                        max: 360.0,
                        default: 0.0,
                        signed: false,
                        track: SliderTrack::Hue,
                    },
                    &theme,
                    move |value, _w, cx| {
                        entity.update(cx, |this, cx| {
                            this.edit_gradient(index, "Gradient range hue", cx, |g| {
                                g.hue = value.clamp(0.0, 360.0)
                            });
                        });
                    },
                )
                .into_any_element()
            };
            rows.push(hue);
            rows.push(self.gradient_slider(
                index,
                "hue-width",
                "Colour width",
                gradient.hue_width,
                SliderSpec {
                    min: 5.0,
                    max: 180.0,
                    default: 30.0,
                    signed: false,
                    track: SliderTrack::Accent,
                },
                |g, v| g.hue_width = v.clamp(5.0, 180.0),
                cx,
            ));
            rows.push(self.gradient_slider(
                index,
                "hue-feather",
                "Colour feather",
                gradient.hue_feather * 100.0,
                amount(100.0, 50.0),
                |g, v| g.hue_feather = (v / 100.0).clamp(0.0, 1.0),
                cx,
            ));
        }
        rows
    }

    /// A small eyedropper that sets layer `index`'s brightness range (or
    /// colour range, with `hue`) from a click on the photo.
    fn range_picker_button(
        &self,
        index: usize,
        hue: bool,
        tip: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let active = self.develop_ui.gradients.range_pick == Some((index, hue));
        div()
            .id(SharedString::from(format!("gradient-pick-{index}-{hue}")))
            .flex()
            .items_center()
            .justify_center()
            .size(sp(20.0))
            .rounded_sm()
            .cursor_pointer()
            .when(active, |el| el.bg(theme.accent_muted))
            .hover(|el| el.bg(theme.surface_elevated))
            .tooltip(rustyedit_ui::tip(tip, &theme))
            .child(rustyedit_ui::icon(
                rustyedit_ui::Icon::Eyedropper,
                13.0,
                if active { theme.accent } else { theme.text_muted },
            ))
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                use super::tool_strip::PhotoTool;
                if active {
                    // Already picking this one: put the eyedropper down.
                    this.toggle_photo_tool(PhotoTool::RangePicker, cx);
                    return;
                }
                this.develop_ui.gradients.pending_pick = Some((index, hue));
                // Picking the other target: put that down first, so the
                // toggle below picks the eyedropper up again for this one.
                if this.active_photo_tool() == Some(PhotoTool::RangePicker) {
                    this.toggle_photo_tool(PhotoTool::RangePicker, cx);
                }
                this.toggle_photo_tool(PhotoTool::RangePicker, cx);
            }))
            .into_any_element()
    }

    /// While a range is being picked: a crosshair over the photo that
    /// samples where it's clicked, and a hint at the top.
    pub(super) fn render_range_pick_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (index, hue) = self.develop_ui.gradients.range_pick?;
        if self.develop_ui.zoom.space_held {
            return None;
        }
        let theme = self.theme;
        let what = if hue { "colour" } else { "brightness" };
        Some(
            div()
                .id("range-pick-overlay")
                .absolute()
                .inset_0()
                .cursor_crosshair()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        this.range_pick_click(event.position, index, hue, cx);
                    }),
                )
                .child(
                    div()
                        .absolute()
                        .top(sp(8.0))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1p5()
                                .h(sp(24.0))
                                .px_2()
                                .rounded_md()
                                .bg(theme.surface.opacity(0.9))
                                .border_1()
                                .border_color(theme.border)
                                .text_size(sp(11.0))
                                .text_color(theme.text)
                                .child(rustyedit_ui::icon(rustyedit_ui::Icon::Eyedropper, 12.0, theme.accent))
                                .child(format!("Click a {what} to select \u{00B7} Esc to cancel")),
                        ),
                )
                .into_any_element(),
        )
    }

    fn range_pick_click(
        &mut self,
        position: Point<Pixels>,
        index: usize,
        hue: bool,
        cx: &mut Context<Self>,
    ) {
        if self.develop_ui.auto_busy {
            return;
        }
        let Some((rect, develop)) = self.gradient_space() else {
            return;
        };
        if !rect.contains(&position) {
            return;
        }
        let at = display_point(position, rect);
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        let id = asset.id;
        let Some(source) = self
            .source_image
            .clone()
            .filter(|_| self.source_image_for == Some(id))
        else {
            self.status_message = Some("Wait for the photo to finish loading.".into());
            cx.notify();
            return;
        };
        self.develop_ui.auto_busy = true;
        cx.notify();
        let work = cx.background_executor().spawn(async move {
            rustyedit_processing::sample_colour(&source, &develop, at)
        });
        cx.spawn(async move |this, cx| {
            let rgb = work.await;
            this.update(cx, |this, cx| {
                this.develop_ui.auto_busy = false;
                if this.catalog.selected_asset().map(|a| a.id) != Some(id) {
                    return;
                }
                let luma = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
                let (max, min) = (rgb[0].max(rgb[1]).max(rgb[2]), rgb[0].min(rgb[1]).min(rgb[2]));
                let hue_degrees = {
                    let d = max - min;
                    if d < 1e-4 {
                        None
                    } else {
                        let h = if max == rgb[0] {
                            ((rgb[1] - rgb[2]) / d).rem_euclid(6.0)
                        } else if max == rgb[1] {
                            (rgb[2] - rgb[0]) / d + 2.0
                        } else {
                            (rgb[0] - rgb[1]) / d + 4.0
                        };
                        Some(h * 60.0)
                    }
                };
                if hue && hue_degrees.is_none() {
                    this.status_message = Some("That's grey: pick something with a colour.".into());
                } else {
                    this.edit_gradient(index, "Pick mask range", cx, |g| {
                        if hue {
                            g.hue_on = true;
                            g.hue = hue_degrees.unwrap_or(g.hue);
                        } else {
                            g.luma_range = ((luma - 0.15).max(0.0), (luma + 0.15).min(1.0));
                            // A range wholly of the extremes keeps its edge at the end.
                            if g.luma_range.0 <= 0.02 {
                                g.luma_range.0 = 0.0;
                            }
                            if g.luma_range.1 >= 0.98 {
                                g.luma_range.1 = 1.0;
                            }
                        }
                    });
                }
                // Done: put the eyedropper down.
                if this.develop_ui.gradients.range_pick.is_some() {
                    this.toggle_photo_tool(super::tool_strip::PhotoTool::RangePicker, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Adds a layer that blurs everything outside a soft oval in the middle
    /// of the photo: a quick lens-blur background. Move and resize it like
    /// any radial, and set how much it blurs with its Blur slider.
    fn add_background_blur(&mut self, cx: &mut Context<Self>) {
        let layer = Gradient {
            kind: GradientKind::Radial,
            start: (0.5, 0.5),
            radius: (0.32, 0.26),
            feather: 0.75,
            invert: true,
            adjust: LocalAdjust {
                blur: 70.0,
                ..Default::default()
            },
            ..Gradient::default()
        };
        let mut added = None;
        self.edit_gradients("Blur background", cx, |d| added = d.gradients.push(layer));
        match added {
            Some(ix) => {
                // The radial tool shows its handles to fit it to the subject.
                if self.active_photo_tool() != Some(super::tool_strip::PhotoTool::Radial) {
                    self.toggle_photo_tool(super::tool_strip::PhotoTool::Radial, cx);
                }
                self.develop_ui.gradients.selected = Some(ix);
                self.status_message = Some("Blur background: drag its handles to fit your subject.".into());
            }
            None => {
                self.status_message =
                    Some(format!("A photo can have up to {MAX_GRADIENTS} gradients").into())
            }
        }
        cx.notify();
    }

    /// Adds a Range layer: a mask that needs no dragging, starting on the
    /// shadows.
    fn add_range_layer(&mut self, cx: &mut Context<Self>) {
        let layer = Gradient {
            kind: GradientKind::Range,
            luma_range: (0.0, 0.4),
            adjust: LocalAdjust {
                exposure: 25.0,
                ..Default::default()
            },
            ..Gradient::default()
        };
        let mut added = None;
        self.edit_gradients("Add range mask", cx, |d| added = d.gradients.push(layer));
        match added {
            Some(ix) => {
                self.develop_ui.gradients.selected = Some(ix);
                self.develop_ui.gradients.tool = None;
            }
            None => {
                self.status_message =
                    Some(format!("A photo can have up to {MAX_GRADIENTS} gradients").into())
            }
        }
        cx.notify();
    }

    /// Starts a brush stroke at `position`: on the selected brush layer, or
    /// on a new one.
    fn begin_brush_stroke(
        &mut self,
        position: Point<Pixels>,
        rect: Bounds<Pixels>,
        develop: &DevelopSettings,
        erase: bool,
        cx: &mut Context<Self>,
    ) {
        let selected_brush = self
            .develop_ui
            .gradients
            .selected
            .filter(|&ix| {
                develop
                    .gradients
                    .as_slice()
                    .get(ix)
                    .is_some_and(|g| g.kind == GradientKind::Brush)
            });
        let index = match selected_brush {
            Some(ix) => ix,
            None => {
                let adjust = self
                    .develop_ui
                    .gradients
                    .selected
                    .and_then(|ix| develop.gradients.as_slice().get(ix))
                    .map(|g| g.adjust)
                    .unwrap_or(LocalAdjust {
                        exposure: 25.0,
                        ..Default::default()
                    });
                let layer = Gradient {
                    kind: GradientKind::Brush,
                    feather: 0.5,
                    adjust,
                    ..Gradient::default()
                };
                let mut added = None;
                self.edit_gradients("Add brush mask", cx, |d| added = d.gradients.push(layer));
                let Some(ix) = added else {
                    self.status_message =
                        Some(format!("A photo can have up to {MAX_GRADIENTS} gradients").into());
                    return;
                };
                ix
            }
        };
        let at = display_point(position, rect);
        let ui = &mut self.develop_ui.gradients;
        ui.selected = Some(index);
        ui.drag = Some(GradientDrag {
            index,
            handle: Handle::Paint(at, erase),
        });
        self.paint_dabs(index, at, at, erase, rect, develop, true, cx);
    }

    /// Lays dabs along the stretch from `from` to `to` (fractions of the
    /// displayed photo) on brush layer `index`.
    fn paint_dabs(
        &mut self,
        index: usize,
        from: (f32, f32),
        to: (f32, f32),
        erase: bool,
        rect: Bounds<Pixels>,
        develop: &DevelopSettings,
        include_start: bool,
        cx: &mut Context<Self>,
    ) {
        let (w, h) = (f32::from(rect.size.width), f32::from(rect.size.height));
        let long = w.max(h).max(1.0);
        let radius = self.develop_ui.gradients.brush_size;
        let spacing = (radius * long * DAB_SPACING).max(1.0);
        let length = ((to.0 - from.0) * w).hypot((to.1 - from.1) * h);
        let steps = (length / spacing).floor() as usize;
        let mut points = Vec::new();
        if include_start {
            points.push(from);
        }
        for i in 1..=steps {
            let t = i as f32 * spacing / length.max(1e-3);
            points.push((from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t));
        }
        if points.is_empty() {
            return;
        }
        let mut full = false;
        self.edit_gradient(index, "Brush stroke", cx, |g| {
            for p in points {
                let (x, y) = to_source(p, develop);
                if !g.add_dab(Dab { x, y, radius, erase }) {
                    full = true;
                    break;
                }
            }
        });
        if full {
            self.status_message = Some(
                format!("The brush layer is full ({MAX_DABS} dabs); start another layer").into(),
            );
        }
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
                        this.gradient_pointer_down(event.position, event.modifiers.alt, cx);
                    }),
                )
                .into_any_element(),
        )
    }

    /// Grabs a handle of the selected gradient under the pointer, selects a
    /// gradient clicked inside, or adds a new one.
    fn gradient_pointer_down(
        &mut self,
        position: Point<Pixels>,
        erase: bool,
        cx: &mut Context<Self>,
    ) {
        let Some((rect, develop)) = self.gradient_space() else {
            return;
        };
        if !rect.contains(&position) {
            return;
        }
        if self.develop_ui.gradients.tool == Some(GradientKind::Brush) {
            self.begin_brush_stroke(position, rect, &develop, erase, cx);
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
        // Wait for a drag before adding anything.
        self.develop_ui.gradients.pending = Some(position);
        cx.notify();
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
        if matches!(kind, GradientKind::Brush | GradientKind::Range) {
            return;
        }
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
                        GradientKind::Radial => Handle::Radius,
                        _ => Handle::End,
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
        if let Some(start) = self.develop_ui.gradients.pending {
            if !dragging {
                // Released without dragging: a click, which adds nothing.
                self.develop_ui.gradients.pending = None;
                return true;
            }
            let (dx, dy) = (
                f32::from(position.x - start.x),
                f32::from(position.y - start.y),
            );
            if (dx * dx + dy * dy).sqrt() < ADD_DRAG_THRESHOLD {
                return true;
            }
            self.develop_ui.gradients.pending = None;
            let Some((rect, develop)) = self.gradient_space() else {
                return true;
            };
            self.add_gradient(start, rect, &develop, cx);
        }
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

        if let Handle::Paint(last, erase) = drag.handle {
            let dist_px = ((at.0 - last.0) * f32::from(rect.size.width))
                .hypot((at.1 - last.1) * f32::from(rect.size.height));
            let spacing = (self.develop_ui.gradients.brush_size * long_edge * DAB_SPACING).max(1.0);
            if dist_px >= spacing {
                // Lay dabs up to the pointer, and remember where the last one went.
                let steps = (dist_px / spacing).floor();
                let t = steps * spacing / dist_px;
                let reached = (last.0 + (at.0 - last.0) * t, last.1 + (at.1 - last.1) * t);
                self.paint_dabs(drag.index, last, at, erase, rect, &develop, false, cx);
                self.develop_ui.gradients.drag = Some(GradientDrag {
                    handle: Handle::Paint(reached, erase),
                    ..drag
                });
            }
            return true;
        }

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
            Handle::Move(_) | Handle::Paint(..) => {}
        });
        true
    }

    pub(super) fn end_gradient_drag(&mut self) {
        self.develop_ui.gradients.drag = None;
        self.develop_ui.gradients.pending = None;
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
    /// Brush dabs on screen: centre, radius and whether it erases.
    dabs: Vec<(Point<Pixels>, f32, bool)>,
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
            dabs: g
                .dabs()
                .map(|d| (at((d.x, d.y)), d.radius * long, d.erase))
                .collect(),
        }
    }

    fn handles(&self) -> Vec<(Point<Pixels>, Handle)> {
        match self.kind {
            GradientKind::Brush | GradientKind::Range => Vec::new(),
            GradientKind::Linear => vec![
                (self.start, Handle::Start),
                (self.end, Handle::End),
                (midpoint(self.start, self.end), Handle::Move((0.0, 0.0))),
            ],
            GradientKind::Radial => vec![
                (self.start, Handle::Move((0.0, 0.0))),
                (
                    self.start + point(px(self.radius.0), sp(0.0)),
                    Handle::RadiusX,
                ),
                (
                    self.start + point(sp(0.0), px(self.radius.1)),
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
            GradientKind::Range => {}
            GradientKind::Brush => {
                // What's been painted, as a soft wash.
                if self.selected {
                    for &(centre, radius, erase) in &self.dabs {
                        let mut path = PathBuilder::fill();
                        path.add_polygon(&ellipse(centre, (radius, radius)), true);
                        if let Ok(path) = path.build() {
                            let wash = if erase {
                                gpui::black().opacity(0.0)
                            } else {
                                theme.accent.opacity(0.10)
                            };
                            window.paint_path(path, wash);
                        }
                    }
                }
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
        path = path.dash_array(&[sp(5.0), sp(4.0)]);
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
