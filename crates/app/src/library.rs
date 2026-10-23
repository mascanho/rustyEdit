use rustyedit_ui::sp;
use gpui::{
    div, img, prelude::*, px, relative, Animation, AnimationExt, AnyElement, App, ClickEvent,
    Context, ObjectFit, SharedString, Window,
};
use rustyedit_core::{
    settings::{DockSide, PanelKind},
    ColorLabel, Flag,
};
use rustyedit_ui::{PanelSide, Theme};
use std::{sync::atomic::Ordering, time::Duration};

use crate::{
    actions::{FlagPick, FlagReject},
    workspace::{LibraryView, Workspace},
};

const TOOLBAR_HEIGHT: f32 = 32.0;

impl Workspace {
    pub(crate) fn render_library(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.prepare_file_tree();
        // `overflow_hidden` + `min_w(0)` keep the center column from ever growing
        // past the space between the docks and painting underneath them.
        let center = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(sp(0.0))
            .overflow_hidden();
        let arrangement = self.settings.layout.preset.arrangement();
        let center = if self.is_importing {
            center.child(self.render_import_loader())
        } else if self.catalog.assets.is_empty() {
            center.child(self.render_empty_library(cx))
        } else {
            center
                .child(self.render_library_toolbar(cx))
                .child(
                    // The view and anything floating over it (the search chip).
                    div()
                        .relative()
                        .flex()
                        .flex_1()
                        .min_h(sp(0.0))
                        .min_w(sp(0.0))
                        .child(match self.library_view {
                            LibraryView::Loupe => self.render_loupe(window, cx),
                            LibraryView::Grid => self.render_grid(window, cx),
                            LibraryView::Compare => self.render_compare(cx),
                            LibraryView::Survey => self.render_survey(cx),
                        })
                        .children(self.render_search_chip(cx))
                        .children(self.render_rename_chip(cx)),
                )
                .child(self.render_cull_bar(cx))
        };

        let left = self.render_dock(DockSide::Left, arrangement.left, cx);
        let right = self.render_dock(DockSide::Right, arrangement.right, cx);

        let main = div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h(sp(0.0))
            .children(self.dock_with_rail(DockSide::Left, left, cx))
            .child(center)
            .children(self.dock_with_rail(DockSide::Right, right, cx));

        self.with_filmstrip(main.into_any_element(), cx)
    }

