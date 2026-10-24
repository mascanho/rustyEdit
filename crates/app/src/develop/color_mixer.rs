//! The Color Mixer section, laid out like Color Grading: one wheel for each
//! of the eight colour ranges, a master slider under it for that colour's
//! brightness, readouts and a reset.
//!
//! On a mixer wheel the puck says what happens to the colour's own hue: it
//! starts on the colour's tick on the ring. Slide it round the ring to
//! shift the hue (up to 30° either way), and towards the centre or the rim
//! to desaturate or saturate it. The ring through the middle of the disc is
//! "no change". Double-click a wheel to put it back.
//!
//! The Hue / Saturation / Luminance tabs of sliders are still there, under
//! "Sliders".

use gpui::{
    canvas, div, prelude::*, AnyElement, ClickEvent, Context, MouseButton, MouseDownEvent,
    Pixels, Point, SharedString,
};
use rustyedit_core::{Adjustment, ColorBand, HistogramChannel, MixerProp};
use rustyedit_ui::{sp, HistogramProps};

use super::{
    adjustments::wheel,
    color_grading::{paint_wheel, WheelFace, DISC, WHEEL_SIZE},
};
use crate::workspace::Workspace;

/// Height of the histogram strip above the wheels.
const HISTOGRAM_STRIP: f32 = 64.0;
/// The most a colour's hue moves, in degrees: what the Hue slider's ends
/// mean (see `finishing/mixer.rs`).
const MAX_HUE_SHIFT: f32 = 30.0;

/// Where a band's puck sits on its wheel: the angle, and how far out as a
/// fraction of the disc (the middle ring is no change).
fn mixer_puck(band: ColorBand, hue: f32, saturation: f32) -> (f32, f32) {
    let angle = (band.hue() + hue / 100.0 * MAX_HUE_SHIFT).rem_euclid(360.0);
    let frac = (0.5 + saturation / 200.0).clamp(0.0, 1.0);
    (angle, frac)
}

/// The inverse: the hue and saturation sliders for a pointer `dx`, `dy`
/// from the centre of a wheel of `radius`. The hue is `None` too near the
/// centre to say.
fn mixer_value(band: ColorBand, dx: f32, dy: f32, radius: f32) -> (Option<f32>, f32) {
    let reach = (radius * DISC).max(1.0);
    let distance = dx.hypot(dy);
    let saturation = ((distance / reach).clamp(0.0, 1.0) - 0.5) * 200.0;
    let hue = (distance > 2.0).then(|| {
        let angle = (dy.atan2(dx).to_degrees() + 90.0).rem_euclid(360.0);
        let mut diff = (angle - band.hue()).rem_euclid(360.0);
        if diff > 180.0 {
            diff -= 360.0;
        }
        diff.clamp(-MAX_HUE_SHIFT, MAX_HUE_SHIFT) / MAX_HUE_SHIFT * 100.0
    });
    (hue, saturation)
}

impl Workspace {
    pub(super) fn color_mixer_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        if self.catalog.selected_asset().is_none() {
            return Vec::new();
        }
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

        for pair in ColorBand::ALL.chunks(2) {
            rows.push(
                div()
                    .flex()
                    .flex_row()
                    .justify_around()
                    .pt_2()
                    .children(pair.iter().map(|band| self.mixer_cell(*band, cx)))
                    .into_any_element(),
            );
        }

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

