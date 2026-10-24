use crate::sp;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

use gpui::{
    div, hsla, linear_color_stop, linear_gradient, prelude::*, px, relative, AnyElement, App,
    ClickEvent, Context, CursorStyle, DragMoveEvent, ElementId, Hsla, Keystroke, MouseButton,
    MouseDownEvent, Render, SharedString, Window,
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
    static KEYBOARD: RefCell<Keyboard> = RefCell::default();
}

type OnChange = Rc<dyn Fn(f32, &mut Window, &mut App)>;

/// A labelled slider as of its last render, so keystrokes can change it
/// between renders.
struct Registered {
    value: f32,
    min: f32,
    max: f32,
    on_change: OnChange,
}

/// A value being typed into a slider.
struct ValueEntry {
    id: ElementId,
    text: String,
    /// The whole value is selected: the next character replaces it.
    replace: bool,
}

/// Keyboard state for labelled sliders: which one the arrow keys move
/// (the hovered one, else the last one clicked) and any typed value.
#[derive(Default)]
struct Keyboard {
    sliders: HashMap<ElementId, Registered>,
    hovered: Option<ElementId>,
    focused: Option<ElementId>,
    entry: Option<ValueEntry>,
}

/// How far one arrow press moves a slider: one unit on the usual wide
/// ranges, a tenth on narrow ones, and ten times finer with Shift (as
/// Shift makes drags finer).
fn arrow_step(min: f32, max: f32, fine: bool) -> f32 {
    let step = if max - min >= 20.0 { 1.0 } else { 0.1 };
    if fine {
        step / 10.0
    } else {
        step
    }
}

/// `value` moved one `step` in `direction`, landing on a multiple of the
/// step, so 12.4 goes to 13 or 12 rather than 13.4 or 11.4.
fn step_value(value: f32, step: f32, direction: f32) -> f32 {
    let steps = value / step;
    let next = if direction > 0.0 {
        (steps + 1e-3).floor() + 1.0
    } else {
        (steps - 1e-3).ceil() - 1.0
    };
    next * step
}

/// Handles a keystroke for the labelled sliders before any shortcut sees
/// it: typing into a value being entered, and, when `arrows` is set, the
/// arrow keys nudging the hovered (or last clicked) slider. Returns
/// whether the keystroke was used; if so, stop it propagating.
pub fn slider_keystroke(
    keystroke: &Keystroke,
    arrows: bool,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    if KEYBOARD.with(|k| k.borrow().entry.is_some()) {
        return entry_keystroke(keystroke, window, cx);
    }
    if !arrows {
        return false;
    }
    let direction = match keystroke.key.as_str() {
        "right" | "up" => 1.0,
        "left" | "down" => -1.0,
        _ => return false,
    };
    let m = &keystroke.modifiers;
    if m.platform || m.control || m.alt {
        return false;
    }
    let change = KEYBOARD.with(|k| {
        let mut k = k.borrow_mut();
        let id = k.hovered.clone().or_else(|| k.focused.clone())?;
        let slider = k.sliders.get_mut(&id)?;
        let step = arrow_step(slider.min, slider.max, m.shift);
        let next = step_value(slider.value, step, direction).clamp(slider.min, slider.max);
        // Remember it now, so presses faster than renders keep adding up.
        slider.value = next;
        Some((next, slider.on_change.clone()))
    });
    let Some((value, on_change)) = change else {
        return false;
    };
    on_change(value, window, cx);
    true
}

/// Typing into a slider's value: digits, sign and decimal point; Enter or
/// Tab applies it, Escape cancels. Every other key is swallowed so it
/// can't trigger a shortcut, except Cmd/Ctrl combinations.
fn entry_keystroke(keystroke: &Keystroke, window: &mut Window, cx: &mut App) -> bool {
    let m = &keystroke.modifiers;
    if m.platform || m.control {
        return false;
    }
    match keystroke.key.as_str() {
        "enter" | "tab" => {
            commit_entry(window, cx);
            return true;
        }
        "escape" => KEYBOARD.with(|k| k.borrow_mut().entry = None),
        "backspace" => KEYBOARD.with(|k| {
            if let Some(entry) = k.borrow_mut().entry.as_mut() {
                if entry.replace {
                    entry.text.clear();
                    entry.replace = false;
                } else {
                    entry.text.pop();
                }
            }
        }),
        _ => {
            let typed = keystroke
                .key_char
                .as_deref()
                .filter(|t| !t.is_empty() && t.chars().all(|c| "0123456789.,+-".contains(c)));
            if let Some(typed) = typed {
                KEYBOARD.with(|k| {
                    if let Some(entry) = k.borrow_mut().entry.as_mut() {
                        if entry.replace {
                            entry.text.clear();
                            entry.replace = false;
                        }
                        entry.text.push_str(&typed.replace(',', "."));
                    }
                });
            }
        }
    }
    window.refresh();
    true
}

