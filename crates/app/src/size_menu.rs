//! Dropdowns opened from icon buttons in the bottom bar: size (loupe zoom
//! or grid card size) and the loupe's background. Each is a menu of
//! icon + label rows floating just above the bar.

use rustyedit_ui::sp;
use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context, SharedString, Window};
use rustyedit_core::{
    settings::{LoupeBackground, GRID_CARD_MAX, GRID_CARD_MIN, GRID_CARD_STEP},
    FilterMode, RatingFilter, SortKey, SortOrder,
};
use rustyedit_ui::{CULL_BAR_HEIGHT, STATUS_BAR_HEIGHT};

use crate::workspace::{LibraryView, Workspace};

fn filter_glyph(mode: FilterMode) -> &'static str {
    match mode {
        FilterMode::All => "\u{25A6}",
        FilterMode::Picked => "\u{2691}",
        FilterMode::Rejected => "\u{2715}",
        FilterMode::Unflagged => "\u{25CB}",
    }
}

const MENU_WIDTH: f32 = 188.0;

/// Which bottom-bar dropdown is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BottomMenu {
    Size,
    Background,
    Filter,
    Sort,
}

/// Card-size presets for the grid, smallest to largest. The glyphs fill in
/// as the size grows.
const GRID_PRESETS: [(&str, &str, u32); 4] = [
    ("\u{25AB}", "Small", 120),
    ("\u{25AA}", "Medium", 180),
    ("\u{25FC}", "Large", 260),
    ("\u{25A0}", "Extra large", 360),
];

