//! Sync Settings: copies the chosen Develop sections of the active photo
//! onto every other selected photo, as one undo step. A dialog lists the
//! sections with a checkbox each; the choice is remembered for next time.

use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context, MouseButton, SharedString, Window};
use rustyedit_core::{DevelopSection, DevelopSettings};
use rustyedit_ui::Theme;

use crate::{actions::SyncSettings, develop::HistoryNote, workspace::Workspace};

const DIALOG_WIDTH: f32 = 380.0;

/// The open dialog's checklist.
pub(crate) struct SyncDialog {
    pub(crate) chosen: Vec<DevelopSection>,
}

/// What's ticked the first time: the look, plus lens corrections. Crop,
/// spot removal and gradients belong to one picture's composition, so
/// they're opt-in (as in Lightroom).
pub(crate) fn default_choice() -> Vec<DevelopSection> {
    let mut chosen = DevelopSection::LOOK.to_vec();
    chosen.push(DevelopSection::Lens);
    chosen
}

/// `own` with the `chosen` sections (values and on/off switch) taken from
/// `source`.
fn synced(own: &DevelopSettings, source: &DevelopSettings, chosen: &[DevelopSection]) -> DevelopSettings {
    let mut out = *own;
    for &section in chosen {
        section.copy(source, &mut out);
        out.set_section_enabled(section, source.is_section_enabled(section));
    }
    out
}

impl Workspace {
    pub fn sync_settings_action(&mut self, _: &SyncSettings, _w: &mut Window, cx: &mut Context<Self>) {
        self.open_sync_dialog(cx);
    }

    /// The other selected photos, which receive the active photo's
    /// settings.
    fn sync_targets(&self) -> Vec<usize> {
        let active = self.catalog.selected;
        self.target_indices()
            .into_iter()
            .filter(|&ix| ix != active)
            .filter(|&ix| self.catalog.assets.get(ix).is_some_and(|a| !a.missing))
            .collect()
    }

    pub(crate) fn can_sync(&self) -> bool {
        self.catalog.selected_asset().is_some() && !self.sync_targets().is_empty()
    }

    pub(crate) fn open_sync_dialog(&mut self, cx: &mut Context<Self>) {
        if !self.can_sync() {
            self.status_message =
                Some("Select two or more photos to sync the active photo's settings.".into());
            cx.notify();
            return;
        }
        self.sync_dialog = Some(SyncDialog {
            chosen: self.sync_choice.clone(),
        });
        cx.notify();
    }

    pub(crate) fn close_sync_dialog(&mut self, cx: &mut Context<Self>) {
        self.sync_dialog = None;
        cx.notify();
    }

    fn set_sync_sections(&mut self, sections: Vec<DevelopSection>, cx: &mut Context<Self>) {
        if let Some(dialog) = &mut self.sync_dialog {
            dialog.chosen = sections;
        }
        cx.notify();
    }

    fn toggle_sync_section(&mut self, section: DevelopSection, cx: &mut Context<Self>) {
        if let Some(dialog) = &mut self.sync_dialog {
            match dialog.chosen.iter().position(|s| *s == section) {
                Some(at) => {
                    dialog.chosen.remove(at);
                }
                None => dialog.chosen.push(section),
            }
        }
        cx.notify();
    }

    /// Applies the ticked sections to the other selected photos.
    fn run_sync(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.sync_dialog.take() else {
            return;
        };
        let Some(source) = self.catalog.selected_asset() else {
            return;
        };
        let (settings, name) = (source.develop, source.display_name());
        let targets = self.sync_targets();
        let chosen = dialog.chosen;
        self.sync_choice = chosen.clone();
        if chosen.is_empty() || targets.is_empty() {
            cx.notify();
            return;
        }
        let count = targets.len();
        let detail = format!("From {name}");
        let sections = chosen.clone();
        self.replace_develop(
            targets,
            "sync settings",
            HistoryNote::Record {
                title: "Sync settings",
                detail: &detail,
            },
            cx,
            move |own| synced(own, &settings, &sections),
        );
        self.status_message = Some(
            format!(
                "Synced {} section{} to {count} photo{}.",
                chosen.len(),
                if chosen.len() == 1 { "" } else { "s" },
                if count == 1 { "" } else { "s" },
            )
            .into(),
        );
        cx.notify();
    }

