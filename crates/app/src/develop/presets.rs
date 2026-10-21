//! The Presets rail tool: built-in and saved looks, searchable and
//! filterable by category or favourites, applied at any strength to any
//! mix of sections, with saving, updating, renaming, duplicating, reordering
//! and importing / exporting of the user's own.

use gpui::{
    deferred, div, hsla, linear_color_stop, linear_gradient, prelude::*, px, AnyElement, App,
    ClickEvent, Context, MouseDownEvent, SharedString, Window,
};
use rustyedit_core::{
    builtin_presets, DevelopPreset, DevelopSection, DevelopSettings, PRESET_CATEGORIES,
};
use rustyedit_ui::{SliderSpec, SliderTrack, Theme};

use super::{tools::name_field, HistoryNote};
use crate::workspace::{TextTarget, Workspace};

/// Which presets the panel lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PresetFilter {
    #[default]
    All,
    Favorites,
    BuiltIn,
    Mine,
    Category(usize),
}

/// View state of the Presets panel.
pub(crate) struct PresetUi {
    pub(crate) filter: PresetFilter,
    /// 0-100: how strongly a clicked preset is applied.
    pub(crate) amount: f32,
    /// Bit per [`DevelopSection::LOOK`] entry: which parts of a look are
    /// applied, and saved into new presets.
    pub(crate) scope: u16,
    /// Category a newly saved preset is filed under.
    pub(crate) new_category: usize,
    pub(crate) search: String,
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
            filter: PresetFilter::All,
            amount: 100.0,
            scope: (1 << DevelopSection::LOOK.len()) - 1,
            new_category: PRESET_CATEGORIES.len() - 1,
            search: String::new(),
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
    pub(crate) fn presets_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let ui = &self.develop_ui.preset_ui;
        let store = &self.develop_ui.presets;
        let current = self.catalog.selected_asset().map(|a| a.develop);
        let query = self
            .draft_for(TextTarget::PresetSearch)
            .unwrap_or(&ui.search)
            .to_lowercase();

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
        let total = entries.len();
        entries.retain(|(entry, fav)| {
            let p = entry.preset();
            let matches_filter = match ui.filter {
                PresetFilter::All => true,
                PresetFilter::Favorites => *fav,
                PresetFilter::BuiltIn => matches!(entry, Entry::BuiltIn(_)),
                PresetFilter::Mine => matches!(entry, Entry::Saved(..)),
                PresetFilter::Category(i) => PRESET_CATEGORIES
                    .get(i)
                    .is_some_and(|c| *c == p.category_label()),
            };
            matches_filter
                && (query.is_empty()
                    || p.name.to_lowercase().contains(&query)
                    || p.category_label().to_lowercase().contains(&query))
        });
        let shown = entries.len();
        if ui.sort_az {
            entries.sort_by_key(|(e, _)| e.preset().name.to_lowercase());
        }

        let mut out: Vec<AnyElement> = Vec::new();
        let mut popup: Vec<AnyElement> = Vec::new();

