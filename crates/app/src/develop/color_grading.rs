//! The Color Grading section, laid out like a colourist's panel: four
//! colour wheels (Lift, Gamma, Gain and Offset) over the shadows,
//! midtones, highlights and the whole picture. Drag a wheel's puck to tint:
//! the angle is the hue and the distance from the centre is the strength.
//! The slider under each wheel is its master, brightening or darkening that
//! range. Double-click a wheel to centre it. A histogram of the photo sits
//! above them, so the tones being graded can be seen.

use gpui::{
    canvas, div, point, prelude::*, px, AnyElement, Bounds, ClickEvent, Context, Hsla,
    MouseButton, MouseDownEvent, PathBuilder, Pixels, Point, SharedString, Window,
};
use rustyedit_core::{Adjustment, GradeProp, HistogramChannel, ToneZone};
use rustyedit_ui::{sp, HistogramProps, Theme};

use super::adjustments::wheel;
use crate::workspace::Workspace;

/// Height of the histogram strip above the wheels.
const HISTOGRAM_STRIP: f32 = 64.0;
/// Diameter of a wheel, in logical pixels.
const WHEEL_SIZE: f32 = 116.0;
/// The inner disc, as a fraction of the wheel's radius; the hue ring is
/// the rest.
const DISC: f32 = 0.78;
/// How many wedges the hue ring is drawn in.
const RING_WEDGES: usize = 72;

fn zone_index(zone: ToneZone) -> usize {
    ToneZone::WHEELS.iter().position(|z| *z == zone).unwrap_or(0)
}

impl Workspace {
    pub(super) fn color_grading_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        if self.catalog.selected_asset().is_none() {
            return Vec::new();
        }

        // The live histogram of the photo, so the tones being graded can
        // be read against the ranges they cover.
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

        // The wheels, two by two.
        for pair in ToneZone::WHEELS.chunks(2) {
            rows.push(
                div()
                    .flex()
                    .flex_row()
                    .justify_around()
                    .pt_2()
                    .children(pair.iter().map(|zone| self.wheel_cell(*zone, cx)))
                    .into_any_element(),
            );
        }

        rows.push(div().h(sp(4.0)).into_any_element());
        rows.push(self.adjustment_slider(Adjustment::GradingBalance, cx));

