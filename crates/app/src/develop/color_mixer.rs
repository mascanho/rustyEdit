//! The Color Mixer section: a spectrum that shows what the mixer does to
//! every colour, a row of swatches to pick the colour range to work on,
//! and a pad for that range.
//!
//! - **The spectrum.** Two strips: the colours as they are, and what the
//!   mixer turns each into. Where the lower strip differs from the upper
//!   one is where the picture will change. Ticks mark the eight ranges.
//! - **The swatches.** One per range; the picked one is ringed, and a dot
//!   marks those that have been changed.
//! - **The pad.** Drag across it to shift the range's hue (left and right)
//!   and its saturation (down and up). Its background paints what the
//!   range's colour becomes at each spot, so the puck is dragged to a
//!   colour rather than to a number. A slider under it sets the range's
//!   brightness. Double-click the pad to put the range back.
//!
//! The Hue / Saturation / Luminance tabs of sliders are still there, under
//! "Sliders".

use gpui::{
    canvas, div, fill, point, prelude::*, px, AnyElement, Bounds, ClickEvent, Context, Hsla,
    MouseButton, MouseDownEvent, PathBuilder, Pixels, Point, Rgba, SharedString,
};
use rustyedit_core::{Adjustment, ColorBand, ColorMixer, HistogramChannel, MixerProp};
use rustyedit_ui::{sp, HistogramProps};

use super::adjustments::wheel;
use crate::workspace::Workspace;

/// Height of the histogram strip above the spectrum.
const HISTOGRAM_STRIP: f32 = 56.0;
/// Height of each spectrum strip, and of the pad.
const STRIP_HEIGHT: f32 = 14.0;
const PAD_HEIGHT: f32 = 128.0;
/// How many cells the pad's background is painted in.
const PAD_COLUMNS: usize = 28;
const PAD_ROWS: usize = 16;
/// How many segments each spectrum strip is drawn in.
const STRIP_SEGMENTS: usize = 90;
/// Within this much of the middle the pad snaps to "no change".
const SNAP: f32 = 3.0;

/// A colour at hue `hue` as the mixer shows it: a clear, medium colour.
fn sample(hue: f32) -> [f32; 3] {
    let c = wheel(hue, 0.75, 0.5);
    let rgba: Rgba = c.into();
    [rgba.r, rgba.g, rgba.b]
}

fn colour([r, g, b]: [f32; 3]) -> Hsla {
    Rgba { r, g, b, a: 1.0 }.into()
}

/// A mixer with only `band` changed, as the pad's background shows.
fn only(band: ColorBand, hue: f32, saturation: f32, luminance: f32) -> ColorMixer {
    let mut mixer = ColorMixer::default();
    let adjust = &mut mixer[band.index()];
    adjust.set(MixerProp::Hue, hue);
    adjust.set(MixerProp::Saturation, saturation);
    adjust.set(MixerProp::Luminance, luminance);
    mixer
}

/// The hue and saturation sliders for a pointer at (`x`, `y`) in a pad
/// `width` × `height`: hue across, saturation up.
fn pad_value(x: f32, y: f32, width: f32, height: f32) -> (f32, f32) {
    let snap = |v: f32| if v.abs() < SNAP { 0.0 } else { v.round() };
    let hue = ((x / width.max(1.0)).clamp(0.0, 1.0) - 0.5) * 200.0;
    let saturation = (0.5 - (y / height.max(1.0)).clamp(0.0, 1.0)) * 200.0;
    (snap(hue), snap(saturation))
}

/// Where the puck sits in a pad `width` × `height`, the inverse of
/// [`pad_value`].
fn puck_position(hue: f32, saturation: f32, width: f32, height: f32) -> (f32, f32) {
    (
        (0.5 + hue / 200.0).clamp(0.0, 1.0) * width,
        (0.5 - saturation / 200.0).clamp(0.0, 1.0) * height,
    )
}

