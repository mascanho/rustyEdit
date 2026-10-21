//! Presets in the tools column: the built-in looks, the user's saved ones,
//! and saving the current photo's look as a new preset.

use gpui::{
    div, hsla, linear_color_stop, linear_gradient, prelude::*, px, AnyElement, ClickEvent, Context,
};
use rustyedit_core::{builtin_presets, DevelopPreset, DevelopSettings};
use rustyedit_ui::Theme;

use super::{
    tools::{name_field, ClickHandler},
    HistoryNote,
};
use crate::workspace::{TextTarget, Workspace};

impl Workspace {
    pub(super) fn presets_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(current) = self.catalog.selected_asset().map(|a| a.develop) else {
            return Vec::new();
        };
        // A preset is "on" when applying it would change nothing.
        let is_active = |preset: &DevelopPreset| current.with_look_of(&preset.settings) == current;

        let save = match self.draft_for(TextTarget::PresetName) {
            Some(draft) => name_field(&theme, draft, "Preset name, then \u{23CE}"),
            None => save_button(
                &theme,
                cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.begin_text_draft(TextTarget::PresetName, window, cx);
                }),
            ),
        };

        let builtin: Vec<AnyElement> = builtin_presets()
            .into_iter()
            .enumerate()
            .map(|(ix, preset)| {
                let (active, name, look) =
                    (is_active(&preset), preset.name.clone(), preset.settings);
                preset_row(
                    ("develop-preset-builtin", ix),
                    &theme,
                    &name,
                    &look,
                    active,
                    cx.listener(move |this, _: &ClickEvent, _w, cx| this.apply_preset(&preset, cx)),
                    None,
                )
            })
            .collect();

        let user = &self.develop_ui.presets.user;
        let yours: Vec<AnyElement> = user
            .iter()
            .enumerate()
            .map(|(ix, preset)| {
                let (active, name, look) =
                    (is_active(preset), preset.name.clone(), preset.settings);
                let preset = preset.clone();
                preset_row(
                    ("develop-preset-user", ix),
                    &theme,
                    &name,
                    &look,
                    active,
                    cx.listener(move |this, _: &ClickEvent, _w, cx| this.apply_preset(&preset, cx)),
                    Some(Box::new(cx.listener(
                        move |this, _: &ClickEvent, _w, cx| {
                            this.delete_preset(ix, cx);
                        },
                    ))),
                )
            })
            .collect();

        vec![div()
            .flex()
            .flex_col()
            .gap_3()
            .pt_1()
            .pb_2()
            .child(save)
            .child(preset_group(&theme, "Built-in", builtin, None))
            .child(preset_group(
                &theme,
                "Yours",
                yours,
                Some("Save a look to reuse it on other photos."),
            ))
            .into_any_element()]
    }

    /// Gives every target photo the preset's look, keeping its geometry.
    fn apply_preset(&mut self, preset: &DevelopPreset, cx: &mut Context<Self>) {
        let look = preset.settings;
        self.replace_develop(
            self.target_indices(),
            &format!("preset \u{201C}{}\u{201D}", preset.name),
            HistoryNote::Record {
                title: "Preset",
                detail: &preset.name,
            },
            cx,
            move |develop: &DevelopSettings| develop.with_look_of(&look),
        );
    }

    /// Saves the selected photo's look under `name` (numbered if blank).
    pub(crate) fn save_preset(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(develop) = self.catalog.selected_asset().map(|a| a.develop) else {
            return;
        };
        let name = self.develop_ui.presets.save(&name, &develop);
        self.persist_presets(format!("Saved preset \u{201C}{name}\u{201D}"));
        cx.notify();
    }

    fn delete_preset(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(name) = self
            .develop_ui
            .presets
            .user
            .get(index)
            .map(|p| p.name.clone())
        else {
            return;
        };
        self.develop_ui.presets.remove(index);
        self.persist_presets(format!("Deleted preset \u{201C}{name}\u{201D}"));
        cx.notify();
    }

    fn persist_presets(&mut self, done: String) {
        self.status_message = Some(match rustyedit_io::save_presets(&self.develop_ui.presets) {
            Ok(()) => done.into(),
            Err(err) => format!("Couldn't save presets: {err}").into(),
        });
    }
}

