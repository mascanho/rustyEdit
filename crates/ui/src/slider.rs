use std::rc::Rc;

use gpui::{
    div, prelude::*, px, relative, App, Context, DragMoveEvent, ElementId, MouseButton,
    MouseDownEvent, Render, SharedString, Window,
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

/// A horizontal, draggable slider for a single develop adjustment. `value`
/// is expected to already be clamped to `[min, max]` by the caller (the
/// `Adjustment` enum in `rustyedit-core` owns that clamping).
pub fn labeled_slider(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    value: f32,
    min: f32,
    max: f32,
    theme: &Theme,
    on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let theme = *theme;
    let id = id.into();
    let fraction = ((value - min) / (max - min).max(f32::EPSILON)).clamp(0.0, 1.0);
    let on_change = Rc::new(on_change);

    let track_id = id.clone();
    let reset = {
        let on_change = on_change.clone();
        move |_: &MouseDownEvent, window: &mut Window, cx: &mut App| on_change(0.0, window, cx)
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
                .child(div().text_color(theme.text).child(format!("{:+.0}", value))),
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
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .h(px(4.0))
                        .rounded_full()
                        .bg(theme.border),
                )
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .h(px(4.0))
                        .w(relative(fraction))
                        .rounded_full()
                        .bg(theme.accent),
                )
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
                        .border_color(theme.surface),
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
