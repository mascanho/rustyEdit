//! The histogram panel at the top of the right-hand panels, always shown:
//!
//! - channel tabs (RGB, Luminance, Red, Green, Blue) and a log-scale toggle;
//! - shadow and highlight clipping warnings in the corners, coloured by
//!   the channels that clip; in Develop, clicking one (or pressing J)
//!   paints the clipped pixels on the photo;
//! - a readout of the level, zone and share of pixels under the pointer;
//! - in Develop, dragging across the graph adjusts the Basic slider of the
//!   zone under the pointer (Blacks, Shadows, Exposure, Highlights,
//!   Whites), as in Lightroom;
//! - mean brightness, clipping percentages and the capture settings.

use rustyedit_ui::sp;
use std::{cell::Cell, rc::Rc, sync::Arc};

use gpui::{
    div, hsla, prelude::*, AnyElement, Bounds, ClickEvent, Context, Hsla, MouseButton,
    MouseDownEvent, MouseMoveEvent, Pixels, Point,
};
use rustyedit_core::{Clipping, Histogram, HistogramChannel, ScopeMode, TonalZone};
use rustyedit_ui::{HistogramProps, Icon, Theme};

use crate::workspace::{Mode, Workspace};

/// View state of the histogram panel.
#[derive(Default)]
pub(crate) struct HistogramUi {
    pub(crate) channel: HistogramChannel,
    /// Histogram, waveform, RGB parade or vectorscope.
    pub(crate) mode: ScopeMode,
    pub(crate) log_scale: bool,
    /// The level under the pointer, as a fraction of the range.
    hover: Option<f32>,
    drag: Option<ZoneDrag>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Paint clipped pixels on the photo in Develop.
    pub(crate) show_shadow_clipping: bool,
    pub(crate) show_highlight_clipping: bool,
}

/// Dragging across the graph to adjust a zone's slider.
#[derive(Clone, Copy)]
struct ZoneDrag {
    zone: TonalZone,
    start_x: Pixels,
    start_value: f32,
}

impl Workspace {
    pub(crate) fn render_histogram_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let ui = &self.histogram_ui;
        let (histogram, source) = self.current_histogram();
        // Only the histogram graph can be dragged to adjust tones.
        let editable = self.mode == Mode::Develop && ui.mode == ScopeMode::Histogram;
        let zone = ui
            .drag
            .map(|drag| drag.zone)
            .or_else(|| ui.hover.filter(|_| editable).map(TonalZone::at));

