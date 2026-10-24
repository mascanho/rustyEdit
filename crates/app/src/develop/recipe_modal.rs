//! The Fujifilm recipe tool: a modal for entering a recipe (film
//! simulation, dynamic range, grain, Color Chrome, white balance shifts,
//! tone, colour, sharpness, clarity, ISO…) and turning it into a Develop
//! preset, with a live summary of what it becomes.

use rustyedit_ui::sp;
use gpui::{
    div, prelude::*, px, AnyElement, App, ClickEvent, ClipboardItem, Context, Entity, MouseButton,
    SharedString, Window,
};
use rustyedit_core::{
    DevelopSection, DynamicRange, FilmSimulation, FujiRecipe, GrainEffect, MonoFilter, Strength,
    WhiteBalance,
};
use rustyedit_ui::Theme;

use super::tools::name_field;
use crate::workspace::{TextTarget, Workspace};

const MODAL_WIDTH: f32 = 860.0;
const MODAL_HEIGHT: f32 = 640.0;

impl Workspace {
    pub(crate) fn open_recipe_modal(&mut self, cx: &mut Context<Self>) {
        if self.develop_ui.preset_ui.recipe.is_none() {
            self.develop_ui.preset_ui.recipe = Some(FujiRecipe::example());
        }
        cx.notify();
    }

    pub(crate) fn close_recipe_modal(&mut self, cx: &mut Context<Self>) {
        self.develop_ui.preset_ui.recipe = None;
        self.text_draft = None;
        cx.notify();
    }

