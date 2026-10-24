//! The Tone Curve section: a square graph of the curve for the chosen
//! channel (RGB master, Red, Green, Blue). Click to add a point, drag a
//! point to move it, double-click one to remove it.

use rustyedit_ui::sp;
use gpui::{
    canvas, div, point, prelude::*, px, AnyElement, Bounds, ClickEvent, Context, Hsla, MouseButton,
    MouseDownEvent, PathBuilder, Pixels, Point, Window,
};
use rustyedit_core::{
    Adjustment, CurveChannel, CurveLook, CurvePreset, HistogramChannel, ParametricCurve, ToneCurve, ToneCurves,
};
use rustyedit_ui::Theme;

use super::adjustments::wheel;
use crate::workspace::Workspace;

/// Largest the graph gets, so a wide panel doesn't make it huge.
const MAX_SIDE: f32 = 260.0;
/// Room the panel's padding takes from its width.
const PANEL_PADDING: f32 = 28.0;
const POINT_SIZE: f32 = 10.0;
/// Line segments the curve is drawn with.
const SAMPLES: usize = 96;

impl Workspace {
    pub(super) fn curve_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(develop) = self.catalog.selected_asset().map(|a| a.develop) else {
            return Vec::new();
        };
        let channel = self.develop_ui.curve_channel;
        let side = (self.settings.layout.inspector_width as f32 - PANEL_PADDING).min(MAX_SIDE);

        let mode_row = self.curve_mode_row(cx);
        if self.develop_ui.curve_parametric {
            let mut rows = vec![mode_row, self.parametric_graph(develop.parametric, side)];
            rows.extend(self.adjustment_sliders(&Adjustment::PARAMETRIC, cx));
            rows.push(
                div()
                    .text_size(sp(9.5))
                    .text_color(self.theme.text_faint)
                    .child("Lift or lower a band of tones. These apply before the point curve.")
                    .into_any_element(),
            );
            return rows;
        }