impl Workspace {
    /// The icon button that opens the menu.
    pub(crate) fn render_size_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let tooltip = match self.library_view {
            LibraryView::Grid => "Thumbnail size",
            _ => "Zoom",
        };
        rustyedit_ui::chrome_icon_button(
            "size-menu-button",
            "\u{25A3}",
            Some(tooltip.into()),
            self.bottom_menu == Some(BottomMenu::Size),
            &self.theme,
            cx.listener(|this, _: &ClickEvent, _w, cx| {
                this.toggle_bottom_menu(BottomMenu::Size);
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn toggle_bottom_menu(&mut self, menu: BottomMenu) {
        self.bottom_menu = if self.bottom_menu == Some(menu) {
            None
        } else {
            Some(menu)
        };
    }

    /// The loupe's backdrop colour.
    pub(crate) fn loupe_bg(&self) -> gpui::Hsla {
        match self.settings.library.loupe_background.hsl() {
            Some((h, s, l)) => gpui::hsla(h, s, l, 1.0),
            None => self.theme.background,
        }
    }

    /// The Develop viewer's backdrop colour.
    pub(crate) fn viewer_bg(&self) -> gpui::Hsla {
        match self.settings.develop.viewer_background.hsl() {
            Some((h, s, l)) => gpui::hsla(h, s, l, 1.0),
            None => self.theme.background,
        }
    }

    pub fn cycle_background(
        &mut self,
        _: &crate::actions::CycleBackground,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode == crate::workspace::Mode::Library && self.library_view == LibraryView::Loupe {
            let next = self.settings.library.loupe_background.next();
            self.update_settings(|s| s.library.loupe_background = next, cx);
        } else if self.mode == crate::workspace::Mode::Develop {
            let next = self.settings.develop.viewer_background.next();
            self.update_settings(|s| s.develop.viewer_background = next, cx);
        }
    }

    pub(crate) fn render_background_button(&self, cx: &mut Context<Self>) -> AnyElement {
        rustyedit_ui::chrome_icon_button(
            "background-menu-button",
            "\u{25D0}",
            Some("Background (B)".into()),
            self.bottom_menu == Some(BottomMenu::Background),
            &self.theme,
            cx.listener(|this, _: &ClickEvent, _w, cx| {
                this.toggle_bottom_menu(BottomMenu::Background);
                cx.notify();
            }),
        )
        .into_any_element()
    }

    /// The open dropdown, or nothing.
    pub(crate) fn render_bottom_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let rows = match self.bottom_menu {
            Some(BottomMenu::Size) => self.size_rows(cx),
            Some(BottomMenu::Background) => self.background_rows(cx),
            Some(BottomMenu::Filter) => self.filter_rows(cx),
            Some(BottomMenu::Sort) => self.sort_rows(cx),
            None => return gpui::div().into_any_element(),
        };
        self.menu_shell(rows, cx)
    }

    /// The filter button: shows the active filter, and is highlighted
    /// whenever the library is narrowed.
    pub(crate) fn render_filter_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let filter = self.catalog.filter();
        let rating = self.catalog.rating_filter();
        let mut label = format!("{} {}", filter_glyph(filter), filter.label());
        if rating != RatingFilter::Any {
            label.push_str(&format!(" \u{00B7} {}", rating.label()));
        }
        let facets = self.catalog.filters().active_count()
            - usize::from(!self.catalog.filters().search.trim().is_empty());
        if facets > 0 {
            label.push_str(&format!(" \u{00B7} +{facets}"));
        }
        rustyedit_ui::chrome_icon_button(
            "filter-menu-button",
            format!("{label} \u{25BE}"),
            Some("Filter photos (F cycles flags)".into()),
            self.bottom_menu == Some(BottomMenu::Filter)
                || filter != FilterMode::All
                || rating != RatingFilter::Any
                || self.catalog.filters().has_facets(),
            &self.theme,
            cx.listener(|this, _: &ClickEvent, _w, cx| {
                this.toggle_bottom_menu(BottomMenu::Filter);
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn filter_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let counts = self.catalog.counts();
        let current = self.catalog.filter();
        let mut rows: Vec<AnyElement> = [
            (FilterMode::All, counts.total),
            (FilterMode::Picked, counts.picked),
            (FilterMode::Rejected, counts.rejected),
            (FilterMode::Unflagged, counts.unflagged),
        ]
        .into_iter()
        .map(|(mode, count)| {
            let active = mode == current;
            let color = match mode {
                FilterMode::Picked => theme.pick,
                FilterMode::Rejected => theme.reject,
                _ => theme.text_muted,
            };
            div()
                .id(SharedString::from(format!("filter-{}", mode.label())))
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .h(sp(28.0))
                .px_2()
                .rounded_md()
                .cursor_pointer()
                .text_sm()
                .when(active, |el| {
                    el.bg(theme.accent_muted).text_color(theme.text)
                })
                .when(!active, |el| {
                    el.text_color(theme.text_muted)
                        .hover(|el| el.bg(theme.surface).text_color(theme.text))
                })
                .child(
                    div()
                        .w(sp(16.0))
                        .text_center()
                        .text_color(color)
                        .child(filter_glyph(mode)),
                )
                .child(div().flex_1().child(mode.label()))
                .child(
                    div()
                        .text_size(sp(10.0))
                        .text_color(theme.text_faint)
                        .child(count.to_string()),
                )
                .child(
                    div()
                        .w(sp(12.0))
                        .text_xs()
                        .child(if active { "\u{2713}" } else { "" }),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.bottom_menu = None;
                    this.catalog.set_filter(mode);
                    // Filtering can move the selection; take the usual path.
                    this.on_selection_changed(cx);
                }))
                .into_any_element()
        })
        .collect();

        // --- by star rating -----------------------------------------------
        let mut by_stars = [0usize; 6];
        for asset in &self.catalog.assets {
            by_stars[asset.rating.value() as usize] += 1;
        }
        let current_rating = self.catalog.rating_filter();
        rows.push(
            div()
                .px_2()
                .pt_2()
                .pb_1()
                .text_size(sp(10.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(theme.text_faint)
                .child("RATING")
                .into_any_element(),
        );
        let mut options = vec![
            (RatingFilter::Any, counts.total),
            (RatingFilter::Unrated, by_stars[0]),
        ];
        for n in 1..=5u8 {
            options.push((
                RatingFilter::AtLeast(n),
                by_stars[n as usize..].iter().sum(),
            ));
        }
        for (option, count) in options {
            let active = option == current_rating;
            rows.push(
                div()
                    .id(SharedString::from(format!(
                        "rating-filter-{}",
                        option.label()
                    )))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .h(sp(28.0))
                    .px_2()
                    .rounded_md()
                    .cursor_pointer()
                    .text_sm()
                    .when(active, |el| {
                        el.bg(theme.accent_muted).text_color(theme.text)
                    })
                    .when(!active, |el| {
                        el.text_color(theme.text_muted)
                            .hover(|el| el.bg(theme.surface).text_color(theme.text))
                    })
                    .child(
                        div()
                            .w(sp(16.0))
                            .text_center()
                            .text_color(theme.star)
                            .child(if option == RatingFilter::Any {
                                "\u{2606}"
                            } else {
                                "\u{2605}"
                            }),
                    )
                    .child(div().flex_1().child(option.label()))
                    .child(
                        div()
                            .text_size(sp(10.0))
                            .text_color(theme.text_faint)
                            .child(count.to_string()),
                    )
                    .child(
                        div()
                            .w(sp(12.0))
                            .text_xs()
                            .child(if active { "\u{2713}" } else { "" }),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.bottom_menu = None;
                        this.catalog.set_rating_filter(option);
                        this.on_selection_changed(cx);
                    }))
                    .into_any_element(),
            );
        }
        rows.push(
            div()
                .id("filter-more")
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .h(sp(28.0))
                .mt_1()
                .px_2()
                .rounded_md()
                .cursor_pointer()
                .text_sm()
                .text_color(theme.accent)
                .hover(|el| el.bg(theme.surface))
                .child(div().w(sp(16.0)).text_center().child("\u{2630}"))
                .child(div().flex_1().child("More filters\u{2026}"))
                .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                    this.bottom_menu = None;
                    this.open_left_tool(rustyedit_core::RailTool::Filters, cx);
                }))
                .into_any_element(),
        );
        rows
    }

    pub(crate) fn render_sort_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let order = self.catalog.sort();
        let default = order == SortOrder::default();
        rustyedit_ui::chrome_icon_button(
            "sort-menu-button",
            format!(
                "\u{21C5}{}",
                if default {
                    String::new()
                } else {
                    format!(
                        " {} {}",
                        order.key.label(),
                        if order.descending {
                            "\u{2193}"
                        } else {
                            "\u{2191}"
                        }
                    )
                }
            ),
            Some("Sort photos".into()),
            self.bottom_menu == Some(BottomMenu::Sort) || !default,
            &self.theme,
            cx.listener(|this, _: &ClickEvent, _w, cx| {
                this.toggle_bottom_menu(BottomMenu::Sort);
                cx.notify();
            }),
        )
        .into_any_element()
    }

    pub(crate) fn apply_sort(&mut self, order: SortOrder, cx: &mut Context<Self>) {
        self.catalog.set_sort(order);
        self.update_settings(|s| s.library.sort = order, cx);
        // Keep the current photo in view in its new place.
        self.on_selection_changed(cx);
    }

    fn sort_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let current = self.catalog.sort();
        let row = |id: String, icon: &'static str, label: &'static str, active: bool| {
            div()
                .id(SharedString::from(id))
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .h(sp(28.0))
                .px_2()
                .rounded_md()
                .cursor_pointer()
                .text_sm()
                .when(active, |el| {
                    el.bg(theme.accent_muted).text_color(theme.text)
                })
                .when(!active, |el| {
                    el.text_color(theme.text_muted)
                        .hover(|el| el.bg(theme.surface).text_color(theme.text))
                })
                .child(div().w(sp(16.0)).text_center().child(icon))
                .child(div().flex_1().child(label))
                .child(
                    div()
                        .w(sp(12.0))
                        .text_xs()
                        .child(if active { "\u{2713}" } else { "" }),
                )
        };

        let mut rows: Vec<AnyElement> = SortKey::ALL
            .into_iter()
            .map(|key| {
                let icon = match key {
                    SortKey::Folder => "\u{25A4}",
                    SortKey::FileName => "Aa",
                    SortKey::CaptureDate => "\u{25F7}",
                    SortKey::Rating => "\u{2605}",
                    SortKey::Flag => "\u{2691}",
                };
                row(
                    format!("sort-{}", key.label()),
                    icon,
                    key.label(),
                    key == current.key,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.bottom_menu = None;
                    let order = SortOrder {
                        key,
                        descending: this.catalog.sort().descending,
                    };
                    this.apply_sort(order, cx);
                }))
                .into_any_element()
            })
            .collect();

        rows.push(
            div()
                .px_2()
                .pt_2()
                .pb_1()
                .text_size(sp(10.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(theme.text_faint)
                .child("ORDER")
                .into_any_element(),
        );
        for (label, icon, descending) in [
            ("Ascending", "\u{2191}", false),
            ("Descending", "\u{2193}", true),
        ] {
            rows.push(
                row(
                    format!("sort-dir-{label}"),
                    icon,
                    label,
                    current.descending == descending,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.bottom_menu = None;
                    let order = SortOrder {
                        key: this.catalog.sort().key,
                        descending,
                    };
                    this.apply_sort(order, cx);
                }))
                .into_any_element(),
            );
        }
        rows
    }

    fn background_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let current = self.settings.library.loupe_background;
        LoupeBackground::ALL
            .into_iter()
            .map(|option| {
                let active = option == current;
                let swatch = match option.hsl() {
                    Some((h, s, l)) => gpui::hsla(h, s, l, 1.0),
                    None => theme.background,
                };
                div()
                    .id(SharedString::from(format!("bg-{}", option.label())))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .h(sp(28.0))
                    .px_2()
                    .rounded_md()
                    .cursor_pointer()
                    .text_sm()
                    .when(active, |el| {
                        el.bg(theme.accent_muted).text_color(theme.text)
                    })
                    .when(!active, |el| {
                        el.text_color(theme.text_muted)
                            .hover(|el| el.bg(theme.surface).text_color(theme.text))
                    })
                    .child(
                        div()
                            .size(sp(14.0))
                            .rounded_full()
                            .bg(swatch)
                            .border_1()
                            .border_color(theme.border),
                    )
                    .child(div().flex_1().child(option.label()))
                    .child(
                        div()
                            .w(sp(12.0))
                            .text_xs()
                            .child(if active { "\u{2713}" } else { "" }),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.bottom_menu = None;
                        this.update_settings(|s| s.library.loupe_background = option, cx);
                    }))
                    .into_any_element()
            })
            .collect()
    }

    fn size_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;

        let row = |id: SharedString, icon: &'static str, label: String, active: bool| {
            div()
                .id(id)
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .h(sp(28.0))
                .px_2()
                .rounded_md()
                .cursor_pointer()
                .text_sm()
                .when(active, |el| {
                    el.bg(theme.accent_muted).text_color(theme.text)
                })
                .when(!active, |el| {
                    el.text_color(theme.text_muted)
                        .hover(|el| el.bg(theme.surface).text_color(theme.text))
                })
                .child(div().w(sp(16.0)).text_center().child(icon))
                .child(div().flex_1().child(label))
        };
        let check = |on: bool| {
            div()
                .w(sp(12.0))
                .text_xs()
                .child(if on { "\u{2713}" } else { "" })
        };

        let mut rows: Vec<AnyElement> = Vec::new();
        match self.library_view {
            LibraryView::Loupe => {
                let mut options: Vec<(&'static str, String, Option<f32>)> =
                    vec![("\u{25FB}", "Fit".into(), None)];
                for (icon, step) in ["\u{25AB}", "\u{25AA}", "\u{25A0}"]
                    .into_iter()
                    .zip(crate::compare::ZOOM_STEPS)
                {
                    options.push((icon, format!("{}%", (step * 100.0) as u32), Some(step)));
                }
                for (icon, label, zoom) in options {
                    let active = self.loupe_zoom == zoom;
                    rows.push(
                        row(
                            SharedString::from(format!("size-zoom-{label}")),
                            icon,
                            label,
                            active,
                        )
                        .child(check(active))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            this.bottom_menu = None;
                            this.set_loupe_zoom(zoom, cx);
                        }))
                        .into_any_element(),
                    );
                }
            }
            _ => {
                let current = self.settings.library.grid_card_size;
                // The preset nearest the current size is the checked one.
                let nearest = GRID_PRESETS
                    .iter()
                    .min_by_key(|(_, _, size)| size.abs_diff(current))
                    .map(|(_, _, size)| *size);
                for (icon, label, size) in GRID_PRESETS {
                    let active = nearest == Some(size) && current.abs_diff(size) < GRID_CARD_STEP;
                    rows.push(
                        row(
                            SharedString::from(format!("size-grid-{label}")),
                            icon,
                            format!("{label} \u{00B7} {size}"),
                            active,
                        )
                        .child(check(active))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            this.bottom_menu = None;
                            this.update_settings(|s| s.library.grid_card_size = size, cx);
                        }))
                        .into_any_element(),
                    );
                }
                // Fine adjustment keeps the menu open so it can be repeated.
                for (id, icon, label, delta) in [
                    (
                        "size-grid-smaller",
                        "\u{2212}",
                        "Smaller",
                        -(GRID_CARD_STEP as i32),
                    ),
                    ("size-grid-larger", "+", "Larger", GRID_CARD_STEP as i32),
                ] {
                    rows.push(
                        row(id.into(), icon, label.to_string(), false)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                let size = (this.settings.library.grid_card_size as i32 + delta)
                                    .clamp(GRID_CARD_MIN as i32, GRID_CARD_MAX as i32)
                                    as u32;
                                this.update_settings(|s| s.library.grid_card_size = size, cx);
                            }))
                            .into_any_element(),
                    );
                }
            }
        }

        rows
    }

    /// Wraps menu rows in a panel floating above the bottom bar, with a
    /// full-window backdrop that closes it on an outside click.
    fn menu_shell(&self, rows: Vec<AnyElement>, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let arrangement = self.settings.layout.preset.arrangement();
        // Sit just above the bottom bar, under the buttons, which hug the
        // right edge of the center column.
        let right_inset =
            self.dock_width(rustyedit_core::settings::DockSide::Right, arrangement.right);

        div()
            .id("bottom-menu-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_click(cx.listener(|this, _: &ClickEvent, _w: &mut Window, cx| {
                this.bottom_menu = None;
                cx.notify();
            }))
            .child(
                div()
                    .id("bottom-menu")
                    .absolute()
                    .bottom(px(STATUS_BAR_HEIGHT + CULL_BAR_HEIGHT + 4.0))
                    .right(px(right_inset + 12.0))
                    .w(px(MENU_WIDTH))
                    .occlude()
                    .p_1()
                    .flex()
                    .flex_col()
                    .rounded_xl()
                    .bg(theme.surface_elevated)
                    .border_1()
                    .border_color(theme.border)
                    .shadow_lg()
                    .children(rows),
            )
            .into_any_element()
    }
}