        let graph = div()
            .id("histogram-graph")
            .relative()
            .w_full()
            .when(editable, |el| el.cursor_ew_resize())
            .child(if ui.mode == ScopeMode::Histogram {
                rustyedit_ui::histogram_view(
                    &theme,
                    HistogramProps {
                        histogram: histogram.clone(),
                        channel: ui.channel,
                        log_scale: ui.log_scale,
                        hover: ui.hover,
                        zone,
                        bounds: Some(ui.bounds.clone()),
                    },
                )
                .into_any_element()
            } else {
                rustyedit_ui::scope_view(&theme, histogram.clone(), ui.mode, ui.channel)
                    .into_any_element()
            })
            .children(histogram.as_ref().map(|h| self.clipping_badges(h, cx)))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _w, cx| {
                if this.histogram_ui.mode != ScopeMode::Histogram {
                    return;
                }
                let level = this.histogram_level(event.position);
                if this.histogram_ui.hover != level {
                    this.histogram_ui.hover = level;
                    cx.notify();
                }
            }))
            .on_hover(cx.listener(|this, hovered: &bool, _w, cx| {
                if !hovered && this.histogram_ui.drag.is_none() {
                    this.histogram_ui.hover = None;
                    cx.notify();
                }
            }))
            .when(editable, |el| {
                el.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, _w, cx| {
                        this.start_zone_drag(event.position, cx);
                    }),
                )
            });

        div()
            .flex()
            .flex_col()
            .gap_1p5()
            .child(self.histogram_toolbar(cx))
            .child(graph)
            .child(self.histogram_readout(histogram.as_deref(), source, zone))
            .children(self.capture_line())
            .into_any_element()
    }

    /// The view switcher, then (where they apply) the channel tabs on the
    /// left and the log toggle on the right.
    fn histogram_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let current = self.histogram_ui.channel;
        let mode = self.histogram_ui.mode;

        let modes = ScopeMode::ALL.iter().map(|&option| {
            let active = option == mode;
            div()
                .id(("histogram-mode", option as usize))
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .h(sp(20.0))
                .cursor_pointer()
                .text_size(sp(10.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .when(active, |el| {
                    el.bg(theme.accent_muted).text_color(theme.text)
                })
                .when(!active, |el| {
                    el.text_color(theme.text_faint)
                        .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                })
                .child(match option {
                    ScopeMode::Histogram => "Histogram",
                    ScopeMode::Waveform => "Wave",
                    ScopeMode::Parade => "Parade",
                    ScopeMode::Vectorscope => "Vector",
                })
                .tooltip(rustyedit_ui::tip(
                    format!("{}: {}", option.label(), option.description()),
                    &theme,
                ))
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.histogram_ui.mode = option;
                    this.histogram_ui.hover = None;
                    cx.notify();
                }))
        });
        let mode_row = div()
            .flex()
            .flex_row()
            .gap_0p5()
            .p_0p5()
            .bg(theme.background)
            .children(modes);

        let has_channels = matches!(mode, ScopeMode::Histogram | ScopeMode::Waveform);
        let tabs = HistogramChannel::ALL.iter().map(|&channel| {
            let active = channel == current;
            div()
                .id(("histogram-channel", channel as usize))
                .flex()
                .items_center()
                .justify_center()
                .min_w(sp(22.0))
                .h(sp(18.0))
                .px_1()
                .cursor_pointer()
                .text_size(sp(10.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(if active {
                    channel_tint(channel, &theme)
                } else {
                    theme.text_faint
                })
                .when(active, |el| el.bg(theme.surface_elevated))
                .hover(|el| el.text_color(theme.text))
                .child(channel.label())
                .tooltip(rustyedit_ui::tip(channel.name(), &theme))
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.histogram_ui.channel = channel;
                    cx.notify();
                }))
        });

        let log = self.histogram_ui.log_scale;
        let channel_row = div()
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
                    .bg(theme.background)
                    .children(tabs),
            )
            .when(mode == ScopeMode::Histogram, |el| {
                el.child(
                    div()
                        .id("histogram-log")
                        .h(sp(18.0))
                        .px_1p5()
                        .flex()
                        .items_center()
                        .border_1()
                        .cursor_pointer()
                        .text_size(sp(10.0))
                        .when(log, |el| {
                            el.border_color(theme.accent)
                                .text_color(theme.accent)
                                .bg(theme.accent.opacity(0.1))
                        })
                        .when(!log, |el| {
                            el.border_color(theme.border_subtle)
                                .text_color(theme.text_faint)
                                .hover(|el| el.text_color(theme.text))
                        })
                        .child("Log")
                        .tooltip(rustyedit_ui::tip(
                            "Logarithmic scale: shows sparse tones next to a tall peak",
                            &theme,
                        ))
                        .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                            this.histogram_ui.log_scale = !this.histogram_ui.log_scale;
                            cx.notify();
                        })),
                )
            });

        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(mode_row)
            .when(has_channels, |el| el.child(channel_row))
            .into_any_element()
    }

    /// The two corner warnings, lit in the colour of the channels that clip.
    fn clipping_badges(&self, histogram: &Histogram, cx: &mut Context<Self>) -> AnyElement {
        let shadows = histogram.shadow_clipping();
        let highlights = histogram.highlight_clipping();
        div()
            .absolute()
            .top(sp(4.0))
            .left(sp(4.0))
            .right(sp(4.0))
            .flex()
            .flex_row()
            .justify_between()
            .child(self.clipping_badge(true, shadows, cx))
            .child(self.clipping_badge(false, highlights, cx))
            .into_any_element()
    }

    fn clipping_badge(
        &self,
        shadows: bool,
        clipping: Clipping,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let ui = &self.histogram_ui;
        let showing = if shadows {
            ui.show_shadow_clipping
        } else {
            ui.show_highlight_clipping
        };
        let color = clip_color(clipping, &theme);
        let what = if shadows { "Shadow" } else { "Highlight" };
        let tip = format!(
            "{what} clipping: {}{}",
            percent(clipping.worst()),
            if self.mode == Mode::Develop {
                " \u{00B7} click (or J) to show on the photo"
            } else {
                ""
            }
        );

        div()
            .id(if shadows {
                "clip-shadows"
            } else {
                "clip-highlights"
            })
            .p(sp(2.0))
            .cursor_pointer()
            .when(showing, |el| {
                el.bg(theme.surface_elevated)
                    .border_1()
                    .border_color(theme.accent)
            })
            .hover(|el| el.bg(theme.surface_elevated))
            .child(rustyedit_ui::icon(
                if shadows {
                    Icon::ClipShadows
                } else {
                    Icon::ClipHighlights
                },
                10.0,
                color,
            ))
            .tooltip(rustyedit_ui::tip(tip, &theme))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                if this.mode != Mode::Develop {
                    return;
                }
                let ui = &mut this.histogram_ui;
                if shadows {
                    ui.show_shadow_clipping = !ui.show_shadow_clipping;
                } else {
                    ui.show_highlight_clipping = !ui.show_highlight_clipping;
                }
                this.request_preview_update(cx);
                cx.notify();
            }))
            .into_any_element()
    }

    /// Under the graph: what's under the pointer, or the overall figures.
    fn histogram_readout(
        &self,
        histogram: Option<&Histogram>,
        source: &'static str,
        zone: Option<TonalZone>,
    ) -> AnyElement {
        let theme = self.theme;
        let ui = &self.histogram_ui;
        let row = || {
            div()
                .flex()
                .flex_row()
                .justify_between()
                .gap_2()
                .text_size(sp(10.0))
                .text_color(theme.text_faint)
        };
        let Some(histogram) = histogram else {
            let waiting = if self.catalog.selected_asset().is_some() {
                "Calculating\u{2026}"
            } else {
                ""
            };
            return row().child(waiting).into_any_element();
        };

        if let Some(drag) = ui.drag {
            let value = self
                .catalog
                .selected_asset()
                .map(|a| drag.zone.adjustment().get(&a.develop))
                .unwrap_or(0.0);
            return row()
                .child(
                    div()
                        .text_color(theme.accent)
                        .child(format!("{} {value:+.0}", drag.zone.label())),
                )
                .child("drag left or right")
                .into_any_element();
        }

        if let Some(level) = ui.hover {
            let bin = (level * 255.0).round() as usize;
            let left = match zone {
                Some(zone) => format!("{} \u{00B7} level {bin}", zone.label()),
                None => format!("Level {bin}"),
            };
            return row()
                .child(div().text_color(theme.text_muted).child(left))
                .child(format!(
                    "{} of pixels",
                    percent(histogram.share_at(ui.channel, bin))
                ))
                .into_any_element();
        }

        let shadows = histogram.shadow_clipping().worst();
        let highlights = histogram.highlight_clipping().worst();
        row()
            .child(format!(
                "{source} \u{00B7} mean {:.0}",
                histogram.mean_luma()
            ))
            .child(format!(
                "clipped {} / {}",
                percent(shadows),
                percent(highlights)
            ))
            .into_any_element()
    }

    /// ISO, focal length and lens, when known.
    fn capture_line(&self) -> Option<AnyElement> {
        let meta = &self.catalog.selected_asset()?.meta;
        let mut parts = Vec::new();
        if meta.iso > 0 {
            parts.push(format!("ISO {}", meta.iso));
        }
        if meta.focal_mm > 0.0 {
            parts.push(format!("{:.0} mm", meta.focal_mm));
        }
        if !meta.lens.trim().is_empty() {
            parts.push(meta.lens.trim().to_string());
        }
        if parts.is_empty() {
            return None;
        }
        Some(
            div()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_size(sp(10.0))
                .text_color(self.theme.text_faint)
                .child(parts.join("  \u{00B7}  "))
                .into_any_element(),
        )
    }

    /// The level under `position`, if it's over the graph.
    fn histogram_level(&self, position: Point<Pixels>) -> Option<f32> {
        let b = self.histogram_ui.bounds.get();
        if !b.contains(&position) || b.size.width <= sp(0.0) {
            return None;
        }
        Some((f32::from(position.x - b.origin.x) / f32::from(b.size.width)).clamp(0.0, 1.0))
    }

    fn start_zone_drag(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(level) = self.histogram_level(position) else {
            return;
        };
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        let zone = TonalZone::at(level);
        self.histogram_ui.drag = Some(ZoneDrag {
            zone,
            start_x: position.x,
            start_value: zone.adjustment().get(&asset.develop),
        });
        cx.notify();
    }

    /// Follows a zone drag: the full graph width spans the slider's whole
    /// range. Returns whether one was in progress.
    pub(crate) fn histogram_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.histogram_ui.drag else {
            return false;
        };
        if !event.dragging() {
            self.histogram_ui.drag = None;
            cx.notify();
            return true;
        }
        let width = f32::from(self.histogram_ui.bounds.get().size.width).max(1.0);
        let adjustment = drag.zone.adjustment();
        let (min, max) = adjustment.range();
        let moved = f32::from(event.position.x - drag.start_x) / width * (max - min);
        self.apply_adjustment(adjustment, drag.start_value + moved, cx);
        true
    }

    pub(crate) fn end_histogram_drag(&mut self) {
        self.histogram_ui.drag = None;
    }

    /// J in Develop: shadow and highlight clipping on the photo, together.
    pub(crate) fn toggle_clipping_overlay(&mut self, cx: &mut Context<Self>) {
        let ui = &mut self.histogram_ui;
        let show = !(ui.show_shadow_clipping && ui.show_highlight_clipping);
        ui.show_shadow_clipping = show;
        ui.show_highlight_clipping = show;
        self.status_message = Some(
            if show {
                "Showing clipping: blue shadows, red highlights"
            } else {
                "Clipping hidden"
            }
            .into(),
        );
        self.request_preview_update(cx);
        cx.notify();
    }

    /// In Develop, the live histogram of the edited preview once one has
    /// been rendered; otherwise the original photo's histogram.
    pub(crate) fn current_histogram(&self) -> (Option<Arc<Histogram>>, &'static str) {
        let Some(asset) = self.catalog.selected_asset() else {
            return (None, "");
        };
        if self.mode == Mode::Develop {
            if let Some((id, histogram)) = &self.develop_histogram {
                if *id == asset.id {
                    return (Some(histogram.clone()), "Edited");
                }
            }
        }
        (self.histogram_cache.get(&asset.id).cloned(), "Original")
    }
}