    pub(crate) fn render_sync_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let Some(dialog) = &self.sync_dialog else {
            return div().into_any_element();
        };
        let source = self.catalog.selected_asset();
        let source_develop = source.map(|a| a.develop).unwrap_or_default();
        let source_name = source.map(|a| a.display_name()).unwrap_or_default();
        let targets = self.sync_targets().len();

        let rows: Vec<AnyElement> = DevelopSection::ALL
            .iter()
            .map(|&section| {
                let checked = dialog.chosen.contains(&section);
                let edited = !section.is_default(&source_develop);
                section_row(section, checked, edited, &theme, cx)
            })
            .collect();

        let all = DevelopSection::ALL.to_vec();
        let header = div()
            .flex()
            .flex_col()
            .gap_0p5()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Sync Settings"),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .child(format!(
                        "From {source_name} to {targets} other photo{}",
                        if targets == 1 { "" } else { "s" }
                    )),
            );

        let footer = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .child(rustyedit_ui::chrome_button(
                        "sync-check-all",
                        "Check All",
                        false,
                        &theme,
                        cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            this.set_sync_sections(all.clone(), cx)
                        }),
                    ))
                    .child(rustyedit_ui::chrome_button(
                        "sync-check-none",
                        "Check None",
                        false,
                        &theme,
                        cx.listener(|this, _: &ClickEvent, _w, cx| {
                            this.set_sync_sections(Vec::new(), cx)
                        }),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .child(rustyedit_ui::chrome_button(
                        "sync-cancel",
                        "Cancel",
                        false,
                        &theme,
                        cx.listener(|this, _: &ClickEvent, _w, cx| this.close_sync_dialog(cx)),
                    ))
                    .child(rustyedit_ui::chrome_button(
                        "sync-run",
                        "Synchronize",
                        !dialog.chosen.is_empty(),
                        &theme,
                        cx.listener(|this, _: &ClickEvent, _w, cx| this.run_sync(cx)),
                    )),
            );

        let card = div()
            .id("sync-card")
            .flex()
            .flex_col()
            .w(px(DIALOG_WIDTH))
            .rounded_lg()
            .overflow_hidden()
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            // Clicks inside the card must not reach the backdrop.
            .on_mouse_down(MouseButton::Left, |_, _w, cx| cx.stop_propagation())
            .child(header)
            .child(div().flex().flex_col().py_1().children(rows))
            .child(footer);

        div()
            .id("sync-backdrop")
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.55))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _w, cx| this.close_sync_dialog(cx)),
            )
            .child(card)
            .into_any_element()
    }
}

/// One section with its checkbox; "edited" marks sections the active
/// photo has changed.
fn section_row(
    section: DevelopSection,
    checked: bool,
    edited: bool,
    theme: &Theme,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let theme = *theme;
    let checkbox = div()
        .flex()
        .items_center()
        .justify_center()
        .size(px(14.0))
        .rounded_sm()
        .border_1()
        .border_color(if checked { theme.accent } else { theme.border })
        .when(checked, |el| el.bg(theme.accent))
        .text_size(px(10.0))
        .text_color(theme.surface)
        .when(checked, |el| el.child("\u{2713}"));
    div()
        .id(SharedString::from(format!("sync-{}", section.key())))
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .h(px(26.0))
        .px_4()
        .cursor_pointer()
        .hover(|el| el.bg(theme.surface_elevated))
        .child(checkbox)
        .child(
            div()
                .flex_1()
                .text_size(px(12.0))
                .text_color(if checked { theme.text } else { theme.text_muted })
                .child(section.label()),
        )
        .when(edited, |el| {
            el.child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme.text_faint)
                    .child("edited"),
            )
        })
        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
            this.toggle_sync_section(section, cx)
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syncs_only_the_chosen_sections_and_their_switches() {
        let mut source = DevelopSettings {
            exposure: 40.0,
            temperature: -20.0,
            vignette: -30.0,
            ..Default::default()
        };
        source.set_section_enabled(DevelopSection::Effects, false);
        let own = DevelopSettings {
            exposure: -10.0,
            temperature: 15.0,
            ..Default::default()
        };
        let out = synced(
            &own,
            &source,
            &[DevelopSection::Basic, DevelopSection::Effects],
        );
        assert_eq!(out.exposure, 40.0);
        assert_eq!(out.vignette, -30.0);
        assert!(!out.is_section_enabled(DevelopSection::Effects));
        // Color wasn't chosen: the photo keeps its own white balance.
        assert_eq!(out.temperature, 15.0);
    }
}