    /// Full-area loader shown while a folder is being scanned and its
    /// thumbnails generated: pulsing dots, the current phase, and (once
    /// thumbnails start) a progress bar with a running count.
    fn render_import_loader(&self) -> AnyElement {
        const DOTS: usize = 5;
        let theme = self.theme;
        let progress = self
            .import_progress
            .as_ref()
            .map(|(done, total)| (done.load(Ordering::Relaxed).min(*total), *total));

        let dots = (0..DOTS).fold(div().flex().gap_2().items_center(), |row, i| {
            row.child(
                div()
                    .id(("import-dot", i))
                    .size(sp(10.0))
                    .rounded_full()
                    .bg(theme.accent)
                    .with_animation(
                        ("import-dot-pulse", i),
                        Animation::new(Duration::from_millis(1200)).repeat(),
                        move |el, delta| {
                            // Staggered wave: each dot peaks a fifth of a
                            // cycle after the previous one.
                            let phase = (delta - i as f32 / DOTS as f32).rem_euclid(1.0);
                            let wave = (phase * std::f32::consts::TAU).sin() * 0.5 + 0.5;
                            el.opacity(0.2 + 0.8 * wave)
                        },
                    ),
            )
        });

        let (title, detail) = match progress {
            Some((done, total)) => (
                "Generating thumbnails".to_string(),
                format!("{done} of {total} photos"),
            ),
            None => (
                "Scanning folder".to_string(),
                "Looking for photos…".to_string(),
            ),
        };

        let bar = progress.map(|(done, total)| {
            let fraction = if total == 0 {
                0.0
            } else {
                done as f32 / total as f32
            };
            div()
                .w(sp(260.0))
                .h(sp(4.0))
                .rounded_full()
                .bg(theme.border)
                .child(
                    div()
                        .h_full()
                        .w(relative(fraction))
                        .rounded_full()
                        .bg(theme.accent),
                )
        });

        div()
            .id("import-loader")
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_4()
                    .child(dots)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_1()
                            .child(div().text_color(theme.text).child(title))
                            .child(div().text_sm().text_color(theme.text_muted).child(detail)),
                    )
                    .children(bar),
            )
            .into_any_element()
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
        // An active rail tool with its own content replaces the panels.
        let takeover = self.active_rail_tool(side).and_then(|tool| {
            self.rail_tool_content(tool, cx)
                .map(|content| (tool, content))
        });
        let title = match &takeover {
            Some((tool, _)) => tool.label().to_string(),
            None => title,
        };
        let presets_tab = takeover
            .as_ref()
            .is_some_and(|(tool, _)| *tool == rustyedit_core::RailTool::Presets);
        let taken_over = takeover.is_some();
        if let Some((_, content)) = takeover {
            children = content;
        }
        for panel in panels.iter().filter(|_| !taken_over) {
            if stacked {
                children.push(panel_group_heading(&self.theme, panel.label()));
            }
            children.extend(match panel {
                PanelKind::Navigator => self.navigator_sections(cx),
                PanelKind::Inspector => self.inspector_sections(cx),
            });
        }

        let footer = self.library_dock_footer(panels, cx);
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
            rustyedit_ui::collapsible_panel_pinned(
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
                {
                    // The left column always starts with a preview of the
                    // selected photo; the Presets tab adds its controls.
                    let preview = (side == DockSide::Left)
                        .then(|| self.library_navigator_preview(width));
                    let presets = presets_tab.then(|| self.presets_pinned(cx));
                    (preview.is_some() || presets.is_some()).then(|| {
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .children(preview)
                            .children(presets)
                            .into_any_element()
                    })
                },
                footer,
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

    /// Folders and catalog overview. (Filters live in the Filters rail tool.)
    pub(crate) fn navigator_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let counts = self.catalog.counts();

        let catalog = rustyedit_ui::section(
            &theme,
            "Catalog",
            vec![
                info_row(&theme, "Catalog", self.catalog_def.name.clone()),
                info_row(
                    &theme,
                    "Folders",
                    self.catalog_def.folders.len().to_string(),
                ),
                info_row(&theme, "Photos", counts.total.to_string()),
            ],
        )
        .into_any_element();

        let mut sections = vec![self.render_folder_tree(cx), catalog];
        sections.extend(self.missing_sections(cx));
        sections
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
                    [
                        Some(info_row(&theme, "File", asset.file_name())),
                        asset
                            .copy_name
                            .clone()
                            .map(|name| info_row(&theme, "Virtual copy", name)),
                        Some(info_row(&theme, "Size", dimensions)),
                        Some(info_row(&theme, "Location", location)),
                        Some(self.file_actions_row(cx)),
                    ]
                    .into_iter()
                    .flatten()
                    .collect(),
                )
                .into_any_element(),
            );
            metadata.extend(self.metadata_sections(cx));

            let entity = cx.entity();
            let flags = div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .child(rustyedit_ui::glyph_button(
                    "library-flag-pick",
                    "\u{2691}",
                    "Pick (P)",
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
                    "Reject (X)",
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
                            .size(sp(14.0))
                            .rounded_full()
                            .bg(color)
                            .border_2()
                            .border_color(if is_active { theme.text } else { color })
                            .cursor_pointer()
                            .hover(|el| el.border_color(theme.text_muted))
                            .tooltip(rustyedit_ui::tip(
                                format!("{} label", label.label()),
                                &theme,
                            ))
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
            div()
                .pt_3()
                .child(self.render_histogram_panel(cx))
                .into_any_element(),
            self.disclosure("metadata", "Metadata", metadata, cx),
        ]
    }

    // --- Center area ----------------------------------------------------------

    fn render_library_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let file_name = self
            .catalog
            .selected_asset()
            .map(|a| a.display_name())
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
                            this.set_library_view(LibraryView::Loupe, cx);
                        }),
                    ))
                    .child(rustyedit_ui::chrome_button(
                        "library-view-grid",
                        "Grid",
                        self.library_view == LibraryView::Grid,
                        &theme,
                        cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.set_library_view(LibraryView::Grid, cx);
                        }),
                    ))
                    .child(rustyedit_ui::chrome_button(
                        "library-view-compare",
                        "Compare",
                        self.library_view == LibraryView::Compare,
                        &theme,
                        cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.set_library_view(LibraryView::Compare, cx);
                        }),
                    ))
                    .child(rustyedit_ui::chrome_button(
                        "library-view-survey",
                        "Survey",
                        self.library_view == LibraryView::Survey,
                        &theme,
                        cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.set_library_view(LibraryView::Survey, cx);
                        }),
                    ))
                    .child(div().w(sp(1.0)).h(sp(16.0)).mx_1().bg(theme.border))
                    .child(rustyedit_ui::chrome_button(
                        "library-presets",
                        "Presets",
                        self.library_presets_open(),
                        &theme,
                        cx.listener(|this, _: &ClickEvent, _window, cx| {
                            this.toggle_library_presets(cx);
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
    fn render_loupe(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let zoomed = self
            .loupe_zoom
            .and_then(|scale| self.render_zoomed_loupe(scale, window, cx));
        let zoomed_in = zoomed.is_some();

        let comparison = self.ba_replacement(!zoomed_in);
        let split = if zoomed_in {
            None
        } else {
            self.loupe_split_overlay(cx)
        };
        let content: AnyElement = match self.catalog.selected_asset() {
            _ if zoomed.is_some() => zoomed.unwrap(),
            Some(_) if comparison.is_some() => comparison.unwrap(),
            Some(asset) => {
                // An edited photo shows its edits once they're rendered
                // (its own file until then).
                let source: gpui::ImageSource = match (self.source_image_for == Some(asset.id)
                    && (asset.develop.is_edited() || self.preview_override.is_some()))
                .then(|| self.live_preview.image())
                .flatten()
                {
                    Some(rendered) => rendered.into(),
                    None => self.loupe_source(asset).into(),
                };
                img(source)
                    .object_fit(ObjectFit::Contain)
                    .size_full()
                    .min_w(sp(0.0))
                    .min_h(sp(0.0))
                    .into_any_element()
            }
            None => div()
                .text_color(theme.text_muted)
                .child("No photo selected")
                .into_any_element(),
        };

        div()
            .id("library-loupe")
            // Records the loupe's area, for the footer slider's fit scale.
            .child({
                let bounds = self.loupe_bounds.clone();
                gpui::canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                    .absolute()
                    .size_full()
            })
            .on_mouse_down(
                gpui::MouseButton::Right,
                Self::right_click_listener(cx.entity(), self.catalog.selected),
            )
            .relative()
            .flex_1()
            .min_h(sp(0.0))
            .min_w(sp(0.0))
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .when(!zoomed_in, |el| el.p_4())
            .bg(self.loupe_bg())
            .child(content)
            .children(split)
            .children(self.ba_pill(true, cx))
            .when(!self.ba_active(), |el| el.child(self.ba_start_button(cx)))
            .children(self.render_info_overlay(cx))
            .on_click(cx.listener(|this, event: &ClickEvent, _window, cx| {
                if event.click_count() >= 2 {
                    this.enter_develop(cx);
                }
            }))
            .into_any_element()
    }

    /// The split comparison over the loupe photo, which is fitted inside the
    /// loupe's padded area.
    fn loupe_split_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let area = self.loupe_bounds.get();
        let (iw, ih) = self.live_preview.size()?;
        let pad = 16.0;
        let (aw, ah) = (
            (f32::from(area.size.width) - pad * 2.0).max(1.0),
            (f32::from(area.size.height) - pad * 2.0).max(1.0),
        );
        let fit = (aw / iw).min(ah / ih);
        let (w, h) = (iw * fit, ih * fit);
        let photo = gpui::Bounds::new(
            gpui::point(
                area.origin.x + gpui::px(pad + (aw - w) / 2.0),
                area.origin.y + gpui::px(pad + (ah - h) / 2.0),
            ),
            gpui::size(gpui::px(w), gpui::px(h)),
        );
        self.ba_split_overlay(photo, area, cx)
    }

    /// The Navigator at the top of the Library's left column: the selected
    /// photo, with its edits once they've been rendered (the Presets tab
    /// renders them so applying a preset shows straight away).
    fn library_navigator_preview(&self, dock_width: f32) -> AnyElement {
        let theme = self.theme;
        const MAX_HEIGHT: f32 = 170.0;
        let header = |name: String| {
            div()
                .flex()
                .flex_row()
                .justify_between()
                .items_baseline()
                .gap_2()
                .pb_1()
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(sp(10.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.text_muted)
                        .child("NAVIGATOR"),
                )
                .child(
                    div()
                        .min_w(sp(0.0))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_size(sp(10.0))
                        .text_color(theme.text_faint)
                        .child(name),
                )
        };
        let Some(asset) = self.catalog.selected_asset() else {
            return div()
                .px_1()
                .child(header(String::new()))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.text_faint)
                        .child("No photo selected"),
                )
                .into_any_element();
        };

        // The edited render when it's for this photo, else the file.
        // Unedited photos show their own file, so the preview never looks
        // different from the photo until something has been applied.
        let live = (self.source_image_for == Some(asset.id)
            && (asset.develop.is_edited() || self.preview_override.is_some()))
            .then(|| self.live_preview.image())
            .flatten();
        let (w, h) = match (&live, self.live_preview.size()) {
            (Some(_), Some(size)) => size,
            _ => (asset.width.max(1) as f32, asset.height.max(1) as f32),
        };
        let (w, h) = if asset.rotation_quarters() % 2 == 1 && live.is_none() {
            (h, w)
        } else {
            (w, h)
        };
        let max_width = (dock_width - 16.0).max(40.0);
        let mut width = max_width;
        let mut height = width * h / w.max(1.0);
        if height > MAX_HEIGHT {
            height = MAX_HEIGHT;
            width = height * w / h.max(1.0);
        }
        let source: gpui::ImageSource = match live {
            Some(image) => image.into(),
            None => self.loupe_source(asset).into(),
        };

        div()
            .flex()
            .flex_col()
            .px_1()
            .child(header(asset.file_name()))
            .child(
                div()
                    .w(px(width))
                    .h(px(height))
                    .mx_auto()
                    .overflow_hidden()
                    .bg(theme.background)
                    .child(img(source).object_fit(ObjectFit::Contain).size_full()),
            )
            .into_any_element()
    }

    /// Width a dock currently occupies (0 when the layout has none).
    pub(crate) fn dock_width(&self, side: DockSide, panels: &[PanelKind]) -> f32 {
        if panels.is_empty() {
            return 0.0;
        }
        let collapsed = match side {
            DockSide::Left => self.left_panel_collapsed,
            DockSide::Right => self.right_panel_collapsed,
        };
        // The rail collapses along with its dock.
        let rail = if collapsed {
            0.0
        } else {
            rustyedit_ui::RAIL_WIDTH
        };
        rail + self.dock_panel_width(side, panels)
    }

    fn dock_panel_width(&self, side: DockSide, panels: &[PanelKind]) -> f32 {
        let collapsed = match side {
            DockSide::Left => self.left_panel_collapsed,
            DockSide::Right => self.right_panel_collapsed,
        };
        if collapsed {
            return rustyedit_ui::COLLAPSED_PANEL_WIDTH;
        }
        let layout = &self.settings.layout;
        panels
            .iter()
            .map(|panel| match panel {
                PanelKind::Navigator => layout.navigator_width,
                PanelKind::Inspector => layout.inspector_width,
            })
            .max()
            .unwrap_or(260) as f32
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
pub(crate) fn panel_group_heading(theme: &Theme, label: &'static str) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .pt_3()
        .pb_1()
        .text_size(sp(10.0))
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(theme.accent)
        .child(label.to_uppercase())
        .child(div().flex_1().h(sp(1.0)).bg(theme.border))
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
