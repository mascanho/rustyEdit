//! A dropdown for picking one of several things in the editing panel: a
//! field showing the current choice that opens a list over the panel.

use gpui::{
    deferred, div, prelude::*, AnyElement, App, ClickEvent, Context, MouseDownEvent,
    SharedString, Window,
};
use rustyedit_ui::{sp, Icon};

use crate::workspace::Workspace;

/// One choice in a dropdown.
pub(super) struct Choice {
    pub(super) label: String,
    pub(super) tip: Option<&'static str>,
    pub(super) action: Box<dyn Fn(&mut Workspace, &mut Context<Workspace>)>,
}

impl Workspace {
    /// A dropdown named `id` showing `current`, offering `choices`. Only one
    /// dropdown is open at a time; picking a choice closes it.
    pub(super) fn dropdown(
        &self,
        id: &'static str,
        current: &str,
        selected: usize,
        choices: Vec<Choice>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let open = self.develop_ui.open_dropdown == Some(id);

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
            .border_color(if open { theme.accent } else { theme.border_subtle })
            .hover(|el| el.border_color(theme.border))
            .child(div().min_w(sp(0.0)).truncate().child(current.to_string()))
            .child(rustyedit_ui::icon(Icon::ChevronDown, 12.0, theme.text_muted))
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                this.develop_ui.open_dropdown = if this.develop_ui.open_dropdown == Some(id) {
                    None
                } else {
                    Some(id)
                };
                cx.notify();
            }));

        let entity = cx.entity();
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
                .on_click(move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                    entity.update(cx, |this, cx| {
                        action(this, cx);
                        this.develop_ui.open_dropdown = None;
                        cx.notify();
                    });
                })
        });

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
            .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, _w, cx| {
                if this.develop_ui.open_dropdown == Some(id) {
                    this.develop_ui.open_dropdown = None;
                    cx.notify();
                }
            }))
            .children(items);

        div()
            .relative()
            .w_full()
            .child(field)
            .when(open, |el| el.child(deferred(menu).with_priority(1)))
            .into_any_element()
    }
}

/// A choice that sets something on the selected photo's settings.
pub(super) fn choice(
    label: impl Into<String>,
    tip: Option<&'static str>,
    action: impl Fn(&mut Workspace, &mut Context<Workspace>) + 'static,
) -> Choice {
    Choice { label: label.into(), tip, action: Box::new(action) }
}