/// Applies a typed value, clamped to the slider's range. Text that isn't
/// a number leaves the slider as it was.
fn commit_entry(window: &mut Window, cx: &mut App) {
    let change = KEYBOARD.with(|k| {
        let mut k = k.borrow_mut();
        let entry = k.entry.take()?;
        let slider = k.sliders.get_mut(&entry.id)?;
        let value = entry
            .text
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())?;
        let value = value.clamp(slider.min, slider.max);
        slider.value = value;
        Some((value, slider.on_change.clone()))
    });
    if let Some((value, on_change)) = change {
        on_change(value, window, cx);
    }
    window.refresh();
}

/// Call on every mouse press in the window, before it reaches any
/// element: applies a value being typed and forgets which slider was
/// last clicked (a slider pressed now claims it again).
pub fn sliders_pointer_down(window: &mut Window, cx: &mut App) {
    let (editing, focused) = KEYBOARD.with(|k| {
        let k = k.borrow();
        (k.entry.is_some(), k.focused.is_some())
    });
    if editing {
        commit_entry(window, cx);
    }
    if focused {
        KEYBOARD.with(|k| k.borrow_mut().focused = None);
        window.refresh();
    }
}

/// Makes `id` the slider the arrow keys move until something else is
/// clicked.
fn focus_slider(id: &ElementId, window: &mut Window) {
    KEYBOARD.with(|k| k.borrow_mut().focused = Some(id.clone()));
    window.refresh();
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
    /// Neutral track with an accent fill from the left end to the value,
    /// for amounts (a strength, an opacity) whatever their default is.
    Amount,
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

/// How far in from the track's ends the handle's centre can go: its radius
/// plus a little air, so at either end it sits clear of the border rather
/// than flush against (or over) it. The track and fill run the same
/// stretch, and so does the pointer's reach.
const LABELED_INSET: f32 = 9.0;
const COMPACT_INSET: f32 = 8.0;

/// Where `x` falls along a slider of `width` starting at `origin`, over the
/// stretch left after `inset` pixels at each end (0..=1).
fn fraction_along(x: f32, origin: f32, width: f32, inset: f32) -> f32 {
    ((x - origin - inset) / (width - 2.0 * inset).max(1.0)).clamp(0.0, 1.0)
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

    // Register for the keyboard, and pick up this slider's keyboard state.
    let (focused, entry) = KEYBOARD.with(|k| {
        let mut k = k.borrow_mut();
        let dyn_change: OnChange = on_change.clone();
        k.sliders.insert(
            id.clone(),
            Registered {
                value,
                min,
                max,
                on_change: dyn_change,
            },
        );
        let entry = k
            .entry
            .as_ref()
            .filter(|e| e.id == id)
            .map(|e| (e.text.clone(), e.replace));
        (k.focused.as_ref() == Some(&id), entry)
    });

    let track_id = id.clone();
    let reset = {
        let on_change = on_change.clone();
        move |_: &MouseDownEvent, window: &mut Window, cx: &mut App| on_change(default, window, cx)
    };
    let value_id = SharedString::from(format!("{:?}-value", track_id));
    let value_view = match entry {
        Some((text, replace)) => value_entry(value_id, text, replace, &theme),
        None => {
            let entry_id = id.clone();
            div()
                .id(value_id)
                .px_1()
                .rounded_sm()
                .text_color(theme.text)
                .cursor(CursorStyle::IBeam)
                .hover(|el| el.bg(theme.surface_elevated))
                .tooltip(tip("Click to type a value", &theme))
                .on_mouse_down(MouseButton::Left, move |_, window, _| {
                    KEYBOARD.with(|k| {
                        let mut k = k.borrow_mut();
                        k.entry = Some(ValueEntry {
                            id: entry_id.clone(),
                            text: format_value(value, false),
                            replace: true,
                        });
                        k.focused = Some(entry_id.clone());
                    });
                    window.refresh();
                })
                .child(format_value(value, signed))
                .into_any_element()
        }
    };
    let group = SharedString::from(format!("{:?}-track", track_id));
    let hover_id = id.clone();

    div()
        .id(SharedString::from(format!("{:?}-slider", track_id)))
        .flex()
        .flex_col()
        .gap_1()
        // The hovered slider is the one the arrow keys move.
        .on_hover(move |hovered, _, _| {
            KEYBOARD.with(|k| {
                let mut k = k.borrow_mut();
                if *hovered {
                    k.hovered = Some(hover_id.clone());
                } else if k.hovered.as_ref() == Some(&hover_id) {
                    k.hovered = None;
                }
            })
        })
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
                        .child(value_view)
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
            let press_id = track_id.clone();
            div()
                .id(track_id.clone())
                .group(group.clone())
                .relative()
                .h(sp(14.0))
                .w_full()
                .flex()
                .items_center()
                .cursor(CursorStyle::ResizeLeftRight)
                .child(
                    div()
                        .absolute()
                        .left(sp(LABELED_INSET))
                        .right(sp(LABELED_INSET))
                        .top_0()
                        .bottom_0()
                        .flex()
                        .items_center()
                        .child(track_background(track, &theme))
                        .when(matches!(track, SliderTrack::Accent | SliderTrack::Amount), |el| {
                            // Fill from the neutral point to the value, so a
                            // bipolar slider fills left or right of centre; an
                            // amount always fills from the left.
                            let (lo, hi) = match track {
                                SliderTrack::Amount => (0.0, fraction),
                                _ => {
                                    let d = to_fraction(default);
                                    (d.min(fraction), d.max(fraction))
                                }
                            };
                            el.child(
                                div()
                                    .absolute()
                                    .left(relative(lo))
                                    .w(relative(hi - lo))
                                    .h(sp(4.0))
                                    .rounded_full()
                                    .bg(theme.accent),
                            )
                        })
                        .child(slider_handle(fraction, 12.0, 1.0, focused, group, &theme)),
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
                        let delta = event.delta.pixel_delta(sp(16.0));
                        // Shift turns a vertical wheel into a horizontal one
                        // on some platforms, so take whichever axis moved.
                        let (dx, dy) = (f32::from(delta.x), f32::from(delta.y));
                        let notches = if dy.abs() >= dx.abs() { dy } else { dx } / 16.0;
                        cx.stop_propagation();
                        let next =
                            (value + notches.clamp(-4.0, 4.0) * unit * (max - min)).clamp(min, max);
                        on_change(next, window, cx);
                    }
                })
                // A new press starts a new drag, so forget any fine anchor.
                // The pressed slider is also the one the arrow keys move.
                .on_mouse_down(MouseButton::Left, move |_, window, _| {
                    FINE_ANCHOR.with(|a| *a.borrow_mut() = None);
                    focus_slider(&press_id, window);
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
                            let fraction = fraction_along(x, origin_x, width, f32::from(sp(LABELED_INSET)));
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
                        let reach = (width - 2.0 * f32::from(sp(LABELED_INSET))).max(1.0);
                        let delta = (x - anchor_x) / reach * (max - min) * factor;
                        on_change((anchor_value + delta).clamp(min, max), window, cx);
                    },
                )
        })
}

