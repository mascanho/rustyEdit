//! The editing panel's sections, each with a title bar holding a drag
//! grip, a reset button (shown once anything in it is changed) and an
//! on/off switch.
//!
//! - A switched-off section keeps its values but stops applying them; its
//!   controls fade until it's switched back on.
//! - Dragging a section's grip onto another moves it there. The order is a
//!   preference (it applies to every photo) and can be reset from the
//!   bottom of the panel.

use rustyedit_ui::sp;
use gpui::{
    div, prelude::*, AnyElement, ClickEvent, Context, MouseButton, SharedString, Window,
};
use rustyedit_core::DevelopSection;
use rustyedit_ui::{Icon, SectionExtras, Theme};

use crate::workspace::Workspace;

/// Drag payload: the section being moved.
#[derive(Clone, Copy)]
struct SectionDrag(DevelopSection);

/// What follows the pointer while a section is dragged: its name.
struct SectionGhost {
    title: &'static str,
    theme: Theme,
}

impl Render for SectionGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(sp(28.0))
            .px_3()
            .rounded_md()
            .bg(theme.surface_elevated)
            .border_1()
            .border_color(theme.accent)
            .shadow_lg()
            .text_size(sp(12.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(theme.text)
            .child(rustyedit_ui::icon(Icon::Grip, 12.0, theme.accent))
            .child(self.title)
    }
}

impl Workspace {
    /// The editing sections in the user's order.
    pub(super) fn develop_section_order(&self) -> Vec<DevelopSection> {
        DevelopSection::arrange(&self.settings.develop.section_order)
    }

    /// `section` as a collapsible block holding `children`; `key`
    /// remembers whether it's folded. Other sections can be dropped on it.
    pub(super) fn develop_section(
        &self,
        section: DevelopSection,
        key: &'static str,
        children: Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let develop = self
            .catalog
            .selected_asset()
            .map(|asset| asset.develop)
            .unwrap_or_default();
        let enabled = develop.is_section_enabled(section);
        let edited = !section.is_default(&develop);

        let reset = rustyedit_ui::reset_button(
            SharedString::from(format!("{key}-reset")),
            edited,
            &theme,
            cx.listener(move |this, _: &ClickEvent, _w, cx| this.reset_section(section, cx)),
        )
        .into_any_element();
        let switch = rustyedit_ui::mini_toggle_switch(
            SharedString::from(format!("{key}-switch")),
            enabled,
            &theme,
            cx.listener(move |this, _: &ClickEvent, _w, cx| {
                this.set_section_enabled(section, !enabled, cx)
            }),
        )
        .into_any_element();

        let block = rustyedit_ui::disclosure_section_with(
            key,
            &theme,
            section.label(),
            !self.collapsed_sections.contains(key),
            cx.listener(move |this, _: &ClickEvent, _window, cx| {
                if !this.collapsed_sections.remove(key) {
                    this.collapsed_sections.insert(key);
                }
                cx.notify();
            }),
            SectionExtras {
                trailing: vec![reset, switch, grip(section, key, &theme)],
                dimmed: !enabled,
            },
            children,
        );

        let entity = cx.entity();
        div()
            .id(SharedString::from(format!("{key}-drop")))
            .rounded_md()
            .border_t_2()
            .border_color(gpui::transparent_black())
            .drag_over::<SectionDrag>(move |style, dragged, _, _| {
                if dragged.0 == section {
                    style
                } else {
                    style
                        .border_color(theme.accent)
                        .bg(theme.accent.opacity(0.06))
                }
            })
            .on_drop(move |dragged: &SectionDrag, _window, cx| {
                entity.update(cx, |this, cx| this.move_section(dragged.0, section, cx));
            })
            .child(block)
            .into_any_element()
    }

    /// "Reset order" under the sections, once they've been rearranged.
    pub(super) fn section_order_footer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.develop_section_order() == DevelopSection::ALL.to_vec() {
            return None;
        }
        let theme = self.theme;
        Some(
            div()
                .flex()
                .justify_center()
                .py_3()
                .child(
                    div()
                        .id("develop-reset-order")
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .text_size(sp(10.5))
                        .text_color(theme.text_faint)
                        .cursor_pointer()
                        .hover(|el| el.text_color(theme.accent))
                        .child(rustyedit_ui::icon(Icon::Reset, 11.0, theme.text_faint))
                        .child("Reset panel order")
                        .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                            this.update_settings(|s| s.develop.section_order.clear(), cx);
                        })),
                )
                .into_any_element(),
        )
    }

    /// Moves `section` to where `onto` is, and remembers the new order.
    fn move_section(
        &mut self,
        section: DevelopSection,
        onto: DevelopSection,
        cx: &mut Context<Self>,
    ) {
        if section == onto {
            return;
        }
        let order = section.moved_onto(onto, &self.develop_section_order());
        let keys = order.iter().map(|s| s.key().to_string()).collect();
        self.update_settings(|s| s.develop.section_order = keys, cx);
    }

    /// Puts every setting in `section` back to its default.
    fn reset_section(&mut self, section: DevelopSection, cx: &mut Context<Self>) {
        self.edit_develop(
            &format!("Reset {}", section.label()),
            |develop| section.reset(develop),
            |_| (String::new(), "Defaults".to_string()),
            cx,
        );
    }

    /// Switches `section`'s edits on or off without changing them.
    fn set_section_enabled(
        &mut self,
        section: DevelopSection,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        self.edit_develop(
            &format!("{} panel", section.label()),
            |develop| develop.set_section_enabled(section, enabled),
            |_| {
                let state = if enabled { "On" } else { "Off" };
                (String::new(), state.to_string())
            },
            cx,
        );
    }
}

/// The handle a section is dragged by.
fn grip(section: DevelopSection, key: &'static str, theme: &Theme) -> AnyElement {
    let theme = *theme;
    div()
        .id(SharedString::from(format!("{key}-grip")))
        .flex()
        .items_center()
        .justify_center()
        .size(sp(18.0))
        .rounded_sm()
        .cursor_grab()
        .hover(|el| el.bg(theme.surface_elevated))
        .child(rustyedit_ui::icon(Icon::Grip, 12.0, theme.text_faint))
        .tooltip(rustyedit_ui::tip("Drag to move this section", &theme))
        // Don't let the press reach the title bar and fold the section.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_drag(SectionDrag(section), move |drag, _offset, _window, cx| {
            cx.new(|_| SectionGhost {
                title: drag.0.label(),
                theme,
            })
        })
        .into_any_element()
}
