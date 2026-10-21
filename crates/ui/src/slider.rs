use std::{cell::RefCell, rc::Rc};

use gpui::{
    div, hsla, linear_color_stop, linear_gradient, prelude::*, px, relative, AnyElement, App,
    ClickEvent, Context, DragMoveEvent, ElementId, Hsla, MouseButton, MouseDownEvent, Render,
    SharedString, Window,
};

use crate::{
    icons::{icon, Icon},
    theme::Theme,
    tooltip::tip,
};

/// An invisible 0x0 view used as the "dragged" element for slider drags.
/// GPUI's drag API always renders *something* that follows the cursor; we
/// don't want a floating tooltip, since the live value is already shown
/// next to the slider's label, so this stays empty.
struct DragGhost;

/// Drag payload naming the slider being dragged. GPUI sends drag-move
/// events to every listener of the payload's type, so without the id
/// dragging one slider would move all of them.
struct SliderDrag(ElementId);

impl Render for DragGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Where a fine (modifier-held) drag started, so it moves relative to that
/// point instead of jumping to the cursor. The UI runs on one thread, so a
/// thread-local is enough.
struct FineAnchor {
    id: ElementId,
    x: f32,
    value: f32,
    /// The precision factor this anchor was taken at.
    factor: f32,
}

thread_local! {
    static FINE_ANCHOR: RefCell<Option<FineAnchor>> = const { RefCell::new(None) };
}

/// How much a drag is slowed down by held modifiers: Shift = 10x finer,
/// Alt/Option = 50x finer, otherwise 1 (absolute positioning).
fn precision_factor(modifiers: gpui::Modifiers) -> f32 {
    if modifiers.alt {
        0.02
    } else if modifiers.shift {
        0.1
    } else {
        1.0
    }
}

/// A slider value for display: whole numbers when it is one, otherwise a
/// decimal so fine adjustments are visible.
fn format_value(value: f32, signed: bool) -> String {
    let fractional = (value - value.round()).abs() > 0.05;
    match (signed, fractional) {
        (true, true) => format!("{value:+.1}"),
        (true, false) => format!("{value:+.0}"),
        (false, true) => format!("{value:.1}"),
        (false, false) => format!("{value:.0}"),
    }
}

/// What a slider's track looks like.
#[derive(Clone, Copy)]
pub enum SliderTrack {
    /// Neutral track with an accent fill from the default to the value.
    Accent,
    /// A colour ramp from left to right (temperature, a colour's
    /// saturation, …), with no fill.
    Gradient(Hsla, Hsla),
    /// The full hue wheel, for picking a hue.
    Hue,
}

/// A slider's range and presentation.
#[derive(Clone, Copy)]
pub struct SliderSpec {
    pub min: f32,
    pub max: f32,
    /// What double-clicking the label resets to.
    pub default: f32,
    /// Show the value as an offset ("+12") rather than an amount ("12").
    pub signed: bool,
    pub track: SliderTrack,
}