/// A slider's round handle at `fraction` along a 14px-tall track. It grows
/// and takes the accent colour while the track (`group`) is hovered, and
/// stays accented while `active` (the slider the arrow keys move).
fn slider_handle(
    fraction: f32,
    size: f32,
    top: f32,
    active: bool,
    group: SharedString,
    theme: &Theme,
) -> impl IntoElement {
    let grown = size + 2.0;
    let accent = theme.accent;
    div()
        .absolute()
        .left(relative(fraction))
        .top(px(top))
        .size(px(size))
        .ml(px(-size / 2.0))
        .rounded_full()
        .bg(if active { accent } else { theme.text })
        .border_1()
        .border_color(theme.surface)
        .shadow_sm()
        .group_hover(group, move |style| {
            style
                .bg(accent)
                .size(px(grown))
                .ml(px(-grown / 2.0))
                .top(px(top - 1.0))
        })
}

/// The text field shown in place of a slider's value while one is typed:
/// the text (highlighted while it is all selected) and a caret.
fn value_entry(id: SharedString, text: String, replace: bool, theme: &Theme) -> AnyElement {
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .min_w(sp(32.0))
        .h(sp(16.0))
        .px_1()
        .rounded_sm()
        .border_1()
        .border_color(theme.accent)
        .bg(theme.background)
        .text_color(theme.text)
        .cursor(CursorStyle::IBeam)
        .child(
            div()
                .when(replace && !text.is_empty(), |el| {
                    el.bg(theme.accent.opacity(0.35))
                })
                .child(text),
        )
        .child(div().w(sp(1.0)).h(sp(11.0)).bg(theme.accent))
        .into_any_element()
}