    fn edit_recipe(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut FujiRecipe)) {
        if let Some(recipe) = &mut self.develop_ui.preset_ui.recipe {
            change(recipe);
        }
        cx.notify();
    }

    /// Starts typing into one of the recipe's text fields, pre-filled.
    fn begin_recipe_text(
        &mut self,
        target: TextTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(recipe) = &self.develop_ui.preset_ui.recipe else {
            return;
        };
        let current = match target {
            TextTarget::RecipeName => recipe.name.clone(),
            TextTarget::RecipeIso => recipe.iso.clone(),
            _ => recipe.exposure_compensation.clone(),
        };
        self.text_draft = Some((target, current));
        window.focus(&self.focus_handle);
        cx.notify();
    }

    pub(crate) fn commit_recipe_text(
        &mut self,
        target: TextTarget,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.edit_recipe(cx, |r| match target {
            TextTarget::RecipeName => r.name = text.trim().to_string(),
            TextTarget::RecipeIso => r.iso = text.trim().to_string(),
            _ => r.exposure_compensation = text.trim().to_string(),
        });
    }

    /// Adds the recipe to the user's presets (replacing one of the same
    /// name), optionally applying it to the selected photo(s) too.
    fn finish_recipe(&mut self, apply: bool, cx: &mut Context<Self>) {
        let Some(recipe) = self.develop_ui.preset_ui.recipe.clone() else {
            return;
        };
        let preset = recipe.to_preset();
        self.develop_ui.presets.upsert(preset.clone());
        self.persist_presets(format!("Added preset \u{201C}{}\u{201D}", preset.name));
        if apply {
            self.apply_preset_blend(&preset, 1.0, &DevelopSection::LOOK, cx);
        }
        self.close_recipe_modal(cx);
    }

    pub(crate) fn render_recipe_modal(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let Some(recipe) = self.develop_ui.preset_ui.recipe.clone() else {
            return div().into_any_element();
        };
        let entity = cx.entity();

        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .flex_shrink_0()
            .h(sp(44.0))
            .px_4()
            .border_b_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Fujifilm recipe"),
                    )
                    .child(
                        div()
                            .text_size(sp(10.5))
                            .text_color(theme.text_faint)
                            .child("Enter a recipe to turn it into a preset"),
                    ),
            )
            .child(rustyedit_ui::glyph_button(
                "recipe-close",
                "\u{2715}",
                "Close",
                theme.text_muted,
                true,
                &theme,
                cx.listener(|this, _: &ClickEvent, _w, cx| this.close_recipe_modal(cx)),
            ));

        // --- Form -----------------------------------------------------------
        let name_draft = self.draft_for(TextTarget::RecipeName);
        let name_field_el = match name_draft {
            Some(draft) => name_field(&theme, draft, "Recipe name, then \u{23CE}"),
            None => text_value(
                &theme,
                "recipe-name",
                &recipe.name,
                "Name this recipe",
                cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.begin_recipe_text(TextTarget::RecipeName, window, cx)
                }),
            ),
        };

        let mut form: Vec<AnyElement> = vec![field(&theme, "Name", name_field_el)];
        form.push(field(
            &theme,
            "Film simulation",
            chips(
                &theme,
                &entity,
                "recipe-sim",
                FilmSimulation::ALL,
                recipe.film_simulation,
                |v| v.label().to_string(),
                |r, v| r.film_simulation = v,
            ),
        ));
        if recipe.film_simulation.is_monochrome() {
            form.push(field(
                &theme,
                "Colour filter",
                chips(
                    &theme,
                    &entity,
                    "recipe-filter",
                    MonoFilter::ALL,
                    recipe.mono_filter,
                    |v| v.label().to_string(),
                    |r, v| r.mono_filter = v,
                ),
            ));
        }
        form.push(field(
            &theme,
            "Dynamic range",
            chips(
                &theme,
                &entity,
                "recipe-dr",
                DynamicRange::ALL,
                recipe.dynamic_range,
                |v| v.label().to_string(),
                |r, v| r.dynamic_range = v,
            ),
        ));
        form.push(field(
            &theme,
            "Grain effect",
            chips(
                &theme,
                &entity,
                "recipe-grain",
                GrainEffect::ALL,
                recipe.grain,
                |v| v.label().to_string(),
                |r, v| r.grain = v,
            ),
        ));
        form.push(field(
            &theme,
            "Color Chrome effect",
            chips(
                &theme,
                &entity,
                "recipe-cc",
                Strength::ALL,
                recipe.color_chrome,
                |v| v.label().to_string(),
                |r, v| r.color_chrome = v,
            ),
        ));
        form.push(field(
            &theme,
            "Color Chrome FX Blue",
            chips(
                &theme,
                &entity,
                "recipe-ccb",
                Strength::ALL,
                recipe.color_chrome_blue,
                |v| v.label().to_string(),
                |r, v| r.color_chrome_blue = v,
            ),
        ));
        form.push(field(
            &theme,
            "White balance",
            chips(
                &theme,
                &entity,
                "recipe-wb",
                WhiteBalance::ALL,
                recipe.white_balance,
                |v| v.label().to_string(),
                |r, v| r.white_balance = v,
            ),
        ));
        if recipe.white_balance == WhiteBalance::Kelvin {
            form.push(stepper_row(
                &theme,
                &entity,
                "recipe-kelvin",
                "Colour temperature",
                format!("{} K", recipe.kelvin),
                |r| r.kelvin = r.kelvin.saturating_sub(100).max(2500),
                |r| r.kelvin = (r.kelvin + 100).min(10000),
            ));
        }
        form.push(stepper_row(
            &theme,
            &entity,
            "recipe-red",
            "WB shift: Red",
            signed(recipe.wb_red),
            |r| r.wb_red = (r.wb_red - 1).max(-9),
            |r| r.wb_red = (r.wb_red + 1).min(9),
        ));
        form.push(stepper_row(
            &theme,
            &entity,
            "recipe-blue",
            "WB shift: Blue",
            signed(recipe.wb_blue),
            |r| r.wb_blue = (r.wb_blue - 1).max(-9),
            |r| r.wb_blue = (r.wb_blue + 1).min(9),
        ));
        form.push(stepper_row(
            &theme,
            &entity,
            "recipe-highlight",
            "Highlight tone",
            tone(recipe.highlight),
            |r| r.highlight = (r.highlight - 0.5).max(-2.0),
            |r| r.highlight = (r.highlight + 0.5).min(4.0),
        ));
        form.push(stepper_row(
            &theme,
            &entity,
            "recipe-shadow",
            "Shadow tone",
            tone(recipe.shadow),
            |r| r.shadow = (r.shadow - 0.5).max(-2.0),
            |r| r.shadow = (r.shadow + 0.5).min(4.0),
        ));
        form.push(stepper_row(
            &theme,
            &entity,
            "recipe-color",
            "Color",
            signed(recipe.color),
            |r| r.color = (r.color - 1).max(-4),
            |r| r.color = (r.color + 1).min(4),
        ));
        form.push(stepper_row(
            &theme,
            &entity,
            "recipe-sharp",
            "Sharpness",
            signed(recipe.sharpness),
            |r| r.sharpness = (r.sharpness - 1).max(-4),
            |r| r.sharpness = (r.sharpness + 1).min(4),
        ));
        form.push(stepper_row(
            &theme,
            &entity,
            "recipe-nr",
            "High ISO NR",
            signed(recipe.high_iso_nr),
            |r| r.high_iso_nr = (r.high_iso_nr - 1).max(-4),
            |r| r.high_iso_nr = (r.high_iso_nr + 1).min(4),
        ));
        form.push(stepper_row(
            &theme,
            &entity,
            "recipe-clarity",
            "Clarity",
            signed(recipe.clarity),
            |r| r.clarity = (r.clarity - 1).max(-5),
            |r| r.clarity = (r.clarity + 1).min(5),
        ));

        for (target, id, label, value, placeholder) in [
            (
                TextTarget::RecipeIso,
                "recipe-iso",
                "ISO",
                recipe.iso.clone(),
                "e.g. Auto, up to ISO 6400",
            ),
            (
                TextTarget::RecipeExposure,
                "recipe-exposure",
                "Exposure compensation",
                recipe.exposure_compensation.clone(),
                "e.g. 0 to +2/3 (typically)",
            ),
        ] {
            let el = match self.draft_for(target) {
                Some(draft) => name_field(&theme, draft, "Type, then \u{23CE}"),
                None => text_value(
                    &theme,
                    id,
                    &value,
                    placeholder,
                    cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.begin_recipe_text(target, window, cx)
                    }),
                ),
            };
            form.push(field(&theme, label, el));
        }

        let form = div()
            .id("recipe-form")
            .flex()
            .flex_col()
            .flex_1()
            .min_w(sp(0.0))
            .gap_3()
            .overflow_y_scroll()
            .track_scroll(&self.develop_ui.preset_ui.recipe_scroll)
            .p_4()
            .children(form);

        // --- Summary --------------------------------------------------------
        let settings = recipe.to_settings();
        let mut mapped: Vec<(&str, f32)> = vec![
            ("Temperature", settings.temperature),
            ("Tint", -settings.tint),
            ("Contrast", settings.contrast),
            ("Highlights", settings.highlights),
            ("Shadows", settings.shadows),
            ("Blacks", settings.blacks),
            ("Saturation", settings.saturation),
            ("Vibrance", settings.vibrance),
            ("Clarity", settings.clarity),
            ("Sharpening", settings.sharpening),
            ("Grain", settings.grain),
        ];
        mapped.retain(|(_, v)| v.abs() >= 0.5);

        let summary = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(sp(280.0))
            .gap_3()
            .p_4()
            .bg(theme.background)
            .border_l_1()
            .border_color(theme.border_subtle)
            .child(caption(&theme, "RECIPE"))
            .child(
                div()
                    .text_size(sp(11.0))
                    .text_color(theme.text_muted)
                    .children(
                        recipe
                            .summary()
                            .lines()
                            .map(|line| div().child(line.to_string())),
                    ),
            )
            .child(caption(&theme, "BECOMES (APPROXIMATELY)"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .children(mapped.into_iter().map(|(label, value)| {
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .text_size(sp(11.0))
                            .child(div().text_color(theme.text_muted).child(label))
                            .child(div().text_color(theme.text).child(format!("{value:+.0}")))
                    })),
            )
            .child(
                div()
                    .text_size(sp(10.0))
                    .text_color(theme.text_faint)
                    .child(
                        "The film simulation is built from curves, colour mixer and \
                         grading, so it's close but not identical to the camera. High ISO \
                         NR, ISO and exposure compensation are kept as notes.",
                    ),
            );

        // --- Footer ---------------------------------------------------------
        let copy_text = recipe.summary();
        let footer = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .flex_shrink_0()
            .h(sp(48.0))
            .px_4()
            .border_t_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .child(rustyedit_ui::chrome_button(
                        "recipe-example",
                        "Load example",
                        false,
                        &theme,
                        cx.listener(|this, _: &ClickEvent, _w, cx| {
                            this.edit_recipe(cx, |r| *r = FujiRecipe::example())
                        }),
                    ))
                    .child(rustyedit_ui::chrome_button(
                        "recipe-reset",
                        "Reset",
                        false,
                        &theme,
                        cx.listener(|this, _: &ClickEvent, _w, cx| {
                            this.edit_recipe(cx, |r| *r = FujiRecipe::default())
                        }),
                    ))
                    .child(rustyedit_ui::chrome_button(
                        "recipe-copy",
                        "Copy recipe",
                        false,
                        &theme,
                        move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone()))
                        },
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(rustyedit_ui::chrome_button(
                        "recipe-cancel",
                        "Cancel",
                        false,
                        &theme,
                        cx.listener(|this, _: &ClickEvent, _w, cx| this.close_recipe_modal(cx)),
                    ))
                    .child(rustyedit_ui::chrome_button(
                        "recipe-add",
                        "Add to presets",
                        false,
                        &theme,
                        cx.listener(|this, _: &ClickEvent, _w, cx| this.finish_recipe(false, cx)),
                    ))
                    .child(rustyedit_ui::chrome_button(
                        "recipe-add-apply",
                        "Add & apply to photo",
                        true,
                        &theme,
                        cx.listener(|this, _: &ClickEvent, _w, cx| this.finish_recipe(true, cx)),
                    )),
            );

        let card = div()
            .id("recipe-card")
            .flex()
            .flex_col()
            .w(px(MODAL_WIDTH))
            .h(px(MODAL_HEIGHT))
            .rounded_lg()
            .overflow_hidden()
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            // Clicks inside the card must not reach the backdrop.
            .on_mouse_down(MouseButton::Left, |_, _w, cx| cx.stop_propagation())
            .child(header)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h(sp(0.0))
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_1()
                            .min_w(sp(0.0))
                            .child(form)
                            .child(rustyedit_ui::vertical_scrollbar(
                                "recipe-form-scrollbar",
                                self.develop_ui.preset_ui.recipe_scroll.clone(),
                                &theme,
                            )),
                    )
                    .child(summary),
            )
            .child(footer);

        div()
            .id("recipe-backdrop")
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.55))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _w, cx| this.close_recipe_modal(cx)),
            )
            .child(card)
            .into_any_element()
    }
}