        // Search
        let editing = self.draft_for(TextTarget::PresetSearch);
        let shown_query = editing.unwrap_or(&ui.search).to_string();
        popup.push(
            div()
                .id("preset-search")
                .flex()
                .flex_row()
                .items_center()
                .h(px(28.0))
                .px_2()
                .bg(theme.background)
                .border_1()
                .border_color(if editing.is_some() {
                    theme.accent
                } else {
                    theme.border_subtle
                })
                .text_sm()
                .cursor_text()
                .child(
                    div()
                        .flex_1()
                        .text_color(if shown_query.is_empty() {
                            theme.text_faint
                        } else {
                            theme.text
                        })
                        .child(if shown_query.is_empty() && editing.is_none() {
                            "Search presets\u{2026}".to_string()
                        } else {
                            shown_query.clone()
                        }),
                )
                .when(editing.is_some(), |el| {
                    el.child(div().w(px(1.0)).h(px(14.0)).bg(theme.accent))
                })
                .when(!shown_query.is_empty(), |el| {
                    el.child(
                        div()
                            .id("preset-search-clear")
                            .ml_1()
                            .px_1()
                            .text_xs()
                            .text_color(theme.text_muted)
                            .cursor_pointer()
                            .hover(|el| el.text_color(theme.text))
                            .child("\u{2715}")
                            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                                this.text_draft = None;
                                this.develop_ui.preset_ui.search.clear();
                                cx.stop_propagation();
                                cx.notify();
                            })),
                    )
                })
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.begin_text_draft(TextTarget::PresetSearch, window, cx)
                }))
                .into_any_element(),
        );

        // Filter chips
        let mut chips: Vec<AnyElement> = Vec::new();
        let mut chip = |id: String, label: String, active: bool, filter: PresetFilter| {
            chips.push(
                rustyedit_ui::chrome_button(
                    SharedString::from(id),
                    label,
                    active,
                    &theme,
                    cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.develop_ui.preset_ui.filter = filter;
                        cx.notify();
                    }),
                )
                .into_any_element(),
            );
        };
        let fav_count =
            store.user.iter().filter(|p| p.favorite).count() + store.builtin_favorites.len();
        chip(
            "preset-f-all".into(),
            "All".into(),
            ui.filter == PresetFilter::All,
            PresetFilter::All,
        );
        chip(
            "preset-f-fav".into(),
            format!("\u{2605} {fav_count}"),
            ui.filter == PresetFilter::Favorites,
            PresetFilter::Favorites,
        );
        chip(
            "preset-f-builtin".into(),
            "Built-in".into(),
            ui.filter == PresetFilter::BuiltIn,
            PresetFilter::BuiltIn,
        );
        chip(
            "preset-f-mine".into(),
            format!("Mine {}", store.user.len()),
            ui.filter == PresetFilter::Mine,
            PresetFilter::Mine,
        );
        for (i, name) in PRESET_CATEGORIES.iter().enumerate() {
            chip(
                format!("preset-f-cat-{i}"),
                (*name).to_string(),
                ui.filter == PresetFilter::Category(i),
                PresetFilter::Category(i),
            );
        }
        out.push(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_1()
                .pt_2()
                .children(chips)
                .into_any_element(),
        );

        // Strength
        let entity = cx.entity();
        out.push(
            div()
                .pt_3()
                .child(rustyedit_ui::labeled_slider(
                    "preset-strength",
                    "Strength",
                    ui.amount,
                    SliderSpec {
                        min: 0.0,
                        max: 100.0,
                        default: 100.0,
                        signed: false,
                        track: SliderTrack::Accent,
                    },
                    &theme,
                    move |value, _w: &mut Window, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.develop_ui.preset_ui.amount = value.clamp(0.0, 100.0);
                            cx.notify();
                        });
                    },
                ))
                .into_any_element(),
        );

        // Which parts of a look to apply / save
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
        let scope_block = Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .pt_2()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .items_center()
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
                .into_any_element(),
        );

        // Save the current look
        let save = match self.draft_for(TextTarget::PresetName) {
            Some(draft) => name_field(&theme, draft, "Preset name, then \u{23CE}"),
            None => div()
                .id("preset-save")
                .flex()
                .flex_row()
                .items_center()
                .justify_center()
                .gap_1p5()
                .h(px(28.0))
                .border_1()
                .border_color(theme.accent.opacity(0.5))
                .bg(theme.accent.opacity(0.08))
                .text_size(px(11.5))
                .text_color(theme.accent)
                .cursor_pointer()
                .hover(|el| el.bg(theme.accent.opacity(0.16)).border_color(theme.accent))
                .child(div().text_size(px(13.0)).child("+"))
                .child("Save current look")
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.begin_text_draft(TextTarget::PresetName, window, cx);
                }))
                .into_any_element(),
        };
        out.push(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .pt_3()
                .child(save)
                .child(div().child(rustyedit_ui::chrome_button(
                    "preset-fuji-recipe",
                    "Fujifilm recipe\u{2026}",
                    false,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| this.open_recipe_modal(cx)),
                )))
                .into_any_element(),
        );

        popup.extend(scope_block);
        popup.push(
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
                ))
                .into_any_element(),
        );

        // Sort
        popup.push(
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
                ))
                .into_any_element(),
        );

        // Header: the title and count, and the button that opens the
        // search & options pop-up.
        let searching = !ui.search.is_empty();
        let popup_open = ui.popup_open;
        let popup_button = div()
            .id("preset-popup-button")
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .h(px(22.0))
            .px_2()
            .cursor_pointer()
            .text_size(px(10.5))
            .when(popup_open || searching, |el| {
                el.bg(theme.accent_muted).text_color(theme.text)
            })
            .when(!(popup_open || searching), |el| {
                el.bg(theme.surface_elevated)
                    .text_color(theme.text_muted)
                    .hover(|el| el.text_color(theme.text))
            })
            .child(rustyedit_ui::icon(
                rustyedit_ui::Icon::Search,
                12.0,
                if popup_open || searching {
                    theme.text
                } else {
                    theme.text_muted
                },
            ))
            .child("Search & options")
            .child(rustyedit_ui::icon(
                rustyedit_ui::Icon::ChevronDown,
                10.0,
                theme.text_faint,
            ))
            .tooltip(rustyedit_ui::tip(
                "Search presets, choose what to apply and save, list order",
                &theme,
            ))
            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                let ui = &mut this.develop_ui.preset_ui;
                ui.popup_open = !ui.popup_open;
                cx.notify();
            }));
        let popup_el = div()
            .id("preset-popup")
            .absolute()
            .top(px(26.0))
            .right_0()
            .w(px(280.0))
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
            .child(small_caption(&theme, "SEARCH"))
            .children(popup);
        out.insert(
            0,
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .pt_3()
                .pb_1()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_baseline()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(10.0))
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(theme.accent)
                                .child("PRESETS"),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(theme.text_faint)
                                .child(if shown == total {
                                    format!("{total}")
                                } else {
                                    format!("{shown} of {total}")
                                }),
                        ),
                )
                .child(div().relative().child(popup_button).when(popup_open, |el| {
                    el.child(deferred(popup_el).with_priority(1))
                }))
                .into_any_element(),
        );

        if current.is_none() {
            out.push(
                div()
                    .pt_2()
                    .text_size(px(10.5))
                    .text_color(theme.text_faint)
                    .child("Select a photo to apply presets.")
                    .into_any_element(),
            );
        }

        // The list: favourites first when browsing everything, then by category.
        let mut groups: Vec<(String, Vec<(Entry, bool)>)> = Vec::new();
        if ui.filter == PresetFilter::All {
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
                    .text_size(px(11.0))
                    .text_color(theme.text_faint)
                    .child(if ui.filter == PresetFilter::Favorites {
                        "Star a preset to keep it here."
                    } else {
                        "No presets match."
                    })
                    .into_any_element(),
            );
        }
        let amount = ui.amount;
        let scope = ui.scope_sections();
        let renaming = ui.renaming;
        let mut uid = 0usize;
        for (title, rows) in groups {
            let count = rows.len();
            let row_elements: Vec<AnyElement> = rows
                .into_iter()
                .map(|(entry, fav)| {
                    uid += 1;
                    let active = current.is_some_and(|c| {
                        c.with_look_blended(&entry.preset().settings, &DevelopSection::LOOK, 1.0)
                            == c
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
                                    .text_size(px(10.0))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme.text_muted)
                                    .child(title.to_uppercase()),
                            )
                            .child(
                                div()
                                    .text_size(px(9.5))
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
        out.push(div().h(px(12.0)).into_any_element());
        // Side padding for the whole panel; the list rows stay edge to edge
        // inside it.
        vec![div()
            .flex()
            .flex_col()
            .px_3()
            .children(out)
            .into_any_element()]
    }

    /// The Presets tab's footer: Import, Export and Reset look.
    pub(crate) fn presets_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let has_photo = self.catalog.selected_asset().is_some();
        rustyedit_ui::panel_footer(
            &theme,
            vec![vec![
                rustyedit_ui::footer_button(
                    "preset-import",
                    "Import\u{2026}",
                    "Add presets from a file",
                    true,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| this.import_presets(cx)),
                ),
                rustyedit_ui::footer_button(
                    "preset-export",
                    "Export\u{2026}",
                    "Save your presets to a file",
                    true,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| this.export_presets(cx)),
                ),
                rustyedit_ui::footer_button(
                    "preset-neutral",
                    "Reset look",
                    "Clear the tone, colour and detail edits of the selected photos",
                    has_photo,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| {
                        this.apply_neutral_look(&DevelopSettings::default(), "Reset look", cx)
                    }),
                ),
            ]],
        )
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
                    .text_size(px(10.5))
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
                .text_size(px(11.0))
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
            .h(px(30.0))
            .pl_2()
            .pr_1()
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
            .child(look_chip(&look))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(preset.name.clone()),
            )
            .when(matches!(entry, Entry::Saved(..)), |el| {
                el.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
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
                        .text_size(px(10.0))
                        .text_color(theme.accent)
                        .child("\u{2713}"),
                )
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                this.apply_preset_blend(&apply_preset, amount / 100.0, &scope, cx)
            }))
            .into_any_element()
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
        let look = preset.settings;
        let sections = sections.to_vec();
        let detail = if amount < 1.0 {
            format!("{} ({}%)", preset.name, (amount * 100.0).round())
        } else {
            preset.name.clone()
        };
        self.replace_develop(
            self.target_indices(),
            &format!("preset \u{201C}{}\u{201D}", preset.name),
            HistoryNote::Record {
                title: "Preset",
                detail: &detail,
            },
            cx,
            move |develop: &DevelopSettings| develop.with_look_blended(&look, &sections, amount),
        );
    }

    /// Replaces the look of every target photo with `look`.
    fn apply_neutral_look(&mut self, look: &DevelopSettings, title: &str, cx: &mut Context<Self>) {
        if self.catalog.selected_asset().is_none() {
            return;
        }
        let (look, sections) = (*look, self.develop_ui.preset_ui.scope_sections());
        self.replace_develop(
            self.target_indices(),
            title,
            HistoryNote::Record {
                title: "Preset",
                detail: title,
            },
            cx,
            move |develop: &DevelopSettings| develop.with_look_blended(&look, &sections, 1.0),
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
        self.develop_ui.preset_ui.filter = PresetFilter::Mine;
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
                .spawn(async {
                    rfd::FileDialog::new()
                        .add_filter("Presets", &["json"])
                        .pick_file()
                })
                .await;
            let Some(file) = file else { return };
            let result = rustyedit_io::import_presets(&file);
            this.update(cx, |this, cx| {
                match result {
                    Ok(list) => {
                        let added = this.develop_ui.presets.import(list);
                        this.develop_ui.preset_ui.filter = PresetFilter::Mine;
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
                    rfd::FileDialog::new()
                        .set_file_name("rustyedit-presets.json")
                        .add_filter("Presets", &["json"])
                        .save_file()
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
        .text_size(px(9.5))
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