    /// One colour's wheel with its name, reset, master slider and readouts.
    fn mixer_cell(&self, band: ColorBand, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let adjust = self
            .catalog
            .selected_asset()
            .map(|a| a.develop.mixer[band.index()])
            .unwrap_or_default();
        let (hue, saturation, luminance) = (adjust.hue, adjust.saturation, adjust.luminance);
        let changed = hue != 0.0 || saturation != 0.0 || luminance != 0.0;
        let bounds = self.develop_ui.mixer_bounds[band.index()].clone();
        let size = sp(WHEEL_SIZE);
        let colour = wheel(band.hue(), 0.85, 0.55);

        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .w(size)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .child(div().size(sp(8.0)).rounded_full().bg(colour))
                    .child(
                        div()
                            .text_size(sp(10.5))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_muted)
                            .child(band.label().to_uppercase()),
                    ),
            )
            .child(rustyedit_ui::reset_button(
                SharedString::from(format!("mixer-reset-{}", band.label())),
                changed,
                &theme,
                cx.listener(move |this, _: &ClickEvent, _w, cx| this.reset_mixer_wheel(band, true, cx)),
            ));

        let (angle, frac) = mixer_puck(band, hue, saturation);
        let face = div()
            .id(SharedString::from(format!("mixer-wheel-{}", band.label())))
            .relative()
            .size(size)
            .cursor_crosshair()
            .child(
                canvas(
                    move |b, _, _| bounds.set(b),
                    move |b, _, window, _| {
                        let face = WheelFace {
                            puck: Some((angle, frac)),
                            puck_colour: colour,
                            marker: Some(band.hue()),
                        };
                        paint_wheel(window, b, face, &theme)
                    },
                )
                .absolute()
                .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    if event.click_count >= 2 {
                        this.reset_mixer_wheel(band, false, cx);
                        return;
                    }
                    this.develop_ui.mixer_drag = Some(band);
                    this.set_mixer_from_pointer(band, event.position, cx);
                }),
            );

        let entity = cx.entity();
        let master = rustyedit_ui::compact_slider(
            SharedString::from(format!("mixer-master-{}", band.label())),
            (luminance + 100.0) / 200.0,
            WHEEL_SIZE,
            &theme,
            move |t, _w, cx| {
                entity.update(cx, |this, cx| {
                    let value = ((t * 200.0 - 100.0) / 2.0).round() * 2.0;
                    let value = if value.abs() < 3.0 { 0.0 } else { value };
                    this.apply_adjustment(Adjustment::Mixer(band, MixerProp::Luminance), value, cx);
                });
            },
        );

        let readout = |label: &'static str, value: String| {
            div()
                .flex()
                .flex_col()
                .items_center()
                .child(div().text_size(sp(8.5)).text_color(theme.text_faint).child(label))
                .child(div().text_size(sp(10.0)).text_color(theme.text).child(value))
        };
        let readouts = div()
            .flex()
            .flex_row()
            .justify_between()
            .w(size)
            .child(readout("HUE", format!("{hue:+.0}")))
            .child(readout("SAT", format!("{saturation:+.0}")))
            .child(readout("LUM", format!("{luminance:+.0}")));

        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .child(header)
            .child(face)
            .child(master)
            .child(readouts)
            .into_any_element()
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

    /// Puts a colour's hue and saturation back (and with `all` its
    /// luminance).
    fn reset_mixer_wheel(&mut self, band: ColorBand, all: bool, cx: &mut Context<Self>) {
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

    fn set_mixer_from_pointer(&mut self, band: ColorBand, position: Point<Pixels>, cx: &mut Context<Self>) {
        let b = self.develop_ui.mixer_bounds[band.index()].get();
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let radius = w.min(h) / 2.0;
        let dx = f32::from(position.x - b.origin.x) - w / 2.0;
        let dy = f32::from(position.y - b.origin.y) - h / 2.0;
        let (hue, saturation) = mixer_value(band, dx, dy, radius);
        let label = format!("{} mixer", band.label());
        self.edit_develop(
            &label,
            |d| {
                let adjust = &mut d.mixer[band.index()];
                if let Some(hue) = hue {
                    adjust.set(MixerProp::Hue, hue);
                }
                adjust.set(MixerProp::Saturation, saturation);
            },
            |d| {
                let a = d.mixer[band.index()];
                (String::new(), format!("hue {:+.0}, saturation {:+.0}", a.hue, a.saturation))
            },
            cx,
        );
    }

    /// Follows a drag on a mixer wheel; `true` while one is in progress.
    pub(super) fn mixer_drag_move(
        &mut self,
        position: Point<Pixels>,
        dragging: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(band) = self.develop_ui.mixer_drag else {
            return false;
        };
        if !dragging {
            self.develop_ui.mixer_drag = None;
            cx.notify();
            return true;
        }
        self.set_mixer_from_pointer(band, position, cx);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::color_grading::puck_offset;
    use super::*;

    #[test]
    fn an_untouched_colour_sits_on_its_tick_on_the_middle_ring() {
        for band in ColorBand::ALL {
            let (angle, frac) = mixer_puck(band, 0.0, 0.0);
            assert_eq!(angle, band.hue());
            assert_eq!(frac, 0.5);
        }
    }

    #[test]
    fn the_pointer_and_the_puck_agree() {
        let r = 100.0;
        for band in ColorBand::ALL {
            for (hue, sat) in [(0.0, 0.0), (60.0, 40.0), (-100.0, -80.0), (100.0, 100.0)] {
                let (angle, frac) = mixer_puck(band, hue, sat);
                let (ox, oy) = puck_offset(angle, frac * 100.0, r);
                let (h, s) = mixer_value(band, ox, oy, r);
                assert!((h.unwrap() - hue).abs() < 0.05, "{band:?} hue {hue}: {h:?}");
                assert!((s - sat).abs() < 0.05, "{band:?} sat {sat}: {s}");
            }
        }
    }

    #[test]
    fn the_hue_shift_stops_thirty_degrees_either_side() {
        // Pointing at the far side of the wheel from red is as far round
        // as it goes.
        let (hue, _) = mixer_value(ColorBand::Red, 0.0, 60.0, 100.0);
        assert!(hue.unwrap().abs() >= 99.9);
        let (hue, _) = mixer_value(ColorBand::Green, 45.0, -45.0, 100.0);
        // 45° is 75° short of green's 120°: pinned at -100.
        assert_eq!(hue.unwrap(), -100.0);
    }

    #[test]
    fn the_centre_desaturates_and_the_rim_saturates() {
        assert_eq!(mixer_value(ColorBand::Blue, 0.0, 0.0, 100.0).1, -100.0);
        assert!((mixer_value(ColorBand::Blue, 0.0, -78.0, 100.0).1 - 100.0).abs() < 0.01);
        assert!(mixer_value(ColorBand::Blue, 0.0, 0.0, 100.0).0.is_none());
    }
}
