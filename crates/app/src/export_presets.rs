//! Export presets and the watermark, in the Export panel:
//!
//! - Presets: built-in setups (Web, Instagram, Client proofs, ...) and the
//!   user's own, one click away at the top of the panel. Applying one keeps
//!   the chosen photos (and the destination, unless the preset has one).
//!   "Save…" stores the current options under a name. Shown while
//!   Settings ▸ Export ▸ Export presets is on.
//! - Watermark: a switch for this export, while Settings ▸ Export ▸
//!   Watermarks is on; the watermark itself is designed in Settings.

use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context, SharedString, Window};
use rustyedit_core::{builtin_export_presets, settings::SETTINGS_PAGES, ExportPreset};

use crate::workspace::{TextTarget, Workspace};

impl Workspace {
    /// Built-in presets, then the user's.
    fn all_export_presets(&self) -> Vec<(ExportPreset, bool)> {
        builtin_export_presets()
            .into_iter()
            .map(|p| (p, true))
            .chain(self.settings.export.presets.iter().cloned().map(|p| (p, false)))
            .collect()
    }

    fn apply_export_preset(&mut self, preset: &ExportPreset, cx: &mut Context<Self>) {
        let options = preset.apply_to(&self.export_options);
        self.export_options = options;
        self.export_message = None;
        self.status_message = Some(format!("Export preset \u{201C}{}\u{201D}", preset.name).into());
        cx.notify();
    }

    /// Saves the current options as a preset called `name` (replacing one
    /// of the same name).
    pub(crate) fn save_export_preset(&mut self, name: String, cx: &mut Context<Self>) {
        let mut name = name.trim().to_string();
        if name.is_empty() {
            name = format!("My preset {}", self.settings.export.presets.len() + 1);
        }
        let preset = ExportPreset {
            name: name.clone(),
            options: self.export_options.clone(),
        };
        self.update_settings(
            |s| {
                let presets = &mut s.export.presets;
                match presets.iter_mut().find(|p| p.name.eq_ignore_ascii_case(&name)) {
                    Some(existing) => *existing = preset,
                    None => presets.push(preset),
                }
            },
            cx,
        );
        self.status_message = Some(format!("Saved export preset \u{201C}{name}\u{201D}").into());
        cx.notify();
    }

    fn delete_export_preset(&mut self, name: String, cx: &mut Context<Self>) {
        self.update_settings(|s| s.export.presets.retain(|p| p.name != name), cx);
        self.status_message = Some(format!("Deleted export preset \u{201C}{name}\u{201D}").into());
        cx.notify();
    }

    /// Opens Settings on the Export page.
    fn open_export_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(page) = SETTINGS_PAGES.iter().position(|p| p.title == "Export") {
            self.settings_page = page;
        }
        self.open_settings(window, cx);
    }

    /// The Presets section, when presets are switched on.
    pub(crate) fn export_presets_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.settings.export.presets_enabled {
            return None;
        }
        let theme = self.theme;
        let options = &self.export_options;
        let mut chips: Vec<AnyElement> = Vec::new();
        for (i, (preset, builtin)) in self.all_export_presets().into_iter().enumerate() {
            let selected = preset.matches(options);
            let name = preset.name.clone();
            let chip = div()
                .id(SharedString::from(format!("export-preset-{i}")))
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .h(px(24.0))
                .px_2()
                .rounded_md()
                .border_1()
                .text_xs()
                .cursor_pointer()
                .when(selected, |el| {
                    el.bg(theme.accent_muted)
                        .border_color(theme.accent)
                        .text_color(theme.text)
                })
                .when(!selected, |el| {
                    el.border_color(theme.border_subtle)
                        .text_color(theme.text_muted)
                        .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                })
                .child(preset.name.clone())
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.apply_export_preset(&preset, cx)
                }))
                .when(!builtin, |el| {
                    el.child(
                        div()
                            .id(SharedString::from(format!("export-preset-del-{i}")))
                            .px_0p5()
                            .text_color(theme.text_faint)
                            .hover(|el| el.text_color(theme.danger))
                            .tooltip(rustyedit_ui::tip("Delete this preset", &theme))
                            .child("\u{2715}")
                            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                cx.stop_propagation();
                                this.delete_export_preset(name.clone(), cx)
                            })),
                    )
                })
                .into_any_element();
            chips.push(chip);
        }

        let save: AnyElement = match self.draft_for(TextTarget::ExportPresetName) {
            Some(draft) => div()
                .flex()
                .flex_row()
                .items_center()
                .h(px(24.0))
                .px_2()
                .rounded_md()
                .border_1()
                .border_color(theme.accent)
                .bg(theme.background)
                .text_xs()
                .text_color(theme.text)
                .child(if draft.is_empty() {
                    "Preset name, then \u{23CE}".to_string()
                } else {
                    draft.to_string()
                })
                .child(div().w(px(1.0)).h(px(12.0)).ml(px(1.0)).bg(theme.accent))
                .into_any_element(),
            None => div()
                .id("export-preset-save")
                .flex()
                .items_center()
                .h(px(24.0))
                .px_2()
                .rounded_md()
                .border_1()
                .border_color(theme.border_subtle)
                .text_xs()
                .text_color(theme.accent)
                .cursor_pointer()
                .hover(|el| el.bg(theme.surface_elevated))
                .tooltip(rustyedit_ui::tip("Save the current options as a preset", &theme))
                .child("+ Save\u{2026}")
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.begin_text_draft(TextTarget::ExportPresetName, window, cx)
                }))
                .into_any_element(),
        };
        chips.push(save);

        Some(
            rustyedit_ui::section(
                &theme,
                "Preset",
                vec![div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_1()
                    .children(chips)
                    .into_any_element()],
            )
            .into_any_element(),
        )
    }

    /// The Watermark section, when watermarks are switched on.
    pub(crate) fn export_watermark_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let export = &self.settings.export;
        if !export.watermark_enabled {
            return None;
        }
        let theme = self.theme;
        let on = self.export_options.watermark;
        let text = export.watermark.text.trim();
        let summary = if text.is_empty() {
            "No watermark text yet".to_string()
        } else {
            format!("\u{201C}{text}\u{201D}")
        };
        let switch = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(div().text_sm().text_color(theme.text_muted).child("Add watermark"))
            .child(rustyedit_ui::toggle_switch(
                "export-watermark",
                on,
                &theme,
                cx.listener(|this, _: &ClickEvent, _w, cx| {
                    this.export_options.watermark = !this.export_options.watermark;
                    this.export_message = None;
                    cx.notify();
                }),
            ))
            .into_any_element();
        let detail = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .text_xs()
                    .text_color(theme.text_faint)
                    .child(summary),
            )
            .child(rustyedit_ui::chrome_button(
                "export-watermark-edit",
                "Edit\u{2026}",
                false,
                &theme,
                cx.listener(|this, _: &ClickEvent, window, cx| this.open_export_settings(window, cx)),
            ))
            .into_any_element();
        Some(rustyedit_ui::section(&theme, "Watermark", vec![switch, detail]).into_any_element())
    }
}
