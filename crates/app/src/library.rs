use gpui::{
    div, img, prelude::*, px, uniform_list, AnyElement, App, ClickEvent, Context, ObjectFit,
    SharedString, Window,
};
use rustyedit_core::{
    settings::{DockSide, PanelKind},
    ColorLabel, FilterMode, Flag,
};
use rustyedit_ui::{PanelSide, Theme};

use crate::{
    actions::{FlagPick, FlagReject},
    workspace::{LibraryView, Workspace},
};

/// Total height of one grid row, including the thumbnail area and the
/// flag/rating footer strip below it.
const ROW_HEIGHT: f32 = 182.0;
const THUMB_HEIGHT: f32 = 152.0;
const ROW_GAP: f32 = 10.0;
const TOOLBAR_HEIGHT: f32 = 32.0;

impl Workspace {
    pub(crate) fn render_library(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.prepare_file_tree();
        let center = div().flex().flex_col().flex_1().min_w(px(0.0));
        let center = if self.catalog.assets.is_empty() {
            center.child(self.render_empty_library(cx))
        } else {
            center
                .child(self.render_library_toolbar(cx))
                .child(match self.library_view {
                    LibraryView::Loupe => self.render_loupe(cx),
                    LibraryView::Grid => self.render_grid(cx),
                })
        };

        let arrangement = self.settings.layout.preset.arrangement();
        let left = self.render_dock(DockSide::Left, arrangement.left, cx);
        let right = self.render_dock(DockSide::Right, arrangement.right, cx);

        let main = div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h(px(0.0))
            .children(left)
            .child(center)
            .children(right);

        self.with_filmstrip(main.into_any_element(), cx)
    }