/// A horizontal, draggable slider for a single develop adjustment. `value`
/// is expected to already be clamped to the spec's range by the caller
/// (the `Adjustment` enum in `rustyedit-core` owns that clamping).
pub fn labeled_slider(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    value: f32,
    spec: SliderSpec,
    theme: &Theme,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let theme = *theme;
    let id = id.into();
    let SliderSpec {
        min,
        max,
        default,
        signed,
        track,
    } = spec;
    let to_fraction = |v: f32| ((v - min) / (max - min).max(f32::EPSILON)).clamp(0.0, 1.0);
    let fraction = to_fraction(value);
    let on_change = Rc::new(on_change);

    let track_id = id.clone();
    let reset = {
        let on_change = on_change.clone();
        move |_: &MouseDownEvent, window: &mut Window, cx: &mut App| on_change(default, window, cx)
    };
    let shown = format_value(value, signed);

    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .flex_row()
                .justify_between()
                .text_xs()
                .child(
                    div()
                        .id(SharedString::from(format!("{:?}-label", track_id)))
                        .text_color(theme.text_muted)
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            move |event: &MouseDownEvent, window, cx| {
                                if event.click_count >= 2 {
                                    reset(event, window, cx);
                                }
                            },
                        )
                        .child(label.into()),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .child(div().text_color(theme.text).child(shown))
                        .child(reset_button(
                            SharedString::from(format!("{:?}-reset", track_id)),
                            (value - default).abs() > 0.05,
                            &theme,
                            {
                                let on_change = on_change.clone();
                                move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                                    on_change(default, window, cx)
                                }
                            },
                        )),
                ),
        )
        .child({
            let on_change = on_change.clone();
            let drag_id = track_id.clone();
            div()
                .id(track_id.clone())
                .relative()
                .h(px(14.0))
                .w_full()
                .flex()
                .items_center()
                .child(track_background(track, &theme))
                .when(matches!(track, SliderTrack::Accent), |el| {
                    // Fill from the neutral point to the value, so a
                    // bipolar slider fills left or right of centre.
                    let (lo, hi) = {
                        let d = to_fraction(default);
                        (d.min(fraction), d.max(fraction))
                    };
                    el.child(
                        div()
                            .absolute()
                            .left(relative(lo))
                            .w(relative(hi - lo))
                            .h(px(4.0))
                            .rounded_full()
                            .bg(theme.accent),
                    )
                })
                .child(
                    div()
                        .absolute()
                        .left(relative(fraction))
                        .top(px(1.0))
                        .size(px(12.0))
                        .ml(px(-6.0))
                        .rounded_full()
                        .bg(theme.text)
                        .border_1()
                        .border_color(theme.surface)
                        .shadow_sm(),
                )
                // Shift/Alt + scroll nudges the value: 1% of the range per
                // notch with Shift, 0.1% with Alt. Plain scrolling is left
                // to the panel.
                .on_scroll_wheel({
                    let on_change = on_change.clone();
                    move |event: &gpui::ScrollWheelEvent, window: &mut Window, cx: &mut App| {
                        let unit = if event.modifiers.alt {
                            0.001
                        } else if event.modifiers.shift {
                            0.01
                        } else {
                            return;
                        };
                        let delta = event.delta.pixel_delta(px(16.0));
                        // Shift turns a vertical wheel into a horizontal one
                        // on some platforms, so take whichever axis moved.
                        let (dx, dy) = (f32::from(delta.x), f32::from(delta.y));
                        let notches = if dy.abs() >= dx.abs() { dy } else { dx } / 16.0;
                        cx.stop_propagation();
                        let next = (value + notches.clamp(-4.0, 4.0) * unit * (max - min))
                            .clamp(min, max);
                        on_change(next, window, cx);
                    }
                })
                // A new press starts a new drag, so forget any fine anchor.
                .on_mouse_down(MouseButton::Left, |_, _, _| {
                    FINE_ANCHOR.with(|a| *a.borrow_mut() = None);
                })
                .on_drag(SliderDrag(track_id), move |_, _, _, cx| {
                    cx.new(|_| DragGhost)
                })
                .on_drag_move(
                    move |event: &DragMoveEvent<SliderDrag>, window: &mut Window, cx: &mut App| {
                        if event.drag(cx).0 != drag_id {
                            return;
                        }
                        let x = f32::from(event.event.position.x);
                        let origin_x = f32::from(event.bounds.origin.x);
                        let width = f32::from(event.bounds.size.width).max(1.0);
                        let factor = precision_factor(event.event.modifiers);
                        if factor >= 1.0 {
                            FINE_ANCHOR.with(|a| *a.borrow_mut() = None);
                            let fraction = ((x - origin_x) / width).clamp(0.0, 1.0);
                            on_change(min + fraction * (max - min), window, cx);
                            return;
                        }
                        // Fine drag: move relative to where the modifier was
                        // pressed, scaled down. Pressing or releasing a
                        // modifier mid-drag re-anchors, so there's no jump.
                        let (anchor_x, anchor_value) = FINE_ANCHOR.with(|a| {
                            let mut a = a.borrow_mut();
                            match a.as_ref() {
                                Some(f) if f.id == drag_id && f.factor == factor => (f.x, f.value),
                                _ => {
                                    *a = Some(FineAnchor {
                                        id: drag_id.clone(),
                                        x,
                                        value,
                                        factor,
                                    });
                                    (x, value)
                                }
                            }
                        });
                        let delta = (x - anchor_x) / width * (max - min) * factor;
                        on_change((anchor_value + delta).clamp(min, max), window, cx);
                    },
                )
        })
}