/// The track behind the handle.
fn track_background(track: SliderTrack, theme: &Theme) -> AnyElement {
    let base = || {
        div()
            .absolute()
            .left_0()
            .right_0()
            .h(sp(4.0))
            .rounded_full()
            .overflow_hidden()
    };
    match track {
        SliderTrack::Accent | SliderTrack::Amount => base().bg(theme.border).into_any_element(),
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
        .size(sp(16.0))
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
    let group = SharedString::from(format!("{:?}-track", id));

    div()
        .id(id.clone())
        .group(group.clone())
        .relative()
        .flex()
        .items_center()
        .w(px(width))
        .h(sp(14.0))
        .cursor(CursorStyle::ResizeLeftRight)
        .child(
            div()
                .absolute()
                .left(sp(COMPACT_INSET))
                .right(sp(COMPACT_INSET))
                .top_0()
                .bottom_0()
                .flex()
                .items_center()
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .h(sp(3.0))
                        .rounded_full()
                        .bg(theme.border),
                )
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .w(relative(fraction))
                        .h(sp(3.0))
                        .rounded_full()
                        .bg(theme.accent),
                )
                .child(slider_handle(fraction, 10.0, 2.0, false, group, &theme)),
        )
        .on_drag(SliderDrag(id), move |_, _, _, cx| cx.new(|_| DragGhost))
        .on_drag_move(
            move |event: &DragMoveEvent<SliderDrag>, window: &mut Window, cx: &mut App| {
                if event.drag(cx).0 != drag_id {
                    return;
                }
                let width = f32::from(event.bounds.size.width).max(1.0);
                let origin = f32::from(event.bounds.origin.x);
                let x = f32::from(event.event.position.x);
                on_change(fraction_along(x, origin, width, f32::from(sp(COMPACT_INSET))), window, cx);
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_handle_stays_clear_of_the_ends() {
        // A 200 wide slider with 9px of air each end: the ends of the
        // value range are reached 9px in, and the middle stays the middle.
        assert_eq!(fraction_along(9.0, 0.0, 200.0, 9.0), 0.0);
        assert_eq!(fraction_along(191.0, 0.0, 200.0, 9.0), 1.0);
        assert_eq!(fraction_along(100.0, 0.0, 200.0, 9.0), 0.5);
        // Past the ends is held there; an origin that isn't zero counts.
        assert_eq!(fraction_along(-50.0, 0.0, 200.0, 9.0), 0.0);
        assert_eq!(fraction_along(400.0, 0.0, 200.0, 9.0), 1.0);
        assert_eq!(fraction_along(150.0, 50.0, 200.0, 9.0), 0.5);
    }

    #[test]
    fn arrow_steps_are_a_unit_or_a_tenth() {
        assert_eq!(arrow_step(-100.0, 100.0, false), 1.0);
        assert_eq!(arrow_step(-100.0, 100.0, true), 0.1);
        assert_eq!(arrow_step(0.0, 5.0, false), 0.1);
    }

    #[test]
    fn stepping_lands_on_the_step_grid() {
        assert_eq!(step_value(12.0, 1.0, 1.0), 13.0);
        assert_eq!(step_value(12.0, 1.0, -1.0), 11.0);
        assert_eq!(step_value(12.4, 1.0, 1.0), 13.0);
        assert_eq!(step_value(12.4, 1.0, -1.0), 12.0);
        assert_eq!(step_value(-0.4, 1.0, 1.0), 0.0);
        assert!((step_value(0.3, 0.1, 1.0) - 0.4).abs() < 1e-5);
        assert!((step_value(0.3, 0.1, -1.0) - 0.2).abs() < 1e-5);
    }
}
