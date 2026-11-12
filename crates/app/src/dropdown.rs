//! A dropdown for picking one of several things: a field showing the
//! current choice that opens a list over whatever is behind it. Used by the
//! Develop panel, the Fuji recipe window and the Zines panel. Only one is
//! open at a time (`Workspace::open_dropdown`); picking a choice or
//! clicking away closes it.

use gpui::{
    deferred, div, prelude::*, AnyElement, App, ClickEvent, Context, Entity, MouseDownEvent,
    SharedString, Window,
};
use rustyedit_ui::{sp, Icon, Theme};

use crate::workspace::Workspace;

/// One choice in a dropdown.
pub(crate) struct Choice {
    pub(crate) label: String,
    pub(crate) tip: Option<&'static str>,
    pub(crate) action: Box<dyn Fn(&mut Workspace, &mut Context<Workspace>)>,
}

/// A choice that does `action` when picked.
pub(crate) fn choice(
    label: impl Into<String>,
    tip: Option<&'static str>,
    action: impl Fn(&mut Workspace, &mut Context<Workspace>) + 'static,
) -> Choice {
    Choice { label: label.into(), tip, action: Box::new(action) }
}

/// The dropdown named `id`, showing `current` (`selected` is its index in
/// `choices`); `open` is the name of the dropdown that's open now.
pub(crate) fn dropdown_view(
    theme: &Theme,
    entity: &Entity<Workspace>,
    open: Option<&'static str>,
    id: &'static str,
    current: &str,
    selected: usize,
    choices: Vec<Choice>,
) -> AnyElement {
    let theme = *theme;
    let is_open = open == Some(id);

    let toggle = entity.clone();
    let field = div()
        .id(SharedString::from(format!("{id}-field")))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_1()
        .h(sp(24.0))
        .px_2()
        .rounded_md()
        .border_1()
        .cursor_pointer()
        .text_size(sp(rustyedit_ui::type_size::BODY))
        .text_color(theme.text)
        .bg(theme.background)
        .border_color(if is_open { theme.accent } else { theme.border_subtle })
        .hover(|el| el.border_color(theme.border))
        .child(div().min_w(sp(0.0)).truncate().child(current.to_string()))
        .child(rustyedit_ui::icon(Icon::ChevronDown, 12.0, theme.text_muted))
        .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            let focus = toggle.update(cx, |this, cx| {
                this.open_dropdown = if this.open_dropdown == Some(id) { None } else { Some(id) };
                cx.notify();
                this.focus_handle.clone()
            });
            // Keep the keyboard shortcuts working after a click here.
            window.focus(&focus);
        });

    let items = choices.into_iter().enumerate().map(|(ix, choice)| {
        let entity = entity.clone();
        let action = choice.action;
        let on = ix == selected;
        div()
            .id(SharedString::from(format!("{id}-{ix}")))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(sp(24.0))
            .px_2()
            .cursor_pointer()
            .text_size(sp(rustyedit_ui::type_size::BODY))
            .text_color(if on { theme.text } else { theme.text_muted })
            .hover(|el| el.bg(theme.accent_muted).text_color(theme.text))
            .when_some(choice.tip, |el, tip| el.tooltip(rustyedit_ui::tip(tip, &theme)))
            .child(div().w(sp(12.0)).text_color(theme.accent).child(if on { "\u{2713}" } else { "" }))
            .child(div().flex_1().min_w(sp(0.0)).truncate().child(choice.label))
            .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                let focus = entity.update(cx, |this, cx| {
                    action(this, cx);
                    this.open_dropdown = None;
                    cx.notify();
                    this.focus_handle.clone()
                });
                window.focus(&focus);
            })
    });

    let away = entity.clone();
    let menu = div()
        .id(SharedString::from(format!("{id}-menu")))
        .absolute()
        .top(sp(26.0))
        .left_0()
        .w_full()
        .max_h(sp(260.0))
        .overflow_y_scroll()
        .py_1()
        .bg(theme.surface_elevated)
        .border_1()
        .border_color(theme.border)
        .rounded_md()
        .shadow_lg()
        .occlude()
        .on_mouse_down_out(move |_: &MouseDownEvent, window: &mut Window, cx: &mut App| {
            let focus = away.update(cx, |this, cx| {
                if this.open_dropdown == Some(id) {
                    this.open_dropdown = None;
                    cx.notify();
                }
                this.focus_handle.clone()
            });
            window.focus(&focus);
        })
        .children(items);

    div()
        .relative()
        .w_full()
        .child(field)
        .when(is_open, |el| el.child(deferred(menu).with_priority(1)))
        .into_any_element()
}

impl Workspace {
    /// [`dropdown_view`] for the workspace's own panels.
    pub(crate) fn dropdown(
        &self,
        id: &'static str,
        current: &str,
        selected: usize,
        choices: Vec<Choice>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        dropdown_view(&self.theme, &cx.entity(), self.open_dropdown, id, current, selected, choices)
    }
}