/// The track behind the handle.
fn track_background(track: SliderTrack, theme: &Theme) -> AnyElement {
    let base = || {
        div()
            .absolute()
            .left_0()
            .right_0()
            .h(px(4.0))
            .rounded_full()
            .overflow_hidden()
    };
    match track {
        SliderTrack::Accent => base().bg(theme.border).into_any_element(),
        SliderTrack::Gradient(from, to) => base()
            .bg(linear_gradient(
                90.0,
                linear_color_stop(from, 0.0),
                linear_color_stop(to, 1.0),
            ))
            .into_any_element(),
        // Gradients take two stops, so the wheel is six segments.
        SliderTrack::Hue => base()
            .flex()
            .flex_row()
            .children((0..6).map(|i| {
                let hue = |n: usize| hsla((n % 6) as f32 / 6.0, 0.75, 0.55, 1.0);
                div().flex_1().h_full().bg(linear_gradient(
                    90.0,
                    linear_color_stop(hue(i), 0.0),
                    linear_color_stop(hue(i + 1), 1.0),
                ))
            }))
            .into_any_element(),
    }
}

/// The small "reset to default" button beside a slider's value. It keeps
/// its space while hidden (at the default) so values don't jump around.
pub fn reset_button(
    id: impl Into<ElementId>,
    visible: bool,
    theme: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let theme = *theme;
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(16.0))
        .when(!visible, |el| el.invisible())
        .cursor_pointer()
        .hover(|el| el.bg(theme.surface_elevated))
        .child(icon(Icon::Reset, 11.0, theme.text_muted))
        .tooltip(tip("Reset to default", &theme))
        .on_click(on_click)
}

/// A label-less horizontal slider over `0..=1`, for toolbars and the
/// footer: a thin track, an accent fill and a handle.
pub fn compact_slider(
    id: impl Into<ElementId>,
    fraction: f32,
    width: f32,
    theme: &Theme,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let theme = *theme;
    let id = id.into();
    let drag_id = id.clone();
    let fraction = fraction.clamp(0.0, 1.0);

    div()
        .id(id.clone())
        .relative()
        .flex()
        .items_center()
        .w(px(width))
        .h(px(14.0))
        .cursor_pointer()
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .h(px(3.0))
                .rounded_full()
                .bg(theme.border),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .w(relative(fraction))
                .h(px(3.0))
                .rounded_full()
                .bg(theme.accent),
        )
        .child(
            div()
                .absolute()
                .left(relative(fraction))
                .size(px(10.0))
                .ml(px(-5.0))
                .rounded_full()
                .bg(theme.text)
                .border_1()
                .border_color(theme.surface)
                .shadow_sm(),
        )
        .on_drag(SliderDrag(id), move |_, _, _, cx| cx.new(|_| DragGhost))
        .on_drag_move(
            move |event: &DragMoveEvent<SliderDrag>, window: &mut Window, cx: &mut App| {
                if event.drag(cx).0 != drag_id {
                    return;
                }
                let x = f32::from(event.event.position.x - event.bounds.origin.x);
                let width = f32::from(event.bounds.size.width).max(1.0);
                on_change((x / width).clamp(0.0, 1.0), window, cx);
            },
        )
}