        // The same values as plain sliders, for typing or keyboard nudges.
        let show = self.develop_ui.grading_sliders;
        rows.push(
            div()
                .id("grading-sliders-toggle")
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .pt_1()
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
                    this.develop_ui.grading_sliders = !this.develop_ui.grading_sliders;
                    cx.notify();
                }))
                .into_any_element(),
        );
        if show {
            for zone in ToneZone::WHEELS {
                rows.push(
                    div()
                        .pt_1()
                        .text_size(sp(10.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.text_muted)
                        .child(zone.wheel_name())
                        .into_any_element(),
                );
                for prop in [GradeProp::Hue, GradeProp::Saturation, GradeProp::Luminance] {
                    rows.push(self.adjustment_slider(Adjustment::Grade(zone, prop), cx));
                }
            }
        }
        rows
    }

    /// One wheel with its name, reset, master slider and readout.
    fn wheel_cell(&self, zone: ToneZone, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let grade = self
            .catalog
            .selected_asset()
            .map(|a| *a.develop.grading.zone(zone))
            .unwrap_or_default();
        let changed = grade.is_active() || grade.hue != 0.0;
        let bounds = self.develop_ui.wheel_bounds[zone_index(zone)].clone();
        let size = sp(WHEEL_SIZE);

        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .w(size)
            .child(
                div()
                    .text_size(sp(10.5))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text_muted)
                    .child(zone.wheel_name().to_uppercase()),
            )
            .child(rustyedit_ui::reset_button(
                SharedString::from(format!("wheel-reset-{}", zone.wheel_name())),
                changed,
                &theme,
                cx.listener(move |this, _: &ClickEvent, _w, cx| this.reset_wheel(zone, true, cx)),
            ));

        let (hue, saturation) = (grade.hue, grade.saturation);
        let face = div()
            .id(SharedString::from(format!("wheel-{}", zone.wheel_name())))
            .relative()
            .size(size)
            .cursor_crosshair()
            .child(
                canvas(
                    move |b, _, _| bounds.set(b),
                    move |b, _, window, _| {
                        let face = WheelFace {
                            puck: (saturation > 0.0).then_some((hue, saturation / 100.0)),
                            puck_colour: wheel(hue, 0.85, 0.55),
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
                        this.reset_wheel(zone, false, cx);
                        return;
                    }
                    this.develop_ui.wheel_drag = Some(zone);
                    this.set_wheel_from_pointer(zone, event.position, cx);
                }),
            );

        // The master slider under it: -100..=100 as 0..=1.
        let entity = cx.entity();
        let master = rustyedit_ui::compact_slider(
            SharedString::from(format!("wheel-master-{}", zone.wheel_name())),
            (grade.luminance + 100.0) / 200.0,
            WHEEL_SIZE,
            &theme,
            move |t, _w, cx| {
                entity.update(cx, |this, cx| {
                    let value = ((t * 200.0 - 100.0) / 2.0).round() * 2.0;
                    // Snap to zero near the middle, like a centred wheel.
                    let value = if value.abs() < 3.0 { 0.0 } else { value };
                    this.apply_adjustment(Adjustment::Grade(zone, GradeProp::Luminance), value, cx);
                });
            },
        );

        let readout = |label: &'static str, value: String| {
            div()
                .flex()
                .flex_col()
                .items_center()
                .child(
                    div()
                        .text_size(sp(8.5))
                        .text_color(theme.text_faint)
                        .child(label),
                )
                .child(
                    div()
                        .text_size(sp(10.0))
                        .text_color(theme.text)
                        .child(value),
                )
        };
        let readouts = div()
            .flex()
            .flex_row()
            .justify_between()
            .w(size)
            .child(readout("HUE", format!("{:.0}\u{b0}", grade.hue)))
            .child(readout("SAT", format!("{:.0}", grade.saturation)))
            .child(readout("LUM", format!("{:+.0}", grade.luminance)));

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

    /// Centres a wheel (its tint), and with `all` its brightness too.
    fn reset_wheel(&mut self, zone: ToneZone, all: bool, cx: &mut Context<Self>) {
        let label = format!("{} wheel", zone.wheel_name());
        self.edit_develop(
            &label,
            |d| {
                let grade = d.grading.zone_mut(zone);
                grade.hue = 0.0;
                grade.saturation = 0.0;
                if all {
                    grade.luminance = 0.0;
                }
            },
            |_| (String::new(), "reset".to_string()),
            cx,
        );
    }

    /// Puts wheel `zone`'s puck where the pointer is.
    fn set_wheel_from_pointer(&mut self, zone: ToneZone, position: Point<Pixels>, cx: &mut Context<Self>) {
        let b = self.develop_ui.wheel_bounds[zone_index(zone)].get();
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let radius = w.min(h) / 2.0;
        let dx = f32::from(position.x - b.origin.x) - w / 2.0;
        let dy = f32::from(position.y - b.origin.y) - h / 2.0;
        let (hue, saturation) = wheel_value(dx, dy, radius);
        let label = format!("{} wheel", zone.wheel_name());
        self.edit_develop(
            &label,
            |d| {
                let grade = d.grading.zone_mut(zone);
                // Near the very centre the angle is noise: keep the hue.
                if let Some(hue) = hue {
                    grade.hue = hue;
                }
                grade.saturation = saturation;
            },
            |d| {
                let g = d.grading.zone(zone);
                (String::new(), format!("hue {:.0}\u{b0}, saturation {:.0}", g.hue, g.saturation))
            },
            cx,
        );
    }

    /// Follows a drag on a wheel; `true` while one is in progress.
    pub(super) fn wheel_drag_move(
        &mut self,
        position: Point<Pixels>,
        dragging: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(zone) = self.develop_ui.wheel_drag else {
            return false;
        };
        if !dragging {
            self.develop_ui.wheel_drag = None;
            cx.notify();
            return true;
        }
        self.set_wheel_from_pointer(zone, position, cx);
        true
    }
}

/// The hue (degrees, red at the top going clockwise; `None` too near the
/// centre to say) and strength (0..=100) of a point `dx`, `dy` from the
/// centre of a wheel of `radius`.
fn wheel_value(dx: f32, dy: f32, radius: f32) -> (Option<f32>, f32) {
    let reach = (radius * DISC).max(1.0);
    let distance = dx.hypot(dy);
    let saturation = (distance / reach).clamp(0.0, 1.0) * 100.0;
    let hue = (distance > 2.0).then(|| (dy.atan2(dx).to_degrees() + 90.0).rem_euclid(360.0));
    (hue, saturation)
}

/// Where a hue and strength sit, as an offset from the wheel's centre.
fn puck_offset(hue: f32, saturation: f32, radius: f32) -> (f32, f32) {
    let angle = (hue - 90.0).to_radians();
    let reach = radius * DISC * (saturation / 100.0).clamp(0.0, 1.0);
    (angle.cos() * reach, angle.sin() * reach)
}

/// What a wheel shows besides its ring and disc.
struct WheelFace {
    /// Where the puck sits: the angle (degrees, red at the top) and how far
    /// out, `0..=1` of the disc.
    pub puck: Option<(f32, f32)>,
    pub puck_colour: Hsla,
}