impl Workspace {
    pub(super) fn color_mixer_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(mixer) = self.catalog.selected_asset().map(|a| a.develop.mixer) else {
            return Vec::new();
        };
        let band = self.develop_ui.mixer_band;
        let adjust = mixer[band.index()];
        let band_colour = wheel(band.hue(), 0.85, 0.55);

        let (histogram, _) = self.current_histogram();
        let mut rows = vec![rustyedit_ui::histogram_view_with_height(
            &theme,
            HistogramProps {
                histogram,
                channel: HistogramChannel::Rgb,
                log_scale: false,
                hover: None,
                zone: None,
                bounds: None,
            },
            HISTOGRAM_STRIP,
        )
        .into_any_element()];

        // --- The spectrum, before and after ---------------------------------
        let caption = |text: &'static str| {
            div()
                .text_size(sp(8.5))
                .text_color(theme.text_faint)
                .child(text)
        };
        let selected_hue = band.hue();
        let strips = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                let each = h / 2.0 - 1.0;
                for i in 0..STRIP_SEGMENTS {
                    let x0 = i as f32 / STRIP_SEGMENTS as f32 * w;
                    let x1 = (i + 1) as f32 / STRIP_SEGMENTS as f32 * w;
                    let hue = (i as f32 + 0.5) / STRIP_SEGMENTS as f32 * 360.0;
                    let original = sample(hue);
                    let result = rustyedit_processing::mix_colour(&mixer, original);
                    for (row, c) in [(0.0, original), (each + 2.0, result)] {
                        let cell = Bounds {
                            origin: b.origin + point(px(x0), px(row)),
                            size: gpui::size(px(x1 - x0 + 0.6), px(each)),
                        };
                        window.paint_quad(fill(cell, colour(c)));
                    }
                }
                // A tick for each range, the picked one bolder.
                for other in ColorBand::ALL {
                    let x = other.hue() / 360.0 * w;
                    let on = other.hue() == selected_hue;
                    let tick = Bounds {
                        origin: b.origin + point(px(x - if on { 1.0 } else { 0.5 }), px(0.0)),
                        size: gpui::size(px(if on { 2.0 } else { 1.0 }), px(h)),
                    };
                    window.paint_quad(fill(
                        tick,
                        if on { gpui::white() } else { gpui::black().opacity(0.45) },
                    ));
                }
            },
        )
        .w_full()
        .h(sp(STRIP_HEIGHT * 2.0 + 2.0));
        rows.push(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .pt_2()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .child(caption("ORIGINAL"))
                        .child(caption("RESULT")),
                )
                .child(strips)
                .into_any_element(),
        );

        // --- The swatches ----------------------------------------------------
        let swatches = ColorBand::ALL.iter().map(|&other| {
            let selected = other == band;
            let edited = mixer[other.index()] != Default::default();
            let c = wheel(other.hue(), 0.8, 0.5);
            div()
                .id(("mixer-swatch", other.index()))
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .size(sp(24.0))
                .rounded_full()
                .cursor_pointer()
                .border_2()
                .border_color(if selected { gpui::white() } else { gpui::transparent_black() })
                .child(div().size(sp(16.0)).rounded_full().bg(c))
                .when(edited, |el| {
                    el.child(
                        div()
                            .absolute()
                            .bottom(sp(-1.0))
                            .right(sp(-1.0))
                            .size(sp(7.0))
                            .rounded_full()
                            .bg(theme.text)
                            .border_1()
                            .border_color(theme.background),
                    )
                })
                .tooltip(rustyedit_ui::tip(other.label(), &theme))
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.develop_ui.mixer_band = other;
                    cx.notify();
                }))
        });
        rows.push(
            div()
                .flex()
                .flex_row()
                .justify_between()
                .pt_2()
                .children(swatches)
                .into_any_element(),
        );

        // --- The picked range: header, pad -------------------------------------
        let changed = adjust != Default::default();
        rows.push(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .pt_2()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .child(div().size(sp(9.0)).rounded_full().bg(band_colour))
                        .child(
                            div()
                                .text_size(sp(11.0))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(theme.text)
                                .child(band.label().to_uppercase()),
                        ),
                )
                .child(rustyedit_ui::reset_button(
                    SharedString::from(format!("mixer-reset-{}", band.label())),
                    changed,
                    &theme,
                    cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.reset_mixer_band(band, true, cx)
                    }),
                ))
                .into_any_element(),
        );

        let pad_bounds = self.develop_ui.mixer_pad.clone();
        let luminance = adjust.luminance;
        let (hue_value, sat_value) = (adjust.hue, adjust.saturation);
        let pad = div()
            .id("mixer-pad")
            .relative()
            .w_full()
            .h(sp(PAD_HEIGHT))
            .rounded_md()
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .cursor_crosshair()
            .child(
                canvas(
                    move |b, _, _| pad_bounds.set(b),
                    move |b, _, window, _| {
                        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                        let base = sample(band.hue());
                        // What the range's colour becomes at each spot.
                        for row in 0..PAD_ROWS {
                            for col in 0..PAD_COLUMNS {
                                let hue = ((col as f32 + 0.5) / PAD_COLUMNS as f32 - 0.5) * 200.0;
                                let sat = (0.5 - (row as f32 + 0.5) / PAD_ROWS as f32) * 200.0;
                                let result = rustyedit_processing::mix_colour(
                                    &only(band, hue, sat, luminance),
                                    base,
                                );
                                let (x0, x1) = (
                                    col as f32 / PAD_COLUMNS as f32 * w,
                                    (col + 1) as f32 / PAD_COLUMNS as f32 * w,
                                );
                                let (y0, y1) = (
                                    row as f32 / PAD_ROWS as f32 * h,
                                    (row + 1) as f32 / PAD_ROWS as f32 * h,
                                );
                                let cell = Bounds {
                                    origin: b.origin + point(px(x0), px(y0)),
                                    size: gpui::size(px(x1 - x0 + 0.6), px(y1 - y0 + 0.6)),
                                };
                                window.paint_quad(fill(cell, colour(result)));
                            }
                        }
                        // The "no change" cross.
                        let line = |window: &mut gpui::Window, a: Point<Pixels>, c: Point<Pixels>| {
                            let mut path = PathBuilder::stroke(px(1.0));
                            path.add_polygon(&[a, c], false);
                            if let Ok(path) = path.build() {
                                window.paint_path(path, gpui::white().opacity(0.45));
                            }
                        };
                        line(window, b.origin + point(px(w / 2.0), px(0.0)), b.origin + point(px(w / 2.0), px(h)));
                        line(window, b.origin + point(px(0.0), px(h / 2.0)), b.origin + point(px(w), px(h / 2.0)));
                        // The puck.
                        let (px_x, px_y) = puck_position(hue_value, sat_value, w, h);
                        let centre = b.origin + point(px(px_x), px(px_y));
                        let circle = |r: f32| -> Vec<Point<Pixels>> {
                            (0..28)
                                .map(|i| {
                                    let a = i as f32 / 28.0 * std::f32::consts::TAU;
                                    centre + point(px(a.cos() * r), px(a.sin() * r))
                                })
                                .collect()
                        };
                        // A white ring over a dark one, to read on any colour.
                        for (r, c, width) in [
                            (8.0, gpui::black().opacity(0.5), 3.0),
                            (7.0, gpui::white(), 2.0),
                        ] {
                            let mut path = PathBuilder::stroke(px(width));
                            path.add_polygon(&circle(r), true);
                            if let Ok(path) = path.build() {
                                window.paint_path(path, c);
                            }
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            // Axis hints in the corners.
            .child(
                div()
                    .absolute()
                    .top(sp(3.0))
                    .left(sp(5.0))
                    .text_size(sp(8.5))
                    .text_color(gpui::white().opacity(0.8))
                    .child("+ SAT"),
            )
            .child(
                div()
                    .absolute()
                    .bottom(sp(3.0))
                    .left(sp(5.0))
                    .text_size(sp(8.5))
                    .text_color(gpui::white().opacity(0.8))
                    .child("\u{2212} SAT"),
            )
            .child(
                div()
                    .absolute()
                    .top(sp(3.0))
                    .right(sp(5.0))
                    .text_size(sp(8.5))
                    .text_color(gpui::white().opacity(0.8))
                    .child("HUE \u{25B6}"),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    if event.click_count >= 2 {
                        this.reset_mixer_band(band, false, cx);
                        return;
                    }
                    this.develop_ui.mixer_pad_drag = true;
                    this.set_mixer_from_pointer(event.position, cx);
                }),
            );
        rows.push(div().pt_1().child(pad).into_any_element());

        // The range's brightness, then what it's at.
        rows.push(self.adjustment_slider(Adjustment::Mixer(band, MixerProp::Luminance), cx));
        let readout = |label: &'static str, value: f32| {
            div()
                .flex()
                .flex_col()
                .items_center()
                .child(div().text_size(sp(8.5)).text_color(theme.text_faint).child(label))
                .child(
                    div()
                        .text_size(sp(10.5))
                        .text_color(if value == 0.0 { theme.text_muted } else { theme.text })
                        .child(format!("{value:+.0}")),
                )
        };
        rows.push(
            div()
                .flex()
                .flex_row()
                .justify_around()
                .child(readout("HUE", adjust.hue))
                .child(readout("SAT", adjust.saturation))
                .child(readout("LUM", adjust.luminance))
                .into_any_element(),
        );

        // The tabbed sliders, for typing and keyboard nudges.
        let show = self.develop_ui.mixer_sliders;
        rows.push(
            div()
                .id("mixer-sliders-toggle")
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .pt_2()
                .cursor_pointer()
                .text_size(sp(10.5))
                .text_color(theme.text_faint)
                .hover(|el| el.text_color(theme.text))
                .child(rustyedit_ui::icon(
                    if show {
                        rustyedit_ui::Icon::ChevronDown
                    } else {
                        rustyedit_ui::Icon::ChevronRight
                    },
                    10.0,
                    theme.text_faint,
                ))
                .child("Sliders")
                .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                    this.develop_ui.mixer_sliders = !this.develop_ui.mixer_sliders;
                    cx.notify();
                }))
                .into_any_element(),
        );
        if show {
            rows.extend(self.mixer_slider_tabs(cx));
        }
        rows
    }

    /// Hue / Saturation / Luminance tabs with a slider per colour.
    fn mixer_slider_tabs(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let prop = self.develop_ui.mixer_prop;
        let tabs = MixerProp::ALL.iter().map(|&tab| {
            let active = tab == prop;
            div()
                .id(("mixer-tab", tab as usize))
                .flex_1()
                .flex()
                .justify_center()
                .h(sp(20.0))
                .items_center()
                .rounded_sm()
                .cursor_pointer()
                .text_size(sp(10.5))
                .when(active, |el| el.bg(theme.surface_elevated).text_color(theme.text))
                .when(!active, |el| {
                    el.text_color(theme.text_muted)
                        .hover(|el| el.text_color(theme.text))
                })
                .child(tab.label())
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.develop_ui.mixer_prop = tab;
                    cx.notify();
                }))
        });
        let mut rows = vec![div()
            .flex()
            .flex_row()
            .gap_0p5()
            .p_0p5()
            .my_1()
            .rounded_md()
            .bg(theme.background)
            .children(tabs)
            .into_any_element()];
        rows.extend(
            ColorBand::ALL
                .iter()
                .map(|band| self.adjustment_slider(Adjustment::Mixer(*band, prop), cx)),
        );
        rows
    }

    /// Puts a colour range's hue and saturation back (and with `all` its
    /// luminance).
    fn reset_mixer_band(&mut self, band: ColorBand, all: bool, cx: &mut Context<Self>) {
        let label = format!("{} mixer", band.label());
        self.edit_develop(
            &label,
            |d| {
                let adjust = &mut d.mixer[band.index()];
                adjust.set(MixerProp::Hue, 0.0);
                adjust.set(MixerProp::Saturation, 0.0);
                if all {
                    adjust.set(MixerProp::Luminance, 0.0);
                }
            },
            |_| (String::new(), "reset".to_string()),
            cx,
        );
    }

    /// Puts the picked range's puck where the pointer is on the pad.
    fn set_mixer_from_pointer(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let b = self.develop_ui.mixer_pad.get();
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let band = self.develop_ui.mixer_band;
        let x = f32::from(position.x - b.origin.x);
        let y = f32::from(position.y - b.origin.y);
        let (hue, saturation) = pad_value(x, y, w, h);
        let label = format!("{} mixer", band.label());
        self.edit_develop(
            &label,
            |d| {
                let adjust = &mut d.mixer[band.index()];
                adjust.set(MixerProp::Hue, hue);
                adjust.set(MixerProp::Saturation, saturation);
            },
            |d| {
                let a = d.mixer[band.index()];
                (String::new(), format!("hue {:+.0}, saturation {:+.0}", a.hue, a.saturation))
            },
            cx,
        );
    }

    /// Follows a drag on the pad; `true` while one is in progress.
    pub(super) fn mixer_drag_move(
        &mut self,
        position: Point<Pixels>,
        dragging: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.develop_ui.mixer_pad_drag {
            return false;
        }
        if !dragging {
            self.develop_ui.mixer_pad_drag = false;
            cx.notify();
            return true;
        }
        self.set_mixer_from_pointer(position, cx);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_middle_of_the_pad_is_no_change_and_the_corners_are_the_extremes() {
        assert_eq!(pad_value(100.0, 60.0, 200.0, 120.0), (0.0, 0.0));
        assert_eq!(pad_value(0.0, 0.0, 200.0, 120.0), (-100.0, 100.0));
        assert_eq!(pad_value(200.0, 120.0, 200.0, 120.0), (100.0, -100.0));
        // Past the edge stays at the extreme.
        assert_eq!(pad_value(500.0, -40.0, 200.0, 120.0), (100.0, 100.0));
    }

    #[test]
    fn the_puck_sits_exactly_under_the_pointer() {
        let (w, h) = (240.0, 128.0);
        for (hue, sat) in [(0.0, 0.0), (50.0, -30.0), (-100.0, 100.0), (88.0, 64.0)] {
            let (x, y) = puck_position(hue, sat, w, h);
            let (back_hue, back_sat) = pad_value(x, y, w, h);
            assert!((back_hue - hue).abs() <= 1.0, "{hue}: {back_hue}");
            assert!((back_sat - sat).abs() <= 1.0, "{sat}: {back_sat}");
        }
    }

    #[test]
    fn a_small_wobble_near_the_middle_stays_at_no_change() {
        let (hue, sat) = pad_value(101.0, 59.0, 200.0, 120.0);
        assert_eq!((hue, sat), (0.0, 0.0));
    }

    #[test]
    fn the_spectrum_shows_a_shifted_range_moving() {
        // Pushing Green's hue by +100 (30°) changes green but leaves red.
        let mut mixer = ColorMixer::default();
        mixer[ColorBand::Green.index()].set(MixerProp::Hue, 100.0);
        let red = sample(0.0);
        assert_eq!(rustyedit_processing::mix_colour(&mixer, red), red);
        let green = sample(120.0);
        assert_ne!(rustyedit_processing::mix_colour(&mixer, green), green);
    }

    #[test]
    fn the_pad_background_follows_the_range_colour() {
        // Saturating a range makes its colour more vivid than desaturating.
        let band = ColorBand::Blue;
        let base = sample(band.hue());
        let spread = |c: [f32; 3]| c.iter().cloned().fold(0.0f32, f32::max) - c.iter().cloned().fold(1.0f32, f32::min);
        let more = rustyedit_processing::mix_colour(&only(band, 0.0, 100.0, 0.0), base);
        let less = rustyedit_processing::mix_colour(&only(band, 0.0, -100.0, 0.0), base);
        assert!(spread(less) < spread(base) && spread(base) <= spread(more) + 0.001);
    }
}
