use std::cell::RefCell;

use gpui::{
    canvas, div, fill, point, prelude::*, px, size, App, Bounds, Context, DragMoveEvent,
    MouseButton, MouseDownEvent, Pixels, Render, ScrollHandle, SharedString, Window,
};

use crate::theme::Theme;

/// Width of the invisible hit area along the right edge.
const TRACK_WIDTH: f32 = 10.0;
/// Width of the visible thumb, and when hovered or dragged.
const THUMB_WIDTH: f32 = 4.0;
const THUMB_WIDTH_ACTIVE: f32 = 6.0;
const THUMB_MIN_HEIGHT: f32 = 20.0;
const THUMB_INSET: f32 = 2.0;

/// Identifies which scrollbar a drag belongs to. GPUI delivers drag-move
/// events to every listener of the drag's type, so each scrollbar checks
/// the id and ignores drags started on another one.
#[derive(Clone)]
struct ScrollbarDrag(SharedString);

thread_local! {
    /// The scrollbar a drag is currently moving, if any. `has_active_drag`
    /// is true for *every* kind of drag (sliders too), so it can't tell a
    /// scrollbar drag apart on its own.
    static ACTIVE_SCROLLBAR: RefCell<Option<SharedString>> = const { RefCell::new(None) };
}

/// The dragged element must render something; the thumb itself already
/// tracks the cursor, so this stays empty.
struct DragGhost;

impl Render for DragGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// A thin, minimal vertical scrollbar for a scroll container driven by
/// `handle`. Place it as an absolutely positioned sibling *after* the
/// scrolled element inside a `.relative()` wrapper: it overlays the right
/// edge, stays hidden when there's nothing to scroll, brightens and widens
/// on hover, and can be clicked or dragged to scroll.
///
/// The thumb is computed at paint time from the handle, after the scroll
/// container has laid out in the same frame, so it never lags behind.
pub fn vertical_scrollbar(
    id: impl Into<SharedString>,
    handle: ScrollHandle,
    theme: &Theme,
) -> impl IntoElement {
    let theme = *theme;
    let id: SharedString = id.into();

    let jump = {
        let handle = handle.clone();
        move |event: &MouseDownEvent, window: &mut Window, cx: &mut App| {
            cx.stop_propagation();
            scroll_to_pointer(&handle, event.position.y, window);
        }
    };
    let drag = {
        let handle = handle.clone();
        let id = id.clone();
        move |event: &DragMoveEvent<ScrollbarDrag>, window: &mut Window, cx: &mut App| {
            if event.drag(cx).0 == id {
                scroll_to_pointer(&handle, event.event.position.y, window);
            }
        }
    };

    div()
        .id(id.clone())
        .absolute()
        .top_0()
        .bottom_0()
        .right_0()
        .w(px(TRACK_WIDTH))
        // A hover style makes GPUI repaint when the pointer enters/leaves,
        // which is what lets the thumb react to hover below.
        .hover(|style| style.bg(gpui::transparent_black()))
        .on_mouse_down(MouseButton::Left, jump)
        .on_drag(ScrollbarDrag(id.clone()), {
            let id = id.clone();
            move |_, _, _, cx| {
                ACTIVE_SCROLLBAR.with(|a| *a.borrow_mut() = Some(id.clone()));
                cx.new(|_| DragGhost)
            }
        })
        .on_drag_move(drag)
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, cx| {
                    let Some(thumb) = thumb_bounds(&handle, bounds) else {
                        return;
                    };
                    if !cx.has_active_drag() {
                        ACTIVE_SCROLLBAR.with(|a| *a.borrow_mut() = None);
                    }
                    let dragging = cx.has_active_drag()
                        && ACTIVE_SCROLLBAR.with(|a| a.borrow().as_ref() == Some(&id));
                    let active = dragging || bounds.contains(&window.mouse_position());
                    let width = px(if active {
                        THUMB_WIDTH_ACTIVE
                    } else {
                        THUMB_WIDTH
                    });
                    let color = if active {
                        theme.text_muted.opacity(0.7)
                    } else {
                        theme.text_faint.opacity(0.45)
                    };
                    let x = bounds.origin.x + bounds.size.width - width - px(THUMB_INSET);
                    window.paint_quad(
                        fill(
                            Bounds::new(point(x, thumb.origin.y), size(width, thumb.size.height)),
                            color,
                        )
                        .corner_radii(width / 2.0),
                    );
                },
            )
            .size_full(),
        )
}

/// Where the thumb sits within `track`, or `None` when the content fits.
fn thumb_bounds(handle: &ScrollHandle, track: Bounds<Pixels>) -> Option<Bounds<Pixels>> {
    let viewport = f32::from(handle.bounds().size.height);
    let max = f32::from(handle.max_offset().height);
    if viewport <= 0.0 || max <= 0.5 {
        return None;
    }
    let track_height = f32::from(track.size.height) - THUMB_INSET * 2.0;
    let thumb_height = (track_height * viewport / (viewport + max)).max(THUMB_MIN_HEIGHT);
    let progress = (-f32::from(handle.offset().y) / max).clamp(0.0, 1.0);
    let top = THUMB_INSET + progress * (track_height - thumb_height);

    Some(Bounds::new(
        point(track.origin.x, track.origin.y + px(top)),
        size(track.size.width, px(thumb_height)),
    ))
}

/// Scrolls so the thumb's centre follows the pointer's `y`.
fn scroll_to_pointer(handle: &ScrollHandle, y: Pixels, window: &mut Window) {
    let bounds = handle.bounds();
    let viewport = f32::from(bounds.size.height);
    let max = f32::from(handle.max_offset().height);
    if viewport <= 0.0 || max <= 0.0 {
        return;
    }
    let thumb_height = (viewport * viewport / (viewport + max)).max(THUMB_MIN_HEIGHT);
    let travel = (viewport - thumb_height).max(1.0);
    let progress = ((f32::from(y - bounds.origin.y) - thumb_height / 2.0) / travel).clamp(0.0, 1.0);

    let offset = handle.offset();
    handle.set_offset(point(offset.x, px(-progress * max)));
    window.refresh();
}
