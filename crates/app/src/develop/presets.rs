//! The Presets rail tool: built-in and saved looks, searchable and
//! applied at any strength to any
//! mix of sections, with saving, updating, renaming, duplicating, reordering
//! and importing / exporting of the user's own.

use rustyedit_ui::sp;
use gpui::{
    deferred, div, hsla, linear_color_stop, linear_gradient, prelude::*, AnyElement, App,
    ClickEvent, Context, MouseDownEvent, SharedString, Window,
};
use rustyedit_core::{
    builtin_presets, DevelopPreset, DevelopSection, DevelopSettings, PRESET_CATEGORIES,
};
use rustyedit_ui::{SliderSpec, SliderTrack, Theme};

use super::{tools::name_field, HistoryNote};
use crate::workspace::{TextTarget, Workspace};

/// View state of the Presets panel.
pub(crate) struct PresetUi {
    /// 0-100: how strongly a clicked preset is applied.
    pub(crate) amount: f32,
    /// Bit per [`DevelopSection::LOOK`] entry: which parts of a look are
    /// applied, and saved into new presets.
    pub(crate) scope: u16,
    /// Category a newly saved preset is filed under.
    pub(crate) new_category: usize,
    /// The saved preset being renamed.
    pub(crate) renaming: Option<usize>,
    /// List alphabetically instead of in saved order.
    pub(crate) sort_az: bool,
    /// The search & options pop-up is open.
    pub(crate) popup_open: bool,
    /// The Fujifilm recipe being entered, while its modal is open.
    pub(crate) recipe: Option<rustyedit_core::FujiRecipe>,
    pub(crate) recipe_scroll: gpui::ScrollHandle,
}

impl Default for PresetUi {
    fn default() -> Self {
        Self {
            amount: 100.0,
            scope: (1 << DevelopSection::LOOK.len()) - 1,
            new_category: PRESET_CATEGORIES.len() - 1,
            renaming: None,
            sort_az: false,
            popup_open: false,
            recipe: None,
            recipe_scroll: gpui::ScrollHandle::new(),
        }
    }
}

impl PresetUi {
    fn scope_sections(&self) -> Vec<DevelopSection> {
        DevelopSection::LOOK
            .iter()
            .enumerate()
            .filter(|(i, _)| self.scope & (1 << i) != 0)
            .map(|(_, s)| *s)
            .collect()
    }
}

/// A preset on show: where it lives, so actions know what to change.
#[derive(Clone)]
enum Entry {
    BuiltIn(DevelopPreset),
    Saved(usize, DevelopPreset),
}

impl Entry {
    fn preset(&self) -> &DevelopPreset {
        match self {
            Entry::BuiltIn(p) | Entry::Saved(_, p) => p,
        }
    }
}

const SCOPE_LABELS: [&str; 7] = [
    "Tone", "Curve", "Colour", "Mixer", "Grading", "Detail", "Effects",
];

