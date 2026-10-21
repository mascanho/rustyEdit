//! Text search and the facet filters (colour label, camera, lens, file type,
//! year, ISO, focal length, edited state). Both live in the left dock, as
//! the Search and Filters rail tools. The matching itself is
//! `rustyedit_core::FilterSet`; this is the UI and the plumbing to apply it.

use std::collections::HashMap;

use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context, SharedString, Window};
use rustyedit_core::{
    Catalog, ColorLabel, FilterMode, FilterSet, FocalBand, IsoBand, RatingFilter,
};

use crate::workspace::{TextTarget, Workspace};

/// Distinct values (with counts) of the facets that never change while a
/// catalog is open, computed once at import.
#[derive(Default)]
pub(crate) struct Facets {
    cameras: Vec<(String, usize)>,
    lenses: Vec<(String, usize)>,
    kinds: Vec<(String, usize)>,
    years: Vec<(i32, usize)>,
    iso: [usize; 4],
    focal: [usize; 3],
}

impl Facets {
    pub(crate) fn build(catalog: &Catalog) -> Self {
        let mut cameras: HashMap<&str, usize> = HashMap::new();
        let mut lenses: HashMap<&str, usize> = HashMap::new();
        let mut kinds: HashMap<String, usize> = HashMap::new();
        let mut years: HashMap<i32, usize> = HashMap::new();
        let mut facets = Facets::default();

        for asset in &catalog.assets {
            if !asset.meta.camera.is_empty() {
                *cameras.entry(&asset.meta.camera).or_default() += 1;
            }
            if !asset.meta.lens.is_empty() {
                *lenses.entry(&asset.meta.lens).or_default() += 1;
            }
            *kinds.entry(asset.kind()).or_default() += 1;
            if let Some(year) = asset.meta.year() {
                *years.entry(year).or_default() += 1;
            }
            if let Some(band) = IsoBand::of(asset.meta.iso) {
                facets.iso[IsoBand::ALL.iter().position(|b| *b == band).unwrap_or(0)] += 1;
            }
            if let Some(band) = FocalBand::of(asset.meta.focal_mm) {
                facets.focal[FocalBand::ALL.iter().position(|b| *b == band).unwrap_or(0)] += 1;
            }
        }

        let by_count = |map: HashMap<String, usize>| {
            let mut list: Vec<_> = map.into_iter().collect();
            list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            list
        };
        facets.cameras = by_count(
            cameras
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        );
        facets.lenses = by_count(
            lenses
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        );
        facets.kinds = by_count(kinds);
        facets.years = {
            let mut list: Vec<_> = years.into_iter().collect();
            list.sort_by(|a, b| b.0.cmp(&a.0));
            list
        };
        facets
    }
}

impl Workspace {
    /// Applies a change to the facet filters and refreshes the view.
    pub(crate) fn update_filters(
        &mut self,
        change: impl FnOnce(&mut FilterSet),
        cx: &mut Context<Self>,
    ) {
        let mut filters = self.catalog.filters().clone();
        change(&mut filters);
        self.catalog.set_filters(filters);
        self.on_selection_changed(cx);
    }

    pub(crate) fn set_search_query(&mut self, query: String, cx: &mut Context<Self>) {
        self.update_filters(|f| f.search = query, cx);
    }

    /// Opens the left dock on `tool` (Search / Filters).
    pub(crate) fn open_left_tool(
        &mut self,
        tool: rustyedit_core::RailTool,
        cx: &mut Context<Self>,
    ) {
        self.left_panel_collapsed = false;
        self.left_rail_tool = Some(tool);
        cx.notify();
    }

    pub fn focus_search(
        &mut self,
        _: &crate::actions::FocusSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode != crate::workspace::Mode::Library {
            return;
        }
        self.open_left_tool(rustyedit_core::RailTool::Search, cx);
        self.begin_search(window, cx);
    }