/// The colour of whichever channels clip, mixed: red + green shows yellow,
/// all three white. Grey when nothing clips.
fn clip_color(clipping: Clipping, theme: &Theme) -> Hsla {
    match clipping.flagged() {
        (false, false, false) => theme.text_faint.opacity(0.6),
        (true, true, true) => hsla(0.0, 0.0, 0.95, 1.0),
        (true, true, false) => hsla(55.0 / 360.0, 0.9, 0.55, 1.0),
        (false, true, true) => hsla(185.0 / 360.0, 0.8, 0.55, 1.0),
        (true, false, true) => hsla(300.0 / 360.0, 0.7, 0.62, 1.0),
        (true, false, false) => hsla(0.0, 0.8, 0.58, 1.0),
        (false, true, false) => hsla(120.0 / 360.0, 0.65, 0.5, 1.0),
        (false, false, true) => hsla(215.0 / 360.0, 0.85, 0.62, 1.0),
    }
}

fn channel_tint(channel: HistogramChannel, theme: &Theme) -> Hsla {
    match channel {
        HistogramChannel::Red => hsla(0.0, 0.75, 0.6, 1.0),
        HistogramChannel::Green => hsla(120.0 / 360.0, 0.6, 0.55, 1.0),
        HistogramChannel::Blue => hsla(215.0 / 360.0, 0.8, 0.65, 1.0),
        _ => theme.text,
    }
}

/// A share as a percentage, with a decimal for small values.
fn percent(share: f32) -> String {
    let pct = share * 100.0;
    if pct > 0.0 && pct < 10.0 {
        format!("{pct:.1}%")
    } else {
        format!("{pct:.0}%")
    }
}