fn signed(v: i32) -> String {
    format!("{v:+}")
}

fn tone(v: f32) -> String {
    if v == v.trunc() {
        format!("{:+}", v as i32)
    } else {
        format!("{v:+.1}")
    }
}

fn caption(theme: &Theme, text: &'static str) -> AnyElement {
    div()
        .text_size(sp(10.0))
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(theme.text_faint)
        .child(text)
        .into_any_element()
}

/// A labelled form field.
fn field(theme: &Theme, label: &'static str, control: AnyElement) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_size(sp(11.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text_muted)
                .child(label),
        )
        .child(control)
        .into_any_element()
}

/// A text value that turns into an input when clicked.
fn text_value(
    theme: &Theme,
    id: &'static str,
    value: &str,
    placeholder: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .flex()
        .items_center()
        .h(sp(26.0))
        .px_2()
        .bg(theme.background)
        .border_1()
        .border_color(theme.border)
        .text_size(sp(11.5))
        .text_color(if value.is_empty() {
            theme.text_faint
        } else {
            theme.text
        })
        .cursor_text()
        .hover(|el| el.border_color(theme.text_faint))
        .child(if value.is_empty() {
            placeholder.to_string()
        } else {
            value.to_string()
        })
        .on_click(on_click)
        .into_any_element()
}