        vec![
            mode_row,
            div()
                .flex()
                .flex_col()
                .gap_2()
                .py_1()
                .child(self.curve_toolbar(&develop.curves, channel, cx))
                .child(self.curve_graph(develop.curves, channel, side, cx))
                .child(self.curve_presets(channel, cx))
                .child(self.curve_looks(cx))
                .child(self.curve_target_button(channel, cx))
                .child(
                    div()
                        .text_size(sp(9.5))
                        .text_color(self.theme.text_faint)
                        .child(
                            "Click to add a point \u{00B7} drag to move \u{00B7} double-click to remove",
                        ),
                )
                .into_any_element(),
        ]
    }

    /// Point curve or parametric sliders.
    fn curve_mode_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let parametric = self.develop_ui.curve_parametric;
        let tab = |id: &'static str, label: &'static str, active: bool, to: bool| {
            div()
                .id(id)
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .h(sp(22.0))
                .cursor_pointer()
                .text_size(sp(10.5))
                .when(active, |el| {
                    el.bg(theme.accent_muted).text_color(theme.text)
                })
                .when(!active, |el| {
                    el.text_color(theme.text_muted)
                        .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                })
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.develop_ui.curve_parametric = to;
                    this.develop_ui.curve_drag = None;
                    cx.notify();
                }))
        };
        div()
            .flex()
            .flex_row()
            .gap_0p5()
            .p_0p5()
            .bg(theme.background)
            .child(tab("curve-mode-point", "Point curve", !parametric, false))
            .child(tab("curve-mode-parametric", "Parametric", parametric, true))
            .into_any_element()
    }

    /// Ready-made shapes for the channel being edited.
    fn curve_presets(&self, channel: CurveChannel, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_1()
            .children(CurvePreset::ALL.iter().map(|&preset| {
                div()
                    .id(("curve-preset", preset as usize))
                    .flex()
                    .items_center()
                    .h(sp(20.0))
                    .px_1p5()
                    .cursor_pointer()
                    .border_1()
                    .border_color(theme.border_subtle)
                    .bg(theme.background)
                    .text_size(sp(10.0))
                    .text_color(theme.text_muted)
                    .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                    .child(preset.label())
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.edit_curve(channel, cx, |curve| *curve = preset.curve());
                    }))
            }))
            .into_any_element()
    }

    /// Colour looks that set the red, green and blue curves together.
    fn curve_looks(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_size(sp(9.5))
                    .text_color(theme.text_faint)
                    .child("COLOUR LOOKS (all three channels)"),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_1()
                    .children(CurveLook::ALL.iter().map(|&look| {
                        div()
                            .id(("curve-look", look as usize))
                            .flex()
                            .items_center()
                            .h(sp(20.0))
                            .px_1p5()
                            .cursor_pointer()
                            .border_1()
                            .border_color(theme.border_subtle)
                            .bg(theme.background)
                            .text_size(sp(10.0))
                            .text_color(theme.text_muted)
                            .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                            .child(look.label())
                            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                this.edit_develop(
                                    "Curve look",
                                    |d| look.apply(&mut d.curves),
                                    |_| (String::new(), look.label().to_string()),
                                    cx,
                                );
                            }))
                    })),
            )
            .into_any_element()
    }

    /// A read-only plot of the parametric curve with its four bands, over
    /// the photo's histogram like the point curve.
    fn parametric_graph(&self, curve: ParametricCurve, side: f32) -> AnyElement {
        let theme = self.theme;
        // The parametric curve shapes all channels together, so it's read
        // against the master (RGB) histogram.
        let (histogram, _) = self.current_histogram();
        let histogram_channel = histogram_channel(CurveChannel::Master);
        let plot = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                if let Some(histogram) = &histogram {
                    rustyedit_ui::paint_histogram(
                        window,
                        b,
                        rustyedit_ui::HistogramPaint {
                            histogram,
                            channel: histogram_channel,
                            log_scale: false,
                            opacity: 0.45,
                        },
                        &theme,
                    );
                }
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                // Band dividers at the quarter points.
                for i in 1..4 {
                    let x = b.origin.x + px(w * i as f32 / 4.0);
                    let mut line = PathBuilder::stroke(sp(1.0));
                    line.move_to(point(x, b.origin.y));
                    line.line_to(point(x, b.origin.y + px(h)));
                    if let Ok(path) = line.build() {
                        window.paint_path(path, theme.border_subtle);
                    }
                }
                paint_curve(window, b, &ToneCurve::default(), theme.border, 1.0);
                let at = |x: f32| {
                    point(
                        b.origin.x + px(x * w),
                        b.origin.y + px((1.0 - curve.evaluate(x)) * h),
                    )
                };
                let mut path = PathBuilder::stroke(sp(1.75));
                path.move_to(at(0.0));
                for i in 1..=SAMPLES {
                    path.line_to(at(i as f32 / SAMPLES as f32));
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, theme.accent);
                }
            },
        )
        .absolute()
        .size_full();

        div()
            .relative()
            .w(px(side))
            .h(px(side))
            .mx_auto()
            .bg(theme.background)
            .border_1()
            .border_color(theme.border_subtle)
            .child(plot)
            .into_any_element()
    }

    /// Channel tabs, and a reset for the channel shown when it's edited.
    fn curve_toolbar(
        &self,
        curves: &ToneCurves,
        channel: CurveChannel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let tabs = CurveChannel::ALL.iter().map(|&tab| {
            let active = tab == channel;
            let edited = !curves.get(tab).is_identity();
            div()
                .id(("curve-channel", tab as usize))
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .h(sp(20.0))
                .px_2()
                .rounded_sm()
                .cursor_pointer()
                .text_size(sp(10.5))
                .when(active, |el| {
                    el.bg(theme.surface_elevated).text_color(theme.text)
                })
                .when(!active, |el| {
                    el.text_color(theme.text_muted)
                        .hover(|el| el.text_color(theme.text))
                })
                .child(
                    div()
                        .size(sp(6.0))
                        .rounded_full()
                        .bg(channel_color(tab, &theme)),
                )
                .child(tab.label())
                // A dot marks channels that have a curve.
                .when(edited && !active, |el| {
                    el.child(div().size(sp(3.0)).rounded_full().bg(theme.accent))
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.develop_ui.curve_channel = tab;
                    cx.notify();
                }))
        });

        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_0p5()
                    .p_0p5()
                    .rounded_md()
                    .bg(theme.background)
                    .children(tabs),
            )
            .when(!curves.get(channel).is_identity(), |el| {
                el.child(
                    div()
                        .id("curve-reset")
                        .text_size(sp(10.0))
                        .text_color(theme.text_muted)
                        .cursor_pointer()
                        .hover(|el| el.text_color(theme.accent))
                        .child("Reset")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            this.edit_curve(channel, cx, |curve| *curve = ToneCurve::default());
                        })),
                )
            })
            .into_any_element()
    }

    /// The graph: the histogram behind it, the grid, the other channels'
    /// curves faintly, the chosen channel's curve and its draggable points.
    fn curve_graph(
        &self,
        curves: ToneCurves,
        channel: CurveChannel,
        side: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let bounds = self.develop_ui.curve_bounds.clone();
        let active_color = channel_color(channel, &theme);
        // The live histogram of the photo, so the curve is read against
        // the tones it is actually shaping.
        let (histogram, _) = self.current_histogram();
        let histogram_channel = histogram_channel(channel);

        let grid = (1..4).flat_map(|i| {
            let at = px(side * i as f32 / 4.0);
            [
                div().absolute().left(at).top_0().bottom_0().w(sp(1.0)),
                div().absolute().top(at).left_0().right_0().h(sp(1.0)),
            ]
            .map(|line| line.bg(theme.border_subtle.opacity(0.6)))
        });

        let lines = canvas(
            move |b, _, _| bounds.set(b),
            move |b, _, window, _| {
                if let Some(histogram) = &histogram {
                    rustyedit_ui::paint_histogram(
                        window,
                        b,
                        rustyedit_ui::HistogramPaint {
                            histogram,
                            channel: histogram_channel,
                            log_scale: false,
                            opacity: 0.45,
                        },
                        &theme,
                    );
                }
                // The untouched diagonal, then other edited channels, then
                // the one being edited on top.
                paint_curve(window, b, &ToneCurve::default(), theme.border, 1.0);
                for other in CurveChannel::ALL.into_iter().filter(|c| *c != channel) {
                    let curve = curves.get(other);
                    if !curve.is_identity() {
                        let color = channel_color(other, &theme).opacity(0.45);
                        paint_curve(window, b, curve, color, 1.0);
                    }
                }
                paint_curve(window, b, curves.get(channel), active_color, 1.75);
            },
        )
        .absolute()
        .size_full();

        let points = curves
            .get(channel)
            .points()
            .iter()
            .enumerate()
            .map(|(ix, [x, y])| {
                let dragging = self.develop_ui.curve_drag == Some(ix);
                div()
                    .id(("curve-point", ix))
                    .absolute()
                    .left(px(x * side - POINT_SIZE / 2.0))
                    .top(px((1.0 - y) * side - POINT_SIZE / 2.0))
                    .size(px(POINT_SIZE))
                    .rounded_full()
                    .bg(if dragging {
                        active_color
                    } else {
                        theme.surface
                    })
                    .border_2()
                    .border_color(active_color)
                    .cursor_grab()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            if event.click_count >= 2 {
                                this.develop_ui.curve_drag = None;
                                this.edit_curve(channel, cx, |curve| curve.remove(ix));
                            } else {
                                this.develop_ui.curve_drag = Some(ix);
                                cx.notify();
                            }
                        }),
                    )
            });

        div()
            .id("curve-graph")
            .relative()
            .w(px(side))
            .h(px(side))
            .mx_auto()
            .rounded_md()
            .bg(theme.background)
            .border_1()
            .border_color(theme.border_subtle)
            .cursor_crosshair()
            .children(grid)
            .child(lines)
            .children(points)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _w, cx| {
                    this.add_curve_point(channel, event.position, cx);
                }),
            )
            .into_any_element()
    }

    /// Adds a point under `position` and starts dragging it.
    fn add_curve_point(
        &mut self,
        channel: CurveChannel,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some((x, y)) = self.curve_position(position) else {
            return;
        };
        let mut added = None;
        self.edit_curve(channel, cx, |curve| added = curve.insert(x, y));
        self.develop_ui.curve_drag = added;
    }

    /// Follows a point drag; `true` while one is in progress.
    pub(super) fn curve_drag_move(
        &mut self,
        position: Point<Pixels>,
        dragging: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(ix) = self.develop_ui.curve_drag else {
            return false;
        };
        if !dragging {
            self.develop_ui.curve_drag = None;
            cx.notify();
            return true;
        }
        if let Some((x, y)) = self.curve_position(position) {
            let channel = self.develop_ui.curve_channel;
            self.edit_curve(channel, cx, |curve| curve.move_point(ix, x, y));
        }
        true
    }

    /// A window position as curve coordinates (0..1, y up).
    fn curve_position(&self, position: Point<Pixels>) -> Option<(f32, f32)> {
        let b = self.develop_ui.curve_bounds.get();
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let x = (f32::from(position.x - b.origin.x) / w).clamp(0.0, 1.0);
        let y = 1.0 - (f32::from(position.y - b.origin.y) / h).clamp(0.0, 1.0);
        Some((x, y))
    }

    fn edit_curve(
        &mut self,
        channel: CurveChannel,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut ToneCurve),
    ) {
        let label = match channel {
            CurveChannel::Master => "Tone curve",
            CurveChannel::Red => "Red curve",
            CurveChannel::Green => "Green curve",
            CurveChannel::Blue => "Blue curve",
        };
        self.edit_develop(
            label,
            |develop| edit(develop.curves.get_mut(channel)),
            |develop| {
                let points = develop.curves.get(channel).points().len();
                (String::new(), format!("{points} points"))
            },
            cx,
        );
    }
}