    fn begin_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.catalog.filters().search.clone();
        self.text_draft = Some((TextTarget::Search, current));
        window.focus(&self.focus_handle);
        cx.notify();
    }

    /// The active text search, floating at the top-right of the photos. It
    /// has a fixed maximum width so a long query truncates instead of
    /// resizing anything.
    pub(crate) fn render_search_chip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = self.theme;
        let query = self.catalog.filters().search.trim().to_string();
        if query.is_empty() {
            return None;
        }
        Some(
            div()
                .id("search-chip")
                .absolute()
                .top(px(12.0))
                .right(px(12.0))
                .max_w(px(260.0))
                .occlude()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .h(px(28.0))
                .pl_3()
                .pr_1()
                .rounded_full()
                .bg(theme.background.opacity(0.85))
                .border_1()
                .border_color(theme.accent.opacity(0.6))
                .shadow_md()
                .text_xs()
                .text_color(theme.text)
                .cursor_pointer()
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(theme.accent)
                        .child("\u{2315}"),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(query),
                )
                .child(rustyedit_ui::glyph_button(
                    "search-chip-clear",
                    "\u{2715}",
                    "Clear search",
                    theme.text_muted,
                    true,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| {
                        this.text_draft = None;
                        this.set_search_query(String::new(), cx);
                    }),
                ))
                // Clicking the chip itself reopens the search panel.
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.open_left_tool(rustyedit_core::RailTool::Search, cx);
                    this.begin_search(window, cx);
                }))
                .into_any_element(),
        )
    }

    // --- Search panel ----------------------------------------------------------

    pub(crate) fn search_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let editing = self.draft_for(TextTarget::Search);
        let query = editing
            .unwrap_or(&self.catalog.filters().search)
            .to_string();
        let matched = self.catalog.visible_indices().len();
        let total = self.catalog.assets.len();

        let field = div()
            .id("search-field")
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
                    .text_color(if query.is_empty() {
                        theme.text_faint
                    } else {
                        theme.text
                    })
                    .child(if query.is_empty() && editing.is_none() {
                        "Search photos\u{2026}".to_string()
                    } else {
                        query.clone()
                    }),
            )
            .when(editing.is_some(), |el| {
                el.child(div().w(px(1.0)).h(px(14.0)).ml(px(1.0)).bg(theme.accent))
            })
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.begin_search(window, cx)))
            .into_any_element();

        let mut sections = vec![
            div()
                .pt_3()
                .pb_1()
                .text_size(px(10.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(theme.accent)
                .child("SEARCH")
                .into_any_element(),
            field,
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .pt_2()
                .text_size(px(10.5))
                .text_color(theme.text_faint)
                .child(if self.catalog.filters().search.trim().is_empty() {
                    format!("{total} photos")
                } else {
                    format!("{matched} of {total} match")
                })
                .child(rustyedit_ui::glyph_button(
                    "search-clear",
                    "\u{2715}",
                    "Clear search",
                    theme.text_muted,
                    !query.is_empty(),
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| {
                        this.text_draft = None;
                        this.set_search_query(String::new(), cx);
                    }),
                ))
                .into_any_element(),
            div()
                .pt_2()
                .text_size(px(10.5))
                .text_color(theme.text_faint)
                .child(
                    "Every word must appear in the file name, camera, lens, file type, \
                     title, caption, creator, copyright or keywords. Combine with the \
                     Filters panel to narrow further.",
                )
                .into_any_element(),
        ];
        if self.catalog.assets.is_empty() {
            sections.push(
                div()
                    .pt_2()
                    .text_xs()
                    .text_color(theme.text_faint)
                    .child("Import a folder to search it.")
                    .into_any_element(),
            );
        }
        sections
    }

    // --- Filters panel ---------------------------------------------------------

    pub(crate) fn filters_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let filters = self.catalog.filters().clone();
        let facets = &self.facets;

        // Colour labels and edited state can change while culling, so they're
        // counted fresh; the loop is integer-only and cheap.
        let mut by_label = [0usize; 6];
        let mut by_stars = [0usize; 6];
        let counts = self.catalog.counts();
        let current_rating = self.catalog.rating_filter();
        let current_flag = self.catalog.filter();
        let (mut edited, mut untouched) = (0usize, 0usize);
        let mut keywords: HashMap<String, (String, usize)> = HashMap::new();
        for asset in &self.catalog.assets {
            by_label[ColorLabel::ALL
                .iter()
                .position(|l| *l == asset.color_label)
                .map_or(5, |p| p)] += 1;
            by_stars[asset.rating.value().min(5) as usize] += 1;
            if asset.develop.is_edited() {
                edited += 1;
            } else {
                untouched += 1;
            }
            for keyword in &asset.keywords {
                let key = keyword.to_lowercase();
                let entry = keywords.entry(key).or_insert_with(|| (keyword.clone(), 0));
                entry.1 += 1;
            }
        }
        let mut keyword_list: Vec<(String, usize)> = keywords.into_values().collect();
        keyword_list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

        let mut out: Vec<AnyElement> = Vec::new();

        // Header with a way back to the unfiltered library.
        out.push(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .pt_3()
                .pb_1()
                .child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.accent)
                        .child(if filters.has_facets() {
                            format!("FILTERS \u{00B7} {} ACTIVE", filters.active_count())
                        } else {
                            "FILTERS".to_string()
                        }),
                )
                .child(rustyedit_ui::chrome_button(
                    "filters-clear",
                    "Clear all",
                    false,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| {
                        this.catalog.set_filter(rustyedit_core::FilterMode::All);
                        this.catalog
                            .set_rating_filter(rustyedit_core::RatingFilter::Any);
                        this.update_filters(
                            |f| {
                                *f = FilterSet {
                                    search: f.search.clone(),
                                    ..Default::default()
                                }
                            },
                            cx,
                        );
                    }),
                ))
                .into_any_element(),
        );
        out.push(
            div()
                .pb_1()
                .child(rustyedit_ui::chrome_button(
                    "filters-save-smart",
                    "Save as smart collection…",
                    false,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.begin_smart_draft(window, cx)
                    }),
                ))
                .into_any_element(),
        );

        // Flag
        let flag_rows: Vec<AnyElement> = [
            (FilterMode::All, counts.total),
            (FilterMode::Picked, counts.picked),
            (FilterMode::Rejected, counts.rejected),
            (FilterMode::Unflagged, counts.unflagged),
        ]
        .into_iter()
        .map(|(mode, count)| {
            self.choice_row(
                format!("filter-flag-{}", mode.label()),
                mode.label(),
                count,
                current_flag == mode,
                None,
                cx,
                move |this, cx| {
                    this.catalog.set_filter(mode);
                    this.on_selection_changed(cx);
                },
            )
        })
        .collect();
        out.push(self.facet_group("FLAG", flag_rows));

        // Star rating
        let mut rating_options = vec![
            (RatingFilter::Any, counts.total),
            (RatingFilter::Unrated, by_stars[0]),
        ];
        for n in 1..=5u8 {
            rating_options.push((
                RatingFilter::AtLeast(n),
                by_stars[n as usize..].iter().sum(),
            ));
        }
        let rating_rows: Vec<AnyElement> = rating_options
            .into_iter()
            .map(|(option, count)| {
                self.choice_row(
                    format!("filter-rating-{}", option.label()),
                    option.label(),
                    count,
                    current_rating == option,
                    Some(theme.star),
                    cx,
                    move |this, cx| {
                        this.catalog.set_rating_filter(option);
                        this.on_selection_changed(cx);
                    },
                )
            })
            .collect();
        out.push(self.facet_group("RATING", rating_rows));

        // Colour label
        let mut rows = vec![self.facet_row(
            "filter-label-none",
            "No label",
            by_label[5],
            filters.label == Some(ColorLabel::None),
            None,
            cx,
            |f| toggle(&mut f.label, ColorLabel::None),
        )];
        for (i, label) in ColorLabel::ALL.into_iter().enumerate() {
            rows.push(self.facet_row(
                format!("filter-label-{}", label.label()),
                label.label(),
                by_label[i],
                filters.label == Some(label),
                theme.color_label(label),
                cx,
                move |f| toggle(&mut f.label, label),
            ));
        }
        out.push(self.facet_group("COLOUR LABEL", rows));

        if !facets.cameras.is_empty() {
            out.push(
                self.facet_group(
                    "CAMERA",
                    facets
                        .cameras
                        .iter()
                        .map(|(name, n)| {
                            let value = name.clone();
                            self.facet_row(
                                format!("filter-camera-{name}"),
                                name.clone(),
                                *n,
                                filters.camera.as_ref() == Some(name),
                                None,
                                cx,
                                move |f| toggle(&mut f.camera, value.clone()),
                            )
                        })
                        .collect(),
                ),
            );
        }
        if !facets.lenses.is_empty() {
            out.push(
                self.facet_group(
                    "LENS",
                    facets
                        .lenses
                        .iter()
                        .map(|(name, n)| {
                            let value = name.clone();
                            self.facet_row(
                                format!("filter-lens-{name}"),
                                name.clone(),
                                *n,
                                filters.lens.as_ref() == Some(name),
                                None,
                                cx,
                                move |f| toggle(&mut f.lens, value.clone()),
                            )
                        })
                        .collect(),
                ),
            );
        }
        out.push(
            self.facet_group(
                "FILE TYPE",
                facets
                    .kinds
                    .iter()
                    .map(|(kind, n)| {
                        let value = kind.clone();
                        self.facet_row(
                            format!("filter-kind-{kind}"),
                            kind.clone(),
                            *n,
                            filters.kind.as_ref() == Some(kind),
                            None,
                            cx,
                            move |f| toggle(&mut f.kind, value.clone()),
                        )
                    })
                    .collect(),
            ),
        );
        if !facets.years.is_empty() {
            out.push(
                self.facet_group(
                    "YEAR",
                    facets
                        .years
                        .iter()
                        .map(|(year, n)| {
                            let year = *year;
                            self.facet_row(
                                format!("filter-year-{year}"),
                                year.to_string(),
                                *n,
                                filters.year == Some(year),
                                None,
                                cx,
                                move |f| toggle(&mut f.year, year),
                            )
                        })
                        .collect(),
                ),
            );
        }
        out.push(
            self.facet_group(
                "ISO",
                IsoBand::ALL
                    .into_iter()
                    .zip(facets.iso)
                    .filter(|(_, n)| *n > 0)
                    .map(|(band, n)| {
                        self.facet_row(
                            format!("filter-iso-{}", band.label()),
                            band.label(),
                            n,
                            filters.iso == Some(band),
                            None,
                            cx,
                            move |f| toggle(&mut f.iso, band),
                        )
                    })
                    .collect(),
            ),
        );
        out.push(
            self.facet_group(
                "FOCAL LENGTH",
                FocalBand::ALL
                    .into_iter()
                    .zip(facets.focal)
                    .filter(|(_, n)| *n > 0)
                    .map(|(band, n)| {
                        self.facet_row(
                            format!("filter-focal-{}", band.label()),
                            band.label(),
                            n,
                            filters.focal == Some(band),
                            None,
                            cx,
                            move |f| toggle(&mut f.focal, band),
                        )
                    })
                    .collect(),
            ),
        );
        if !keyword_list.is_empty() {
            out.push(
                self.facet_group(
                    "KEYWORDS",
                    keyword_list
                        .into_iter()
                        .map(|(keyword, n)| {
                            let value = keyword.clone();
                            self.facet_row(
                                format!("filter-keyword-{keyword}"),
                                keyword,
                                n,
                                filters.keyword.as_deref() == Some(value.as_str()),
                                None,
                                cx,
                                move |f| toggle(&mut f.keyword, value.clone()),
                            )
                        })
                        .collect(),
                ),
            );
        }
        out.push(self.facet_group(
            "EDITS",
            vec![
                self.facet_row(
                    "filter-edited",
                    "Edited",
                    edited,
                    filters.edited == Some(true),
                    None,
                    cx,
                    |f| toggle(&mut f.edited, true),
                ),
                self.facet_row(
                    "filter-untouched",
                    "Not edited",
                    untouched,
                    filters.edited == Some(false),
                    None,
                    cx,
                    |f| toggle(&mut f.edited, false),
                ),
            ],
        ));
        // Side padding for the whole panel.
        vec![div()
            .flex()
            .flex_col()
            .px_3()
            .children(out)
            .into_any_element()]
    }

    fn facet_group(&self, title: &'static str, rows: Vec<AnyElement>) -> AnyElement {
        let theme = self.theme;
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
                    .px_2()
                    .py_1()
                    .bg(theme.surface_elevated)
                    .text_size(px(10.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.text_muted)
                    .child(title),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p_1()
                    .bg(theme.background.opacity(0.45))
                    .children(rows),
            )
            .into_any_element()
    }

    /// One clickable row: optional colour swatch, label, count, and a ✓ when
    /// active. Clicking applies `change` to the filters.
    fn facet_row(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        count: usize,
        active: bool,
        swatch: Option<gpui::Hsla>,
        cx: &mut Context<Self>,
        change: impl Fn(&mut FilterSet) + 'static,
    ) -> AnyElement {
        self.choice_row(id, label, count, active, swatch, cx, move |this, cx| {
            this.update_filters(|f| change(f), cx)
        })
    }

    /// [`facet_row`](Self::facet_row) with an arbitrary click action, for
    /// filters that don't live in the `FilterSet` (flag, rating).
    fn choice_row(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        count: usize,
        active: bool,
        swatch: Option<gpui::Hsla>,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let theme = self.theme;
        div()
            .id(id.into())
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(24.0))
            .px_2()
            .cursor_pointer()
            .text_size(px(11.5))
            .when(active, |el| {
                el.bg(theme.accent_muted).text_color(theme.text)
            })
            .when(!active, |el| {
                el.text_color(theme.text_muted)
                    .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
            })
            .when_some(swatch, |el, color| {
                el.child(div().size(px(9.0)).rounded_full().bg(color))
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .child(label.into()),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme.text_faint)
                    .child(count.to_string()),
            )
            .child(
                div()
                    .w(px(10.0))
                    .text_xs()
                    .child(if active { "\u{2713}" } else { "" }),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| on_click(this, cx)))
            .into_any_element()
    }
}

/// Selects `value`, or clears the facet if it's already the selection.
fn toggle<T: PartialEq>(slot: &mut Option<T>, value: T) {
    *slot = if slot.as_ref() == Some(&value) {
        None
    } else {
        Some(value)
    };
}