/// A wrapping row of option chips.
fn chips<T: Copy + PartialEq + 'static>(
    theme: &Theme,
    entity: &Entity<Workspace>,
    id: &'static str,
    values: &[T],
    current: T,
    label: impl Fn(T) -> String,
    apply: fn(&mut FujiRecipe, T),
) -> AnyElement {
    let theme = *theme;
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap_1()
        .children(values.iter().enumerate().map(|(i, &value)| {
            let entity = entity.clone();
            let selected = value == current;
            div()
                .id(SharedString::from(format!("{id}-{i}")))
                .flex()
                .items_center()
                .h(sp(24.0))
                .px_2p5()
                .cursor_pointer()
                .border_1()
                .text_xs()
                .when(selected, |el| {
                    el.bg(theme.accent_muted)
                        .border_color(theme.accent)
                        .text_color(theme.text)
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                })
                .when(!selected, |el| {
                    el.bg(theme.background)
                        .border_color(theme.border_subtle)
                        .text_color(theme.text_muted)
                        .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                })
                .child(label(value))
                .on_click(move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                    entity.update(cx, |this, cx| this.edit_recipe(cx, |r| apply(r, value)));
                })
                .into_any_element()
        }))
        .into_any_element()
}

/// A label with a value and − / + buttons.
fn stepper_row(
    theme: &Theme,
    entity: &Entity<Workspace>,
    id: &'static str,
    label: &'static str,
    value: String,
    dec: fn(&mut FujiRecipe),
    inc: fn(&mut FujiRecipe),
) -> AnyElement {
    let step = |suffix: &'static str, glyph: &'static str, change: fn(&mut FujiRecipe)| {
        let entity = entity.clone();
        rustyedit_ui::chrome_button(
            SharedString::from(format!("{id}-{suffix}")),
            glyph,
            false,
            theme,
            move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                entity.update(cx, |this, cx| this.edit_recipe(cx, change));
            },
        )
    };
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(sp(11.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text_muted)
                .child(label),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .child(step("dec", "\u{2212}", dec))
                .child(
                    div()
                        .w(sp(64.0))
                        .text_center()
                        .text_size(sp(12.0))
                        .text_color(theme.text)
                        .child(value),
                )
                .child(step("inc", "+", inc)),
        )
        .into_any_element()
}