fn channel_color(channel: CurveChannel, theme: &Theme) -> Hsla {
    match channel {
        CurveChannel::Master => theme.text,
        CurveChannel::Red => wheel(0.0, 0.75, 0.6),
        CurveChannel::Green => wheel(120.0, 0.6, 0.5),
        CurveChannel::Blue => wheel(215.0, 0.8, 0.62),
    }
}

/// Which distribution the histogram behind the curve shows: the master
/// curve reads the photo as a whole, the others their own channel.
fn histogram_channel(channel: CurveChannel) -> HistogramChannel {
    match channel {
        CurveChannel::Master => HistogramChannel::Rgb,
        CurveChannel::Red => HistogramChannel::Red,
        CurveChannel::Green => HistogramChannel::Green,
        CurveChannel::Blue => HistogramChannel::Blue,
    }
}

/// Strokes `curve` across `bounds` (input left to right, output bottom to
/// top).
fn paint_curve(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    curve: &ToneCurve,
    color: Hsla,
    width: f32,
) {
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let at = |x: f32| {
        let y = curve.evaluate(x);
        point(
            bounds.origin.x + px(x * w),
            bounds.origin.y + px((1.0 - y) * h),
        )
    };
    let mut path = PathBuilder::stroke(px(width));
    path.move_to(at(0.0));
    for i in 1..=SAMPLES {
        path.line_to(at(i as f32 / SAMPLES as f32));
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

impl Workspace {
    /// The eyedropper that adds a curve point from a tone in the photo.
    fn curve_target_button(&self, channel: CurveChannel, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let active = self.develop_ui.curve_target;
        div()
            .id("curve-target")
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .h(sp(24.0))
            .px_2()
            .rounded_md()
            .border_1()
            .cursor_pointer()
            .text_size(sp(10.5))
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
            .tooltip(rustyedit_ui::tip(
                "Click a tone in the photo to put a point on the curve for it",
                &theme,
            ))
            .child(rustyedit_ui::icon(
                rustyedit_ui::Icon::Eyedropper,
                13.0,
                if active { theme.accent } else { theme.text_muted },
            ))
            .child(format!("Target a tone ({})", channel.label()))
            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                this.toggle_photo_tool(super::tool_strip::PhotoTool::CurveTarget, cx);
            }))
            .into_any_element()
    }

    /// While the target picker is out: a crosshair over the photo that adds
    /// a curve point for the tone where it's clicked, and a hint.
    pub(super) fn render_curve_target_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.develop_ui.curve_target || self.develop_ui.zoom.space_held {
            return None;
        }
        let theme = self.theme;
        Some(
            div()
                .id("curve-target-overlay")
                .absolute()
                .inset_0()
                .cursor_crosshair()
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, event: &gpui::MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        this.curve_target_click(event.position, cx);
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
                                .child("Click a tone to add a curve point for it \u{00B7} Esc to cancel"),
                        ),
                )
                .into_any_element(),
        )
    }

    fn curve_target_click(&mut self, position: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        if self.develop_ui.auto_busy {
            return;
        }
        let Some(size) = self.develop_image_size() else {
            return;
        };
        let Some(rect) = self.develop_ui.zoom.photo_rect(size) else {
            return;
        };
        if !rect.contains(&position) {
            return;
        }
        let at = (
            f32::from(position.x - rect.origin.x) / f32::from(rect.size.width),
            f32::from(position.y - rect.origin.y) / f32::from(rect.size.height),
        );
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        let (id, develop) = (asset.id, asset.develop);
        let channel = self.develop_ui.curve_channel;
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
        // The tone as it is before the curve, so the point lands where the
        // curve will act on it.
        let work = cx.background_executor().spawn(async move {
            rustyedit_processing::sample_colour_without(
                &source,
                &develop,
                at,
                &[rustyedit_core::DevelopSection::ToneCurve, rustyedit_core::DevelopSection::Gradients],
            )
        });
        cx.spawn(async move |this, cx| {
            let rgb = work.await;
            this.update(cx, |this, cx| {
                this.develop_ui.auto_busy = false;
                if this.catalog.selected_asset().map(|a| a.id) != Some(id) {
                    return;
                }
                let tone = curve_target_tone(&develop, channel, rgb);
                let mut added = false;
                this.edit_curve(channel, cx, |curve| {
                    let y = curve.evaluate(tone);
                    added = curve.insert(tone, y).is_some();
                });
                this.status_message = Some(
                    if added {
                        format!("Added a {} curve point at {:.0}%.", channel.label(), tone * 100.0)
                    } else {
                        "There's already a point (or no room for one) there.".to_string()
                    }
                    .into(),
                );
                // Done: put the eyedropper down.
                if this.develop_ui.curve_target {
                    this.toggle_photo_tool(super::tool_strip::PhotoTool::CurveTarget, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

/// Where on `channel`'s curve the colour `rgb` (before any curve) falls:
/// its brightness for the master curve, its own value for a channel's. The
/// parametric sliders (and, for a channel, the master curve) come first in
/// the pipeline, so they're applied.
fn curve_target_tone(
    develop: &rustyedit_core::DevelopSettings,
    channel: CurveChannel,
    rgb: [f32; 3],
) -> f32 {
    let value = match channel {
        CurveChannel::Master => 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2],
        CurveChannel::Red => rgb[0],
        CurveChannel::Green => rgb[1],
        CurveChannel::Blue => rgb[2],
    }
    .clamp(0.0, 1.0);
    let shaped = develop.parametric.evaluate(value);
    match channel {
        CurveChannel::Master => shaped.clamp(0.0, 1.0),
        _ => develop.curves.get(CurveChannel::Master).evaluate(shaped).clamp(0.0, 1.0),
    }
}

#[cfg(test)]
mod target_tests {
    use super::*;
    use rustyedit_core::DevelopSettings;

    #[test]
    fn a_plain_photo_targets_its_own_tone() {
        let develop = DevelopSettings::default();
        let t = curve_target_tone(&develop, CurveChannel::Master, [0.5, 0.5, 0.5]);
        assert!((t - 0.5).abs() < 0.01, "{t}");
        let t = curve_target_tone(&develop, CurveChannel::Red, [0.8, 0.2, 0.2]);
        assert!((t - 0.8).abs() < 0.01, "{t}");
    }

    #[test]
    fn a_channel_target_goes_through_the_master_curve_first() {
        let mut develop = DevelopSettings::default();
        develop.curves.master = rustyedit_core::CurvePreset::Brighten.curve();
        let plain = curve_target_tone(&DevelopSettings::default(), CurveChannel::Green, [0.5; 3]);
        let lifted = curve_target_tone(&develop, CurveChannel::Green, [0.5; 3]);
        assert!(lifted > plain + 0.05, "{plain} {lifted}");
        // The master curve's own target isn't lifted by itself.
        let master = curve_target_tone(&develop, CurveChannel::Master, [0.5; 3]);
        assert!((master - 0.5).abs() < 0.01);
    }
}