    /// One side dock, holding the panels the current layout preset assigns
    /// to it (stacked top to bottom). `None` when the dock is empty.
    fn render_dock(
        &self,
        side: DockSide,
        panels: &'static [PanelKind],
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if panels.is_empty() {
            return None;
        }
        let layout = &self.settings.layout;
        let width = panels
            .iter()
            .map(|panel| match panel {
                PanelKind::Navigator => layout.navigator_width,
                PanelKind::Inspector => layout.inspector_width,
            })
            .max()
            .unwrap_or(260) as f32;
        let title = panels
            .iter()
            .map(|panel| panel.label())
            .collect::<Vec<_>>()
            .join(" \u{00B7} ");

        // A stacked dock introduces each panel's group with a divider
        // heading so it's clear where one ends and the next begins.
        let stacked = panels.len() > 1;
        let mut children = Vec::new();
        for panel in panels {
            if stacked {
                children.push(panel_group_heading(&self.theme, panel.label()));
            }
            children.extend(match panel {
                PanelKind::Navigator => self.navigator_sections(cx),
                PanelKind::Inspector => self.inspector_sections(cx),
            });
        }

        let (id, panel_side, collapsed) = match side {
            DockSide::Left => (
                "library-dock-left",
                PanelSide::Left,
                self.left_panel_collapsed,
            ),
            DockSide::Right => (
                "library-dock-right",
                PanelSide::Right,
                self.right_panel_collapsed,
            ),
        };

        Some(
            rustyedit_ui::collapsible_panel(
                id,
                &self.theme,
                title,
                panel_side,
                width,
                collapsed,
                match side {
                    DockSide::Left => &self.left_dock_scroll,
                    DockSide::Right => &self.right_dock_scroll,
                },
                cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    match side {
                        DockSide::Left => this.left_panel_collapsed = !this.left_panel_collapsed,
                        DockSide::Right => this.right_panel_collapsed = !this.right_panel_collapsed,
                    }
                    cx.notify();
                }),
                children,
            )
            .into_any_element(),
        )
    }

    fn render_empty_library(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;

        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_color(theme.text_muted)
                            .child(if self.is_importing {
                                "Scanning folder…".to_string()
                            } else {
                                "No photos yet.".to_string()
                            }),
                    )
                    .child(rustyedit_ui::chrome_button(
                        "empty-import",
                        "Import a folder…",
                        false,
                        &theme,
                        cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.start_import(cx);
                        }),
                    )),
            )
            .into_any_element()
    }

    // --- Side panels ----------------------------------------------------------

    /// Catalog overview and filter buttons.
    fn navigator_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let counts = self.catalog.counts();

        let folder = self
            .catalog
            .folder
            .as_ref()
            .and_then(|f| f.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        let catalog = rustyedit_ui::section(
            &theme,
            "Catalog",
            vec![
                info_row(&theme, "Folder", folder),
                info_row(&theme, "Photos", counts.total.to_string()),
            ],
        )
        .into_any_element();

        let filters: Vec<AnyElement> = [
            (FilterMode::All, counts.total),
            (FilterMode::Picked, counts.picked),
            (FilterMode::Rejected, counts.rejected),
            (FilterMode::Unflagged, counts.unflagged),
        ]
        .into_iter()
        .map(|(mode, count)| {
            rustyedit_ui::chrome_button(
                SharedString::from(format!("library-filter-{}", mode.label())),
                format!("{} ({count})", mode.label()),
                self.catalog.filter() == mode,
                &theme,
                cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    this.catalog.set_filter(mode);
                    this.on_selection_changed(cx);
                }),
            )
            .into_any_element()
        })
        .collect();

        let filter = rustyedit_ui::section(
            &theme,
            "Filter",
            vec![div()
                .flex()
                .flex_col()
                .items_start()
                .gap_1()
                .children(filters)
                .into_any_element()],
        )
        .into_any_element();

        vec![self.render_folder_tree(cx), catalog, filter]
    }

    /// Histogram ("Main") and the selected photo's metadata.
    fn inspector_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let mut metadata = Vec::new();

        if let Some(asset) = self.catalog.selected_asset() {
            let dimensions = if asset.width > 0 && asset.height > 0 {
                format!("{} × {}", asset.width, asset.height)
            } else {
                "—".to_string()
            };
            let location = asset
                .path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();

            metadata.push(
                rustyedit_ui::section(
                    &theme,
                    "Info",
                    vec![
                        info_row(&theme, "File", asset.file_name()),
                        info_row(&theme, "Size", dimensions),
                        info_row(&theme, "Location", location),
                    ],
                )
                .into_any_element(),
            );

            let entity = cx.entity();
            let flags = div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .child(rustyedit_ui::glyph_button(
                    "library-flag-pick",
                    "\u{2691}",
                    theme.pick,
                    asset.flag == Flag::Picked,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.flag_pick(&FlagPick, window, cx);
                    }),
                ))
                .child(rustyedit_ui::glyph_button(
                    "library-flag-reject",
                    "\u{2715}",
                    theme.reject,
                    asset.flag == Flag::Rejected,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.flag_reject(&FlagReject, window, cx);
                    }),
                ));

            let swatches = div().flex().flex_row().items_center().gap_1().children(
                ColorLabel::ALL.into_iter().filter_map(|label| {
                    let color = theme.color_label(label)?;
                    let is_active = asset.color_label == label;
                    Some(
                        div()
                            .id(SharedString::from(format!(
                                "library-label-{}",
                                label.label()
                            )))
                            .size(px(14.0))
                            .rounded_full()
                            .bg(color)
                            .border_2()
                            .border_color(if is_active { theme.text } else { color })
                            .cursor_pointer()
                            .hover(|el| el.border_color(theme.text_muted))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                this.set_color_label_value(label, cx);
                            })),
                    )
                }),
            );

            metadata.push(
                rustyedit_ui::section(
                    &theme,
                    "Quick Cull",
                    vec![
                        labeled_row(&theme, "Flag", flags.into_any_element()),
                        labeled_row(
                            &theme,
                            "Rating",
                            rustyedit_ui::rating_stars(
                                "library-panel-rating",
                                asset.rating,
                                &theme,
                                12.0,
                                Some(move |value: u8, _window: &mut Window, cx: &mut App| {
                                    entity.update(cx, |this, cx| this.set_rating_value(value, cx));
                                }),
                            )
                            .into_any_element(),
                        ),
                        labeled_row(&theme, "Label", swatches.into_any_element()),
                    ],
                )
                .into_any_element(),
            );
        }

        if metadata.is_empty() {
            metadata.push(
                div()
                    .text_xs()
                    .text_color(theme.text_faint)
                    .child("No photo selected")
                    .into_any_element(),
            );
        }

        vec![
            self.render_main_section(cx),
            self.disclosure("metadata", "Metadata", metadata, cx),
        ]
    }

    // --- Center area ----------------------------------------------------------

    fn render_library_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let file_name = self
            .catalog
            .selected_asset()
            .map(|a| a.file_name())
            .unwrap_or_default();

        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .flex_shrink_0()
            .h(px(TOOLBAR_HEIGHT))
            .px_3()
            .border_b_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .child(rustyedit_ui::chrome_button(
                        "library-view-loupe",
                        "Loupe",
                        self.library_view == LibraryView::Loupe,
                        &theme,
                        cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.library_view = LibraryView::Loupe;
                            cx.notify();
                        }),
                    ))
                    .child(rustyedit_ui::chrome_button(
                        "library-view-grid",
                        "Grid",
                        self.library_view == LibraryView::Grid,
                        &theme,
                        cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.library_view = LibraryView::Grid;
                            this.on_selection_changed(cx);
                        }),
                    )),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_muted)
                    .child(file_name),
            )
            .into_any_element()
    }

    /// Shows the selected photo as large as the center area allows. Uses
    /// the original file rather than the (small) grid thumbnail so the
    /// showcase stays sharp.
    fn render_loupe(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;

        let content: AnyElement = match self.catalog.selected_asset() {
            Some(asset) => img(asset.path.clone())
                .object_fit(ObjectFit::Contain)
                .size_full()
                .into_any_element(),
            None => div()
                .text_color(theme.text_muted)
                .child("No photo selected")
                .into_any_element(),
        };

        div()
            .id("library-loupe")
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .bg(theme.background)
            .child(content)
            .on_click(cx.listener(|this, event: &ClickEvent, _window, cx| {
                if event.click_count() >= 2 {
                    this.enter_develop(cx);
                }
            }))
            .into_any_element()
    }

    fn render_grid(&self, cx: &mut Context<Self>) -> AnyElement {
        let columns = self.grid_columns();
        let visible_count = self.catalog.visible_indices().len();
        let row_count = visible_count.div_ceil(columns);

        div()
            .id("library-body")
            .flex_1()
            .min_h(px(0.0))
            .child(
                uniform_list(
                    "library-grid",
                    row_count,
                    cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                        let theme = this.theme;
                        let visible = this.catalog.visible_indices();
                        let selected = this.catalog.selected;
                        let entity = cx.entity();

                        range
                            .map(|row| {
                                let start = row * columns;
                                let end = (start + columns).min(visible.len());

                                let cells = (start..end)
                                    .map(|i| {
                                        let asset_ix = visible[i];
                                        let asset = &this.catalog.assets[asset_ix];
                                        let is_selected = asset_ix == selected;
                                        let entity = entity.clone();

                                        let thumb: AnyElement = match asset.thumbnail_path.as_ref()
                                        {
                                            Some(path) => img(path.clone())
                                                .object_fit(ObjectFit::Contain)
                                                .size_full()
                                                .into_any_element(),
                                            None => div()
                                                .size_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_color(theme.text_faint)
                                                .text_xs()
                                                .child("…")
                                                .into_any_element(),
                                        };

                                        div()
                                            .id(("grid-cell", asset_ix))
                                            .flex_1()
                                            .h(px(ROW_HEIGHT))
                                            .flex()
                                            .flex_col()
                                            .rounded_md()
                                            .overflow_hidden()
                                            .bg(theme.surface)
                                            .border_2()
                                            .border_color(if is_selected {
                                                theme.accent
                                            } else {
                                                theme.surface
                                            })
                                            .cursor_pointer()
                                            .child(
                                                div()
                                                    .h(px(THUMB_HEIGHT))
                                                    .w_full()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .bg(theme.background)
                                                    .child(thumb),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .justify_between()
                                                    .px_2()
                                                    .flex_1()
                                                    .child(rustyedit_ui::flag_badge(
                                                        asset.flag, &theme,
                                                    ))
                                                    .child(rustyedit_ui::rating_stars(
                                                        format!("grid-rating-{asset_ix}"),
                                                        asset.rating,
                                                        &theme,
                                                        9.0,
                                                        None::<fn(u8, &mut Window, &mut gpui::App)>,
                                                    )),
                                            )
                                            .on_click(move |event: &ClickEvent, _window, cx| {
                                                entity.update(cx, |this, cx| {
                                                    this.select_asset(asset_ix, cx);
                                                    if event.click_count() >= 2 {
                                                        this.enter_develop(cx);
                                                    }
                                                });
                                            })
                                            .into_any_element()
                                    })
                                    .collect::<Vec<_>>();

                                div()
                                    .flex()
                                    .flex_row()
                                    .gap(px(ROW_GAP))
                                    .px_3()
                                    .pb(px(ROW_GAP))
                                    .children(cells)
                                    .into_any_element()
                            })
                            .collect()
                    }),
                )
                .track_scroll(self.grid_scroll.clone())
                .pt_3()
                .h_full(),
            )
            .into_any_element()
    }
}

/// A muted label above a value, for read-only facts in the side panels.
fn info_row(theme: &Theme, label: &'static str, value: impl Into<SharedString>) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .child(div().text_xs().text_color(theme.text_faint).child(label))
        .child(div().text_sm().text_color(theme.text).child(value.into()))
        .into_any_element()
}

/// Divider heading that separates panels stacked in one dock.
fn panel_group_heading(theme: &Theme, label: &'static str) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .pt_3()
        .pb_1()
        .text_size(px(10.0))
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(theme.accent)
        .child(label.to_uppercase())
        .child(div().flex_1().h(px(1.0)).bg(theme.border))
        .into_any_element()
}

/// A label on the left with an interactive control on the right.
fn labeled_row(theme: &Theme, label: &'static str, control: AnyElement) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .child(div().text_xs().text_color(theme.text_muted).child(label))
        .child(control)
        .into_any_element()
}