/// "Save current look": a full-width outlined button above the groups.
fn save_button(
    theme: &Theme,
    on_click: impl Fn(&ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let theme = *theme;
    div()
        .id("develop-preset-save")
        .flex()
        .flex_row()
        .items_center()
        .justify_center()
        .gap_1p5()
        .h(px(28.0))
        .rounded_md()
        .border_1()
        .border_color(theme.accent.opacity(0.5))
        .bg(theme.accent.opacity(0.06))
        .text_size(px(11.5))
        .text_color(theme.accent)
        .cursor_pointer()
        .hover(|el| el.bg(theme.accent.opacity(0.14)).border_color(theme.accent))
        .child(div().text_size(px(13.0)).child("+"))
        .child("Save current look")
        .on_click(on_click)
        .into_any_element()
}

/// A titled card of preset rows separated by hairlines. Shows `empty` in
/// place of the card when there are no rows (or nothing when `None`).
fn preset_group(
    theme: &Theme,
    title: &'static str,
    rows: Vec<AnyElement>,
    empty: Option<&'static str>,
) -> AnyElement {
    let count = rows.len();
    let heading = div()
        .flex()
        .flex_row()
        .items_baseline()
        .justify_between()
        .px_0p5()
        .pb_1()
        .child(
            div()
                .text_size(px(9.5))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text_muted)
                .child(title.to_uppercase()),
        )
        .when(count > 0, |el| {
            el.child(
                div()
                    .text_size(px(9.5))
                    .text_color(theme.text_faint)
                    .child(count.to_string()),
            )
        });

    let body = if rows.is_empty() {
        div()
            .px_2()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(theme.border_subtle)
            .text_size(px(10.5))
            .text_color(theme.text_faint)
            .children(empty)
    } else {
        div()
            .flex()
            .flex_col()
            .rounded_md()
            .overflow_hidden()
            .bg(theme.background.opacity(0.6))
            .border_1()
            .border_color(theme.border_subtle)
            .children(rows.into_iter().enumerate().map(|(ix, row)| {
                div()
                    .when(ix > 0, |el| {
                        el.border_t_1().border_color(theme.border_subtle)
                    })
                    .child(row)
            }))
    };

    div()
        .flex()
        .flex_col()
        .child(heading)
        .when(count > 0 || empty.is_some(), |el| el.child(body))
        .into_any_element()
}

/// One preset: a chip hinting at its look, the name, and a check when it's
/// the look the photo has. User presets get a delete button on hover.
fn preset_row(
    id: impl Into<gpui::ElementId>,
    theme: &Theme,
    name: &str,
    look: &DevelopSettings,
    active: bool,
    on_click: impl Fn(&ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    on_delete: Option<ClickHandler>,
) -> AnyElement {
    let theme = *theme;
    div()
        .id(id)
        .group("develop-preset-row")
        .relative()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .h(px(30.0))
        .pl_2()
        .pr_1p5()
        .cursor_pointer()
        .text_size(px(11.5))
        .when(active, |el| {
            el.bg(theme.accent_muted.opacity(0.6))
                .text_color(theme.text)
        })
        .when(!active, |el| {
            el.text_color(theme.text_muted)
                .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
        })
        // Accent bar marking the applied preset.
        .when(active, |el| {
            el.child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(2.0))
                    .bg(theme.accent),
            )
        })
        .child(look_chip(look))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .whitespace_nowrap()
                .child(name.to_string()),
        )
        .when(active, |el| {
            el.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(10.0))
                    .text_color(theme.accent)
                    .child("\u{2713}"),
            )
        })
        .children(on_delete.map(|on_delete| {
            div()
                .id("delete")
                .flex_shrink_0()
                .px_1()
                .text_size(px(10.0))
                .text_color(theme.text_faint)
                .invisible()
                .group_hover("develop-preset-row", |s| s.visible())
                .hover(|el| el.text_color(theme.danger))
                .child("\u{2715}")
                .on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    on_delete(event, window, cx);
                })
        }))
        .on_click(on_click)
        .into_any_element()
}

/// A small gradient swatch suggesting what a look does: warm or cool,
/// colourful or grey, bright or dark, flat or contrasty.
fn look_chip(look: &DevelopSettings) -> AnyElement {
    let hue = match look.temperature {
        t if t > 0.0 => 30.0,
        t if t < 0.0 => 215.0,
        _ => 195.0,
    } / 360.0;
    let saturation = if look.saturation <= -100.0 {
        0.0
    } else {
        (0.3 + look.vibrance / 100.0 * 0.25
            + look.saturation / 100.0 * 0.3
            + look.temperature.abs() / 100.0 * 0.4)
            .clamp(0.0, 0.85)
    };
    let mid = (0.5 + look.exposure / 100.0 * 0.6 + look.shadows / 100.0 * 0.15).clamp(0.2, 0.85);
    let spread = (0.18 + look.contrast / 100.0 * 0.12).clamp(0.04, 0.35);
    let top = (mid + spread).min(0.95);
    let bottom = (mid - spread + look.blacks / 100.0 * 0.2).max(0.05);

    div()
        .flex_shrink_0()
        .w(px(22.0))
        .h(px(16.0))
        .rounded(px(3.0))
        .border_1()
        .border_color(gpui::black().opacity(0.25))
        .bg(linear_gradient(
            180.0,
            linear_color_stop(hsla(hue, saturation, top, 1.0), 0.0),
            linear_color_stop(hsla(hue, saturation * 0.8, bottom, 1.0), 1.0),
        ))
        .into_any_element()
}