impl Workspace {
    /// The Presets tab's pinned controls, which stay put while the list
    /// scrolls: save the current look (the main action), then icon buttons
    /// for Fujifilm recipe, import, export, reset and options, and the
    /// strength slider.
    pub(crate) fn presets_pinned(&self, cx: &mut Context<Self>) -> AnyElement {
        use rustyedit_ui::Icon;
        let theme = self.theme;
        let ui = &self.develop_ui.preset_ui;
        let has_photo = self.catalog.selected_asset().is_some();
        let popup_open = ui.popup_open;

        // The main action: save the photo's look as a preset.
        let save = match self.draft_for(TextTarget::PresetName) {
            Some(draft) => div()
                .flex_1()
                .min_w(sp(0.0))
                .child(name_field(&theme, draft, "Preset name, then \u{23CE}"))
                .into_any_element(),
            None => div()
                .id("preset-save")
                .flex_1()
                .min_w(sp(0.0))
                .flex()
                .flex_row()
                .items_center()
                .justify_center()
                .gap_1p5()
                .h(sp(28.0))
                .border_1()
                .border_color(theme.accent)
                .bg(theme.accent_muted)
                .text_size(sp(11.5))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text)
                .when(has_photo, |el| {
                    el.cursor_pointer()
                        .hover(|el| el.bg(theme.accent.opacity(0.35)))
                })
                .when(!has_photo, |el| el.opacity(0.45))
                .tooltip(rustyedit_ui::tip(
                    "Save the selected photo's look as a preset",
                    &theme,
                ))
                .child(rustyedit_ui::icon(Icon::SaveLook, 15.0, theme.text))
                .child("Save look")
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    if this.catalog.selected_asset().is_some() {
                        this.begin_text_draft(TextTarget::PresetName, window, cx);
                    }
                }))
                .into_any_element(),
        };

        // A square icon button with a tooltip.
        let tool = |id: &'static str,
                    icon: Icon,
                    tip: &'static str,
                    active: bool,
                    enabled: bool,
                    on_click: Box<dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)>,
                    cx: &mut Context<Self>|
         -> gpui::Stateful<gpui::Div> {
            let entity = cx.entity();
            // Buttons share the row's width, so the toolbar always fits
            // the panel exactly, however wide it is.
            div()
                .id(id)
                .flex()
                .flex_1()
                .min_w(sp(22.0))
                .items_center()
                .justify_center()
                .h(sp(28.0))
                .border_1()
                .when(active, |el| {
                    el.bg(theme.accent_muted).border_color(theme.accent)
                })
                .when(!active, |el| {
                    el.bg(theme.background).border_color(theme.border_subtle)
                })
                .when(enabled, |el| {
                    el.cursor_pointer()
                        .hover(|el| el.bg(theme.surface_elevated).border_color(theme.border))
                })
                .when(!enabled, |el| el.opacity(0.4))
                .tooltip(rustyedit_ui::tip(tip, &theme))
                .child(rustyedit_ui::icon(
                    icon,
                    15.0,
                    if active { theme.text } else { theme.text_muted },
                ))
                .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                    if enabled {
                        entity.update(cx, |this, cx| on_click(this, window, cx));
                    }
                })
        };

        let recipe = tool(
            "preset-fuji-recipe",
            Icon::Film,
            "Fujifilm recipe\u{2026}",
            false,
            true,
            Box::new(|this, _, cx| this.open_recipe_modal(cx)),
            cx,
        );
        let from_photo = tool(
            "preset-from-photo",
            Icon::PresetFromPhoto,
            "New preset from a photo: pick any image and save its look",
            false,
            !self.develop_ui.auto_busy,
            Box::new(|this, _, cx| this.preset_from_photo(cx)),
            cx,
        );
        let match_look = tool(
            "preset-match-look",
            Icon::MatchLook,
            "Match look: give the selected photos the look of another image",
            false,
            has_photo && !self.develop_ui.auto_busy,
            Box::new(|this, _, cx| this.match_look(cx)),
            cx,
        );
        let import = tool(
            "preset-import",
            Icon::Download,
            "Import presets from a file",
            false,
            true,
            Box::new(|this, _, cx| this.import_presets(cx)),
            cx,
        );
        let export = tool(
            "preset-export",
            Icon::Upload,
            "Export your presets to a file",
            false,
            true,
            Box::new(|this, _, cx| this.export_presets(cx)),
            cx,
        );
        let reset = tool(
            "preset-neutral",
            Icon::Reset,
            "Reset look: clear the tone, colour and detail edits of the selected photos",
            false,
            has_photo,
            Box::new(|this, _, cx| {
                this.apply_neutral_look(&DevelopSettings::default(), "Reset look", cx)
            }),
            cx,
        );
        let options = tool(
            "preset-popup-button",
            Icon::Adjust,
            "Options: what a preset applies and saves, and the list order",
            popup_open,
            true,
            Box::new(|this, _, cx| {
                let ui = &mut this.develop_ui.preset_ui;
                ui.popup_open = !ui.popup_open;
                cx.notify();
            }),
            cx,
        );

        // Strength: how strongly the photo's preset is applied (changing it
        // re-applies the preset), or a clicked preset will be.
        let entity = cx.entity();
        let shown = self
            .catalog
            .selected_asset()
            .and_then(|a| a.current_preset_amount())
            .map_or(ui.amount, |amount| (amount * 100.0).round());
        let strength = rustyedit_ui::labeled_slider(
            "preset-strength",
            "Strength",
            shown,
            SliderSpec {
                min: 0.0,
                max: 100.0,
                default: 100.0,
                signed: false,
                track: SliderTrack::Amount,
            },
            &theme,
            move |value, _w: &mut Window, cx: &mut App| {
                entity.update(cx, |this, cx| this.set_preset_strength(value, cx));
            },
        );

        div()
            .flex()
            .flex_col()
            .gap_2()
            .px_1()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .w_full()
                    .gap_1()
                    .child(save)
                    .child(recipe)
                    .child(from_photo)
                    .child(match_look)
                    .child(import)
                    .child(export)
                    .child(reset)
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_1()
                            .min_w(sp(22.0))
                            .child(options)
                            .when(popup_open, |el| {
                                el.child(deferred(self.presets_options_popup(cx)).with_priority(1))
                            }),
                    ),
            )
            .child(strength)
            .into_any_element()
    }

    /// The Options pop-up: which parts of a look presets apply and save,
    /// the category new presets are filed under, and the list order.
    fn presets_options_popup(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let ui = &self.develop_ui.preset_ui;
        let scope_chips: Vec<AnyElement> = SCOPE_LABELS
            .iter()
            .enumerate()
            .map(|(i, label)| {
                rustyedit_ui::chrome_button(
                    SharedString::from(format!("preset-scope-{i}")),
                    *label,
                    ui.scope & (1 << i) != 0,
                    &theme,
                    cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.develop_ui.preset_ui.scope ^= 1 << i;
                        cx.notify();
                    }),
                )
                .into_any_element()
            })
            .collect();

        div()
            .id("preset-popup")
            .absolute()
            .top(sp(32.0))
            .right_0()
            .w(sp(280.0))
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .bg(theme.surface_elevated)
            .border_1()
            .border_color(theme.border)
            .rounded_md()
            .shadow_lg()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _w, cx| {
                this.develop_ui.preset_ui.popup_open = false;
                cx.notify();
            }))
            .child(small_caption(&theme, "OPTIONS"))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .items_center()
                    .pt_1()
                    .child(small_caption(&theme, "APPLY & SAVE"))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_1()
                            .child(rustyedit_ui::chrome_button(
                                "preset-scope-all",
                                "All",
                                false,
                                &theme,
                                cx.listener(|this, _: &ClickEvent, _w, cx| {
                                    this.develop_ui.preset_ui.scope =
                                        (1 << DevelopSection::LOOK.len()) - 1;
                                    cx.notify();
                                }),
                            ))
                            .child(rustyedit_ui::chrome_button(
                                "preset-scope-none",
                                "None",
                                false,
                                &theme,
                                cx.listener(|this, _: &ClickEvent, _w, cx| {
                                    this.develop_ui.preset_ui.scope = 0;
                                    cx.notify();
                                }),
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_1()
                    .children(scope_chips),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .pt_2()
                    .child(small_caption(&theme, "SAVE NEW PRESETS IN"))
                    .child(rustyedit_ui::chrome_button(
                        "preset-new-category",
                        format!("{} \u{25B8}", PRESET_CATEGORIES[ui.new_category]),
                        false,
                        &theme,
                        cx.listener(|this, _: &ClickEvent, _w, cx| {
                            let ui = &mut this.develop_ui.preset_ui;
                            ui.new_category = (ui.new_category + 1) % PRESET_CATEGORIES.len();
                            cx.notify();
                        }),
                    )),
            )

            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .pt_2()
                    .child(small_caption(&theme, "LIST ORDER"))
                    .child(rustyedit_ui::chrome_button(
                        "preset-sort",
                        if ui.sort_az {
                            "A\u{2013}Z"
                        } else {
                            "Saved order"
                        },
                        false,
                        &theme,
                        cx.listener(|this, _: &ClickEvent, _w, cx| {
                            let ui = &mut this.develop_ui.preset_ui;
                            ui.sort_az = !ui.sort_az;
                            cx.notify();
                        }),
                    )),
            )
            .into_any_element()
    }

    pub(crate) fn presets_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let ui = &self.develop_ui.preset_ui;
        let store = &self.develop_ui.presets;
        let current = self.catalog.selected_asset().map(|a| a.develop);
        // The preset making up the selected photo's look, if any.
        let applied: Option<String> = self
            .catalog
            .selected_asset()
            .and_then(|a| a.current_preset().map(str::to_string));

        // Everything, with favourites resolved.
        let mut entries: Vec<(Entry, bool)> = builtin_presets()
            .into_iter()
            .map(|p| {
                let fav = store.is_builtin_favorite(&p.name);
                (Entry::BuiltIn(p), fav)
            })
            .chain(store.user.iter().cloned().enumerate().map(|(i, p)| {
                let fav = p.favorite;
                (Entry::Saved(i, p), fav)
            }))
            .collect();
        if ui.sort_az {
            entries.sort_by_key(|(e, _)| e.preset().name.to_lowercase());
        }

        let mut out: Vec<AnyElement> = Vec::new();

        if current.is_none() {
            out.push(
                div()
                    .pt_2()
                    .text_size(sp(10.5))
                    .text_color(theme.text_faint)
                    .child("Select a photo to apply presets.")
                    .into_any_element(),
            );
        }

        // The list: favourites first when browsing everything, then by category.
        let mut groups: Vec<(String, Vec<(Entry, bool)>)> = Vec::new();
        {
            let favs: Vec<_> = entries.iter().filter(|(_, f)| *f).cloned().collect();
            if !favs.is_empty() {
                groups.push(("\u{2605} Favorites".to_string(), favs));
            }
        }
        for cat in PRESET_CATEGORIES {
            let rows: Vec<_> = entries
                .iter()
                .filter(|(e, _)| e.preset().category_label() == cat)
                .cloned()
                .collect();
            if !rows.is_empty() {
                groups.push((cat.to_string(), rows));
            }
        }
        if groups.is_empty() {
            out.push(
                div()
                    .pt_3()
                    .text_size(sp(11.0))
                    .text_color(theme.text_faint)
                    .child("No presets.")
                    .into_any_element(),
            );
        }
        // Clicking a preset uses the strength the slider shows.
        let amount = self
            .catalog
            .selected_asset()
            .and_then(|a| a.current_preset_amount())
            .map_or(ui.amount, |amount| (amount * 100.0).round());
        let scope = ui.scope_sections();
        let renaming = ui.renaming;
        let mut uid = 0usize;
        for (title, rows) in groups {
            let count = rows.len();
            let row_elements: Vec<AnyElement> = rows
                .into_iter()
                .map(|(entry, fav)| {
                    uid += 1;
                    // Ticked when it's the photo's preset (only one can
                    // be), or when the photo's look is exactly it.
                    let name = &entry.preset().name;
                    let active = applied.as_deref() == Some(name.as_str())
                        || current.is_some_and(|c| {
                            c.is_edited()
                                && c.with_look_blended(
                                    &entry.preset().settings,
                                    &DevelopSection::LOOK,
                                    1.0,
                                ) == c
                        });
                    self.preset_row(uid, &entry, fav, active, amount, &scope, renaming, cx)
                })
                .collect();
            // Same card as the Filters panel's groups: rounded border, a
            // raised title band, and an inset body.
            out.push(
                div()
                    .flex()
                    .flex_col()
                    .mt_2()
                    .rounded_md()
                    .overflow_hidden()
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .items_center()
                            .px_2()
                            .py_1()
                            .bg(theme.surface_elevated)
                            .child(
                                div()
                                    .text_size(sp(10.0))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme.text_muted)
                                    .child(title.to_uppercase()),
                            )
                            .child(
                                div()
                                    .text_size(sp(9.5))
                                    .text_color(theme.text_faint)
                                    .child(count.to_string()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .p_1()
                            .bg(theme.background.opacity(0.45))
                            .children(row_elements.into_iter().enumerate().map(|(i, row)| {
                                div()
                                    .when(i > 0, |el| {
                                        el.border_t_1().border_color(theme.border_subtle)
                                    })
                                    .child(row)
                            })),
                    )
                    .into_any_element(),
            );
        }
        out.push(div().h(sp(12.0)).into_any_element());
        // Side padding for the whole panel; the list rows stay edge to edge
        // inside it.
        vec![div()
            .flex()
            .flex_col()
            .px_3()
            .children(out)
            .into_any_element()]
    }

    #[allow(clippy::too_many_arguments)]
    fn preset_row(
        &self,
        uid: usize,
        entry: &Entry,
        favorite: bool,
        active: bool,
        amount: f32,
        scope: &[DevelopSection],
        renaming: Option<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let preset = entry.preset().clone();
        let look = preset.settings;
        let group = SharedString::from(format!("preset-row-{uid}"));

        // Inline rename replaces the whole row.
        if let (Entry::Saved(ix, _), Some(rix)) = (entry, renaming) {
            if *ix == rix {
                if let Some(draft) = self.draft_for(TextTarget::PresetRename) {
                    return div()
                        .p_1()
                        .child(name_field(&theme, draft, "New name, then \u{23CE}"))
                        .into_any_element();
                }
            }
        }

        let scope = scope.to_vec();
        let apply_preset = preset.clone();
        let fav_entry = entry.clone();

        let action =
            |id: String,
             glyph: &'static str,
             tip: &'static str,
             danger: bool,
             on_click: Box<dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)>,
             cx: &mut Context<Self>|
             -> AnyElement {
                let theme = theme;
                let entity = cx.entity();
                div()
                    .id(SharedString::from(id))
                    .flex_shrink_0()
                    .px_1()
                    .text_size(sp(10.5))
                    .text_color(theme.text_faint)
                    .cursor_pointer()
                    .hover(|el| el.text_color(if danger { theme.danger } else { theme.text }))
                    .tooltip(rustyedit_ui::tip(tip, &theme))
                    .child(glyph)
                    .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                        cx.stop_propagation();
                        entity.update(cx, |this, cx| on_click(this, window, cx));
                    })
                    .into_any_element()
            };

        let star = {
            let entity = cx.entity();
            div()
                .id(SharedString::from(format!("preset-star-{uid}")))
                .flex_shrink_0()
                .px_0p5()
                .text_size(sp(11.0))
                .text_color(if favorite {
                    theme.star
                } else {
                    theme.text_faint
                })
                .when(!favorite, |el| {
                    el.invisible().group_hover(group.clone(), |s| s.visible())
                })
                .cursor_pointer()
                .hover(|el| el.text_color(theme.star))
                .child(if favorite { "\u{2605}" } else { "\u{2606}" })
                .on_click(move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                    cx.stop_propagation();
                    let entry = fav_entry.clone();
                    entity.update(cx, |this, cx| this.toggle_preset_favorite(&entry, cx));
                })
        };

        let mut actions: Vec<AnyElement> = Vec::new();
        if let Entry::Saved(ix, _) = entry {
            let ix = *ix;
            let n = self.develop_ui.presets.user.len();
            actions.push(action(
                format!("preset-up-{uid}"),
                "\u{25B2}",
                "Move up",
                false,
                Box::new(move |this, _, cx| this.move_preset(ix, -1, cx)),
                cx,
            ));
            actions.push(action(
                format!("preset-down-{uid}"),
                "\u{25BC}",
                "Move down",
                false,
                Box::new(move |this, _, cx| this.move_preset(ix, 1, cx)),
                cx,
            ));
            let _ = n;
            actions.push(action(
                format!("preset-update-{uid}"),
                "\u{21BB}",
                "Update with the current photo's look",
                false,
                Box::new(move |this, _, cx| this.update_preset_from_photo(ix, cx)),
                cx,
            ));
            actions.push(action(
                format!("preset-cat-{uid}"),
                "\u{25EB}",
                "Move to the next category",
                false,
                Box::new(move |this, _, cx| {
                    this.develop_ui.presets.next_category(ix);
                    this.persist_presets("Moved preset".to_string());
                    cx.notify();
                }),
                cx,
            ));
            actions.push(action(
                format!("preset-dup-{uid}"),
                "\u{29C9}",
                "Duplicate",
                false,
                Box::new(move |this, _, cx| {
                    this.develop_ui.presets.duplicate(ix);
                    this.persist_presets("Duplicated preset".to_string());
                    cx.notify();
                }),
                cx,
            ));
            actions.push(action(
                format!("preset-rename-{uid}"),
                "\u{270E}",
                "Rename",
                false,
                Box::new(move |this, window, cx| {
                    this.develop_ui.preset_ui.renaming = Some(ix);
                    this.begin_text_draft(TextTarget::PresetRename, window, cx);
                }),
                cx,
            ));
            actions.push(action(
                format!("preset-del-{uid}"),
                "\u{2715}",
                "Delete",
                true,
                Box::new(move |this, _, cx| this.delete_preset(ix, cx)),
                cx,
            ));
        }

        div()
            .id(SharedString::from(format!("preset-row-el-{uid}")))
            .group(group.clone())
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .h(sp(30.0))
            .pl_2()
            .pr_1()
            .cursor_pointer()
            .text_size(sp(11.5))
            .when(active, |el| {
                el.bg(theme.accent_muted.opacity(0.6))
                    .text_color(theme.text)
            })
            .when(!active, |el| {
                el.text_color(theme.text_muted)
                    .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
            })
            .when(active, |el| {
                el.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(sp(2.0))
                        .bg(theme.accent),
                )
            })
            .child(look_chip(&look))
            .child(
                div()
                    .flex_1()
                    .min_w(sp(0.0))
                    .truncate()
                    .child(preset.name.clone()),
            )
            // A saved preset's actions float over the row's right end while
            // it's hovered, so they never take room from the name.
            .when(matches!(entry, Entry::Saved(..)), |el| {
                el.child(
                    div()
                        .absolute()
                        .right(sp(22.0))
                        .top_0()
                        .bottom_0()
                        .flex()
                        .flex_row()
                        .items_center()
                        .pl_2()
                        .rounded_md()
                        .bg(theme.surface_elevated)
                        .invisible()
                        .group_hover(group.clone(), |s| s.visible())
                        .children(actions),
                )
            })
            .child(star)
            .when(active, |el| {
                el.child(
                    div()
                        .flex_shrink_0()
                        .text_size(sp(10.0))
                        .text_color(theme.accent)
                        .child("\u{2713}"),
                )
            })
            .on_hover({
                let preset = preset.clone();
                let scope = scope.clone();
                cx.listener(move |this, hovered: &bool, _w, cx| {
                    if *hovered {
                        this.preview_preset(&preset, amount / 100.0, &scope, cx);
                    } else {
                        this.end_preset_preview(&preset.name, cx);
                    }
                })
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                this.apply_preset_blend(&apply_preset, amount / 100.0, &scope, cx)
            }))
            .into_any_element()
    }

    /// Shows the selected photo as it would look with `preset` applied now
    /// (at `amount`, for `sections`), without changing it.
    fn preview_preset(
        &mut self,
        preset: &DevelopPreset,
        amount: f32,
        sections: &[DevelopSection],
        cx: &mut Context<Self>,
    ) {
        if !self.settings.develop.preset_hover_preview || sections.is_empty() {
            return;
        }
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        let mut next = asset.clone();
        next.apply_preset(&preset.name, &preset.settings, sections, amount);
        self.preset_hover = Some(preset.name.clone());
        self.preview_override = Some(next.develop);
        self.refresh_live_preview(cx);
        cx.notify();
    }

    /// The pointer left `name`'s row: back to the photo's own look (unless
    /// another preset is already being previewed).
    fn end_preset_preview(&mut self, name: &str, cx: &mut Context<Self>) {
        if self.preset_hover.as_deref() != Some(name) {
            return;
        }
        self.preset_hover = None;
        if self.preview_override.take().is_some() {
            self.refresh_live_preview(cx);
            cx.notify();
        }
    }

    /// Gives every target photo the preset's look, at `amount` (0-1) and for
    /// the chosen `sections`, keeping each photo's geometry.
    pub(super) fn apply_preset_blend(
        &mut self,
        preset: &DevelopPreset,
        amount: f32,
        sections: &[DevelopSection],
        cx: &mut Context<Self>,
    ) {
        if self.catalog.selected_asset().is_none() {
            return;
        }
        if sections.is_empty() {
            self.status_message = Some("Turn on at least one section to apply.".into());
            cx.notify();
            return;
        }
        // The hovered look becomes the real one.
        self.preview_override = None;
        self.preset_hover = None;
        let look = preset.settings;
        let detail = if amount < 1.0 {
            format!("{} ({}%)", preset.name, (amount * 100.0).round())
        } else {
            preset.name.clone()
        };
        let targets = self.target_indices();
        let count = targets.len();
        // Each photo's new look, and the preset record that goes with it.
        // Applying a preset replaces the one before it entirely.
        let mut updates = Vec::new();
        let mut records = std::collections::HashMap::new();
        for ix in targets {
            let Some(asset) = self.catalog.assets.get(ix) else {
                continue;
            };
            let mut next = asset.clone();
            next.apply_preset(&preset.name, &look, sections, amount);
            updates.push((ix, next.develop));
            records.insert(asset.id, next.applied_preset);
        }
        self.develop_ui.preset_ui.amount = (amount * 100.0).round();
        self.replace_develop_each_and(
            updates,
            &format!("preset \u{201C}{}\u{201D}", preset.name),
            HistoryNote::Record {
                title: "Preset",
                detail: &detail,
            },
            false,
            cx,
            move |asset, _| {
                if let Some(record) = records.get(&asset.id) {
                    asset.applied_preset = record.clone();
                }
            },
        );
        self.status_message = Some(
            format!(
                "Applied \u{201C}{}\u{201D} to {count} photo{}",
                preset.name,
                if count == 1 { "" } else { "s" }
            )
            .into(),
        );
        cx.notify();
    }

    /// The preset called `name`: the user's own first, then a built-in.
    fn find_preset(&self, name: &str) -> Option<DevelopPreset> {
        self.develop_ui
            .presets
            .user
            .iter()
            .find(|p| p.name == name)
            .cloned()
            .or_else(|| builtin_presets().into_iter().find(|p| p.name == name))
    }

    /// The Strength slider: remembered for the next preset clicked, and
    /// applied straight away to the target photos' current presets. Each
    /// photo re-applies its own preset from its pre-preset settings, so any
    /// strength can be set and changed back exactly.
    fn set_preset_strength(&mut self, value: f32, cx: &mut Context<Self>) {
        let value = value.clamp(0.0, 100.0);
        self.develop_ui.preset_ui.amount = value;
        let sections = self.develop_ui.preset_ui.scope_sections();
        let mut updates = Vec::new();
        let mut records = std::collections::HashMap::new();
        for ix in self.target_indices() {
            let Some(asset) = self.catalog.assets.get(ix) else {
                continue;
            };
            let Some(name) = asset.current_preset().map(str::to_string) else {
                continue;
            };
            let Some(preset) = self.find_preset(&name) else {
                continue;
            };
            let mut next = asset.clone();
            next.apply_preset(&name, &preset.settings, &sections, value / 100.0);
            updates.push((ix, next.develop));
            records.insert(asset.id, next.applied_preset);
        }
        if updates.is_empty() {
            cx.notify();
            return;
        }
        let detail = format!("{}%", value.round());
        self.replace_develop_each_and(
            updates,
            "preset strength",
            HistoryNote::Record {
                title: "Preset strength",
                detail: &detail,
            },
            true,
            cx,
            move |asset, _| {
                if let Some(record) = records.get(&asset.id) {
                    asset.applied_preset = record.clone();
                }
            },
        );
        cx.notify();
    }

    /// Replaces the look of every target photo with `look`.
    fn apply_neutral_look(&mut self, look: &DevelopSettings, title: &str, cx: &mut Context<Self>) {
        if self.catalog.selected_asset().is_none() {
            return;
        }
        let (look, sections) = (*look, self.develop_ui.preset_ui.scope_sections());
        self.replace_develop_and(
            self.target_indices(),
            title,
            HistoryNote::Record {
                title: "Preset",
                detail: title,
            },
            cx,
            move |develop: &DevelopSettings| develop.with_look_blended(&look, &sections, 1.0),
            |asset, _| asset.applied_preset = None,
        );
    }

    /// Saves the selected photo's look under `name` (numbered if blank), in
    /// the chosen category and with only the chosen sections.
    pub(crate) fn save_preset(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(develop) = self.catalog.selected_asset().map(|a| a.develop) else {
            return;
        };
        let ui = &self.develop_ui.preset_ui;
        let (category, sections) = (PRESET_CATEGORIES[ui.new_category], ui.scope_sections());
        if sections.is_empty() {
            self.status_message = Some("Turn on at least one section to save.".into());
            cx.notify();
            return;
        }
        let name = self
            .develop_ui
            .presets
            .save_in(&name, category, &develop, &sections);
        self.persist_presets(format!("Saved preset \u{201C}{name}\u{201D}"));
        cx.notify();
    }

    pub(crate) fn rename_preset(&mut self, name: String, cx: &mut Context<Self>) {
        if let Some(ix) = self.develop_ui.preset_ui.renaming.take() {
            if self.develop_ui.presets.rename(ix, &name) {
                self.persist_presets("Renamed preset".to_string());
            } else {
                self.status_message = Some("That name is empty or already used.".into());
            }
        }
        cx.notify();
    }

    fn update_preset_from_photo(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(develop) = self.catalog.selected_asset().map(|a| a.develop) else {
            return;
        };
        let sections = self.develop_ui.preset_ui.scope_sections();
        self.develop_ui
            .presets
            .overwrite(index, &develop, &sections);
        self.persist_presets("Updated preset from the current photo".to_string());
        cx.notify();
    }

    fn move_preset(&mut self, index: usize, delta: i32, cx: &mut Context<Self>) {
        self.develop_ui.presets.move_by(index, delta);
        self.persist_presets("Moved preset".to_string());
        cx.notify();
    }

    fn toggle_preset_favorite(&mut self, entry: &Entry, cx: &mut Context<Self>) {
        match entry {
            Entry::BuiltIn(p) => self.develop_ui.presets.toggle_builtin_favorite(&p.name),
            Entry::Saved(ix, _) => self.develop_ui.presets.toggle_favorite(*ix),
        }
        self.persist_presets("Updated favorites".to_string());
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

    fn import_presets(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let file = cx
                .background_executor()
                // Async: the blocking dialog fails on Windows' background
                // threads.
                .spawn(async {
                    rfd::AsyncFileDialog::new()
                        .add_filter("Presets", &["json"])
                        .pick_file()
                        .await
                        .map(|handle| handle.path().to_path_buf())
                })
                .await;
            let Some(file) = file else { return };
            let result = rustyedit_io::import_presets(&file);
            this.update(cx, |this, cx| {
                match result {
                    Ok(list) => {
                        let added = this.develop_ui.presets.import(list);
                        this.persist_presets(format!(
                            "Imported {added} preset{}",
                            if added == 1 { "" } else { "s" }
                        ));
                    }
                    Err(err) => {
                        this.status_message = Some(format!("Couldn't import presets: {err}").into())
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn export_presets(&mut self, cx: &mut Context<Self>) {
        let presets = self.develop_ui.presets.user.clone();
        if presets.is_empty() {
            self.status_message = Some("Save a preset first; there's nothing to export.".into());
            cx.notify();
            return;
        }
        cx.spawn(async move |this, cx| {
            let file = cx
                .background_executor()
                .spawn(async {
                    rfd::AsyncFileDialog::new()
                        .set_file_name("rustyedit-presets.json")
                        .add_filter("Presets", &["json"])
                        .save_file()
                        .await
                        .map(|handle| handle.path().to_path_buf())
                })
                .await;
            let Some(file) = file else { return };
            let result = rustyedit_io::export_presets(&file, &presets);
            this.update(cx, |this, cx| {
                this.status_message = Some(match result {
                    Ok(()) => format!("Exported {} presets", presets.len()).into(),
                    Err(err) => format!("Couldn't export presets: {err}").into(),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn persist_presets(&mut self, done: String) {
        self.status_message = Some(match rustyedit_io::save_presets(&self.develop_ui.presets) {
            Ok(()) => done.into(),
            Err(err) => format!("Couldn't save presets: {err}").into(),
        });
    }
}

fn small_caption(theme: &Theme, text: &str) -> AnyElement {
    div()
        .text_size(sp(9.5))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(theme.text_muted)
        .child(text.to_string())
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
        .w(sp(22.0))
        .h(sp(16.0))
        .rounded(sp(3.0))
        .border_1()
        .border_color(gpui::black().opacity(0.25))
        .bg(linear_gradient(
            180.0,
            linear_color_stop(hsla(hue, saturation, top, 1.0), 0.0),
            linear_color_stop(hsla(hue, saturation * 0.8, bottom, 1.0), 1.0),
        ))
        .into_any_element()
}