fn paint_wheel(window: &mut Window, bounds: Bounds<Pixels>, face: WheelFace, theme: &Theme) {
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let radius = w.min(h) / 2.0;
    if radius <= 1.0 {
        return;
    }
    let centre = point(
        bounds.origin.x + bounds.size.width / 2.0,
        bounds.origin.y + bounds.size.height / 2.0,
    );
    let at = |angle_deg: f32, r: f32| {
        let a = (angle_deg - 90.0).to_radians();
        centre + point(px(a.cos() * r), px(a.sin() * r))
    };
    let fill = |window: &mut Window, points: &[Point<Pixels>], colour: Hsla| {
        let mut path = PathBuilder::fill();
        path.add_polygon(points, true);
        if let Ok(path) = path.build() {
            window.paint_path(path, colour);
        }
    };

    // The hue ring, in wedges.
    let step = 360.0 / RING_WEDGES as f32;
    for i in 0..RING_WEDGES {
        let (a0, a1) = (i as f32 * step - 0.4, (i + 1) as f32 * step + 0.4);
        let colour = wheel(i as f32 * step + step / 2.0, 0.78, 0.52);
        fill(
            window,
            &[
                at(a0, radius),
                at(a1, radius),
                at(a1, radius * DISC + 2.0),
                at(a0, radius * DISC + 2.0),
            ],
            colour,
        );
    }
    // The disc, its rings and crosshair.
    let circle = |r: f32| -> Vec<Point<Pixels>> { (0..90).map(|i| at(i as f32 * 4.0, r)).collect() };
    fill(window, &circle(radius * DISC), theme.background);
    let guide = theme.border_subtle;
    stroke(window, circle(radius * DISC * 0.5), true, guide, 1.0);
    stroke(window, circle(radius * DISC), true, theme.border, 1.0);
    stroke(window, vec![at(0.0, radius * DISC), at(180.0, radius * DISC)], false, guide, 1.0);
    stroke(window, vec![at(90.0, radius * DISC), at(270.0, radius * DISC)], false, guide, 1.0);

    // The puck, and a line to it from the centre.
    let Some((angle, frac)) = face.puck else {
        let centre_puck = centre;
        stroke(window, dot(centre_puck), true, gpui::white().opacity(0.9), 1.5);
        return;
    };
    let (ox, oy) = puck_offset(angle, frac * 100.0, radius);
    let puck = centre + point(px(ox), px(oy));
    stroke(window, vec![centre, puck], false, face.puck_colour.opacity(0.7), 1.5);
    fill(window, &dot(puck), face.puck_colour);
    stroke(window, dot(puck), true, gpui::white().opacity(0.9), 1.5);
}

/// A small circle round `at`: the puck.
fn dot(at: Point<Pixels>) -> Vec<Point<Pixels>> {
    (0..24)
        .map(|i| {
            let a = (i as f32 * 15.0).to_radians();
            at + point(px(a.cos() * 6.0), px(a.sin() * 6.0))
        })
        .collect()
}

fn stroke(window: &mut Window, points: Vec<Point<Pixels>>, closed: bool, colour: Hsla, width: f32) {
    let mut path = PathBuilder::stroke(px(width));
    path.add_polygon(&points, closed);
    if let Ok(path) = path.build() {
        window.paint_path(path, colour);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wheel_reads_hue_clockwise_from_the_top() {
        let r = 100.0;
        // Straight up is red, to the right is 90°, down 180°, left 270°.
        let up = wheel_value(0.0, -50.0, r);
        assert!((up.0.unwrap() - 0.0).abs() < 0.01 || (up.0.unwrap() - 360.0).abs() < 0.01);
        assert!((wheel_value(50.0, 0.0, r).0.unwrap() - 90.0).abs() < 0.01);
        assert!((wheel_value(0.0, 50.0, r).0.unwrap() - 180.0).abs() < 0.01);
        assert!((wheel_value(-50.0, 0.0, r).0.unwrap() - 270.0).abs() < 0.01);
    }

    #[test]
    fn strength_grows_with_distance_and_stops_at_the_disc() {
        let r = 100.0;
        assert_eq!(wheel_value(0.0, 0.0, r).1, 0.0);
        assert!(wheel_value(0.0, 0.0, r).0.is_none(), "no hue at the centre");
        let half = wheel_value(r * DISC / 2.0, 0.0, r).1;
        assert!((half - 50.0).abs() < 0.01);
        assert_eq!(wheel_value(r * 2.0, 0.0, r).1, 100.0);
    }

    #[test]
    fn the_puck_sits_where_the_pointer_set_it() {
        for (hue, sat) in [(0.0, 40.0), (130.0, 80.0), (300.0, 100.0)] {
            let (ox, oy) = puck_offset(hue, sat, 100.0);
            let (h, s) = wheel_value(ox, oy, 100.0);
            assert!((h.unwrap() - hue).abs() < 0.01, "{hue}");
            assert!((s - sat).abs() < 0.01, "{sat}");
        }
    }
}
