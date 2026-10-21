use std::rc::Rc;

use gpui::{
    div, hsla, linear_color_stop, linear_gradient, prelude::*, px, relative, AnyElement, App,
    Context, DragMoveEvent, ElementId, Hsla, MouseButton, MouseDownEvent, Render, SharedString,
    Window,
};

use crate::theme::Theme;

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
    let shown = if signed {
        format!("{value:+.0}")
    } else {
        format!("{value:.0}")
    };

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
                .child(div().text_color(theme.text).child(shown)),
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
                        let fraction = ((x - origin_x) / width).clamp(0.0, 1.0);
                        on_change(min + fraction * (max - min), window, cx);
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
