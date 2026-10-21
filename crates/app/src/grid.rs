//! The Library's Grid view: a virtualized list of rows of photo cards.
//!
//! What a card shows around its thumbnail depends on the Grid settings'
//! card style, from Detailed (rating strip, name and capture details) down
//! to Bare (only the photo). All the styles share one geometry so the
//! uniform row height and keyboard row-stepping stay exact.

use gpui::{
    div, img, prelude::*, px, uniform_list, Animation, AnimationExt, AnyElement, App, ClickEvent,
    Context, Entity, ObjectFit, Window,
};
use rustyedit_core::{
    settings::{DockSide, GridSettings, GridStyle},
    Asset, Flag,
};
use rustyedit_ui::Theme;
use std::time::Duration;

use crate::{
    actions::CycleGridStyle,
    selection::CullOp,
    workspace::{LibraryView, Mode, Workspace},
};

/// Height of the rating strip above the thumbnail (Detailed, Compact).
const HEADER_HEIGHT: f32 = 24.0;
/// Footer height with just the name, and with the details line under it.
const FOOTER_NAME_HEIGHT: f32 = 24.0;
const FOOTER_DETAILS_HEIGHT: f32 = 38.0;
/// Card borders (2px each side) that sit outside the thumbnail + strips.
const CARD_BORDER: f32 = 4.0;
/// Horizontal padding of a grid row (`px_3`) on each side.
const ROW_PAD_X: f32 = 12.0;
/// Hover group of a grid card, which reveals its floating controls.
const GRID_CARD_GROUP: &str = "grid-card";

/// Sizes shared by every card in the current layout.
#[derive(Clone, Copy)]
struct CardGeometry {
    width: f32,
    thumb_height: f32,
    header: f32,
    footer: f32,
}

impl CardGeometry {
    fn row_height(&self) -> f32 {
        self.thumb_height + self.header + self.footer + CARD_BORDER
    }
}

impl Workspace {
    pub fn cycle_grid_style(
        &mut self,
        _: &CycleGridStyle,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // J shows clipping in Develop, as in Lightroom.
        if self.mode == Mode::Develop {
            self.toggle_clipping_overlay(cx);
            return;
        }
        if self.mode != Mode::Library || self.library_view != LibraryView::Grid {
            return;
        }
        let next = self.settings.grid.style.next();
        self.update_settings(|s| s.grid.style = next, cx);
        self.status_message = Some(format!("Grid: {}", next.label()).into());
        // Card heights changed; keep the selected photo in view.
        self.on_selection_changed(cx);
    }

    pub(crate) fn render_grid(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let grid = &self.settings.grid;
        let gap = grid.spacing.gap();

        // The chosen card size is a target: the column count is however many
        // of that size fit between the docks.
        let arrangement = self.settings.layout.preset.arrangement();
        let center_width = f32::from(window.viewport_size().width)
            - self.dock_width(DockSide::Left, arrangement.left)
            - self.dock_width(DockSide::Right, arrangement.right);
        let target = (self.settings.library.grid_card_size as f32).max(48.0);
        let usable = center_width - 2.0 * ROW_PAD_X;
        let columns = (((usable + gap) / (target + gap)).floor() as usize).max(1);
        // Spread the leftover pixels across the columns so the grid fills
        // the width evenly. The width depends on the column count, never on
        // how many photos the last row holds, so a lone card keeps its size.
        let width = ((usable - gap * (columns as f32 - 1.0)) / columns as f32)
            .floor()
            .max(48.0);
        self.grid_columns_cache.set(columns);

        let (header, footer) = match grid.style {
            GridStyle::Detailed if grid.show_details => (HEADER_HEIGHT, FOOTER_DETAILS_HEIGHT),
            GridStyle::Detailed => (HEADER_HEIGHT, FOOTER_NAME_HEIGHT),
            GridStyle::Compact => (HEADER_HEIGHT, 0.0),
            GridStyle::Minimal | GridStyle::Bare => (0.0, 0.0),
        };
        let geometry = CardGeometry {
            width,
            thumb_height: (width * grid.aspect.height_ratio()).floor(),
            header,
            footer,
        };
        let row_count = self.catalog.visible_indices().len().div_ceil(columns);

        div()
            .id("library-body")
            .flex_1()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .overflow_hidden()
            .child(
                uniform_list(
                    "library-grid",
                    row_count,
                    cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                        let entity = cx.entity();
                        let visible = this.catalog.visible_indices();
                        range
                            .map(|row| {
                                let start = row * columns;
                                let end = (start + columns).min(visible.len());
                                div()
                                    .flex()
                                    .flex_row()
                                    .w_full()
                                    .gap(px(gap))
                                    .px_3()
                                    .pb(px(gap))
                                    .children((start..end).map(|i| {
                                        this.render_grid_card(visible[i], geometry, &entity)
                                    }))
                                    .into_any_element()
                            })
                            .collect()
                    }),
                )
                .track_scroll(self.grid_scroll.clone())
                .pt_3()
                .size_full(),
            )
            .into_any_element()
    }

    fn render_grid_card(
        &self,
        asset_ix: usize,
        geometry: CardGeometry,
        entity: &Entity<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let grid = &self.settings.grid;
        let style = grid.style;
        let asset = &self.catalog.assets[asset_ix];
        let is_selected = asset_ix == self.catalog.selected;
        let in_multi = self.in_selection(asset_ix);
        let label_color = theme.color_label(asset.color_label);
        // Detailed and Compact sit on a surface tile; Minimal and Bare let
        // the photos float on the background.
        let tile = match style {
            GridStyle::Detailed | GridStyle::Compact => Some(theme.surface),
            GridStyle::Minimal => Some(theme.surface.opacity(0.5)),
            GridStyle::Bare => None,
        };

        let stack = asset.stack_id.map(|id| {
            (
                id,
                self.catalog.stack_count(id),
                self.catalog.is_stack_collapsed(id),
            )
        });

        let thumb = div()
            .relative()
            .h(px(geometry.thumb_height))
            .w_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .when(style != GridStyle::Bare, |el| el.bg(theme.background))
            .when(style == GridStyle::Bare, |el| el.rounded_sm())
            .child(thumb_image(asset, asset_ix, grid.crop_to_fill, &theme))
            .when(style == GridStyle::Minimal, |el| {
                el.children(minimal_badges(asset, &theme))
            })
            .when(grid.hover_controls, |el| {
                el.child(card_controls(asset_ix, entity, &theme))
            })
            .when_some(stack, |el, (id, count, collapsed)| {
                el.child(stack_badge(asset_ix, id, count, collapsed, entity, &theme))
            });

        let card = div()
            .id(("grid-cell", asset_ix))
            .group(GRID_CARD_GROUP)
            .relative()
            .w(px(geometry.width))
            .h(px(geometry.row_height()))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .when(style == GridStyle::Bare, |el| el.rounded_sm())
            .when(style != GridStyle::Bare, |el| el.rounded_md())
            .when_some(tile, |el, bg| el.bg(bg))
            .when(grid.dim_rejected && asset.flag == Flag::Rejected, |el| {
                el.opacity(0.45)
            })
            .border_2()
            .border_color(if is_selected {
                theme.accent
            } else if in_multi {
                theme.accent.opacity(0.55)
            } else {
                tile.unwrap_or(gpui::transparent_black())
            })
            .when(in_multi && !is_selected, |el| {
                el.bg(theme.accent_muted.opacity(0.5))
            })
            .cursor_pointer();

        card.when(geometry.header > 0.0, |el| {
            el.child(card_header(asset, asset_ix, entity, &theme))
        })
        .child(thumb)
        .when(grid.tooltips, |el| {
            el.child(info_button(asset_ix, entity, &theme))
        })
        .when(geometry.footer > 0.0, |el| {
            el.child(card_footer(asset, grid, &theme))
        })
        .when_some(label_color.filter(|_| grid.show_label_bar), |el, color| {
            el.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(3.0))
                    .bg(color),
            )
        })
        .on_mouse_down(
            gpui::MouseButton::Right,
            Self::right_click_listener(entity.clone(), asset_ix),
        )
        .on_click({
            let entity = entity.clone();
            move |event: &ClickEvent, _window, cx| {
                entity.update(cx, |this, cx| {
                    this.click_select(asset_ix, event.modifiers(), cx);
                    if event.click_count() >= 2 {
                        this.enter_develop(cx);
                    }
                });
            }
        })
        .into_any_element()
    }
}

/// The photo itself, a pulsing placeholder while its thumbnail is being
/// made, or a note when the original is gone.
fn thumb_image(asset: &Asset, asset_ix: usize, crop: bool, theme: &Theme) -> AnyElement {
    match asset.thumbnail_path.as_ref() {
        Some(path) => img(path.clone())
            .object_fit(if crop {
                ObjectFit::Cover
            } else {
                ObjectFit::Contain
            })
            .size_full()
            .into_any_element(),
        // No thumbnail will ever arrive for a missing original, so don't pulse.
        None if asset.missing => div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(theme.text_faint)
            .text_xs()
            .child("Missing")
            .into_any_element(),
        None => thumb_placeholder(asset_ix, theme),
    }
}

/// Pulsing block shown in a grid card until its thumbnail is generated.
fn thumb_placeholder(asset_ix: usize, theme: &Theme) -> AnyElement {
    div()
        .id(("grid-thumb-loading", asset_ix))
        .size_full()
        .bg(theme.surface_elevated)
        .with_animation(
            ("grid-thumb-pulse", asset_ix),
            Animation::new(Duration::from_millis(1400)).repeat(),
            |el, delta| {
                let wave = (delta * std::f32::consts::TAU).sin() * 0.5 + 0.5;
                el.opacity(0.35 + 0.45 * wave)
            },
        )
        .into_any_element()
}

/// Rating strip above the thumbnail: clickable stars on the left; keywords,
/// edits, colour label and flag on the right.
fn card_header(
    asset: &Asset,
    asset_ix: usize,
    entity: &Entity<Workspace>,
    theme: &Theme,
) -> AnyElement {
    let entity = entity.clone();
    let keyword_count = asset.keywords.len();
    let small = |color| div().text_size(px(9.0)).text_color(color);

    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_1()
        .flex_shrink_0()
        .h(px(HEADER_HEIGHT))
        .px_2()
        .child(div().flex_shrink_0().child(rustyedit_ui::rating_stars(
            format!("grid-rating-{asset_ix}"),
            asset.rating,
            theme,
            10.0,
            Some(move |value: u8, _window: &mut Window, cx: &mut App| {
                // Rate without the card click re-selecting.
                cx.stop_propagation();
                entity.update(cx, |this, cx| {
                    this.act_on_card(asset_ix, cx, |this, cx| this.set_rating_value(value, cx));
                });
            }),
        )))
        .child(
            div()
                .flex()
                .flex_row()
                .flex_shrink_0()
                .items_center()
                .gap_1()
                .when(keyword_count > 0, |el| {
                    el.child(small(theme.text_muted).child(format!("#{keyword_count}")))
                })
                .when(!asset.history.is_empty(), |el| {
                    el.child(small(theme.accent).child("\u{270E}"))
                })
                .when_some(theme.color_label(asset.color_label), |el, color| {
                    el.child(div().size(px(8.0)).rounded_full().bg(color))
                })
                .child(rustyedit_ui::flag_badge(asset.flag, theme)),
        )
        .into_any_element()
}

/// Name, and optionally the capture details, under the thumbnail.
fn card_footer(asset: &Asset, grid: &GridSettings, theme: &Theme) -> AnyElement {
    let name = if asset.title.trim().is_empty() {
        asset.file_name()
    } else {
        asset.title.clone()
    };

    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .px_2()
        .pt_1()
        .pb(px(6.0))
        .flex_1()
        .min_w(px(0.0))
        .overflow_hidden()
        .child(card_text_row(
            div()
                .h(px(14.0))
                .line_height(px(14.0))
                .text_size(px(11.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.text)
                .child(name),
        ))
        .when(grid.show_details, |el| {
            el.child(card_text_row(
                div()
                    .h(px(12.0))
                    .line_height(px(12.0))
                    .text_size(px(9.0))
                    .text_color(theme.text_faint)
                    .child(card_facts(asset)),
            ))
        })
        .into_any_element()
}

/// One line of text in a card footer, clipped at the card's edge. The row
/// never shrinks, so the footer's fixed height always fits every line.
fn card_text_row(text: gpui::Div) -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .flex_shrink_0()
        .w_full()
        .min_w(px(0.0))
        .child(
            text.flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .whitespace_nowrap(),
        )
}

/// Minimal style: a small pill over the thumbnail's top-left corner with
/// the flag, colour label and rating. Nothing when there's nothing to say.
fn minimal_badges(asset: &Asset, theme: &Theme) -> Option<AnyElement> {
    let label = theme.color_label(asset.color_label);
    let rating = asset.rating.value();
    if asset.flag == Flag::Unflagged && label.is_none() && rating == 0 {
        return None;
    }
    let flag = match asset.flag {
        Flag::Unflagged => None,
        Flag::Picked => Some(("\u{2691}", theme.pick)),
        Flag::Rejected => Some(("\u{2715}", theme.reject)),
    };
    Some(
        div()
            .absolute()
            .top(px(4.0))
            .left(px(4.0))
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .h(px(16.0))
            .px_1p5()
            .rounded_full()
            .bg(theme.background.opacity(0.8))
            .text_size(px(9.5))
            .when_some(flag, |el, (glyph, color)| {
                el.child(div().text_color(color).child(glyph))
            })
            .when_some(label, |el, color| {
                el.child(div().size(px(7.0)).rounded_full().bg(color))
            })
            .when(rating > 0, |el| {
                el.child(
                    div()
                        .text_color(theme.star)
                        .child(format!("\u{2605}{rating}")),
                )
            })
            .into_any_element(),
    )
}

/// Expand / collapse toggle for a stack, pinned to the thumbnail's
/// top-right corner.
fn stack_badge(
    asset_ix: usize,
    id: u64,
    count: usize,
    collapsed: bool,
    entity: &Entity<Workspace>,
    theme: &Theme,
) -> AnyElement {
    let entity = entity.clone();
    div()
        .id(("grid-stack", asset_ix))
        .absolute()
        .top(px(4.0))
        .right(px(4.0))
        .flex()
        .flex_row()
        .items_center()
        .gap_0p5()
        .h(px(18.0))
        .px_1p5()
        .rounded_full()
        .bg(theme.background.opacity(0.85))
        .border_1()
        .border_color(theme.border)
        .text_size(px(9.5))
        .text_color(theme.text)
        .cursor_pointer()
        .tooltip(rustyedit_ui::tip(
            if collapsed {
                format!("Expand stack of {count}")
            } else {
                format!("Collapse stack of {count}")
            },
            theme,
        ))
        .child(if collapsed { "\u{25B8}" } else { "\u{25BE}" })
        .child(format!("{count}"))
        .on_click(move |_, _window, cx| {
            cx.stop_propagation();
            entity.update(cx, |this, cx| {
                this.catalog.toggle_stack_collapsed(id);
                this.on_selection_changed(cx);
            });
        })
        .into_any_element()
}

/// Pick / reject and rotate buttons floating over a card's thumbnail,
/// shown only while the card is hovered. Each acts on the card's photo, or
/// on the whole selection when the card is part of it.
fn card_controls(asset_ix: usize, entity: &Entity<Workspace>, theme: &Theme) -> AnyElement {
    let button = |id: &'static str,
                  glyph: &'static str,
                  tip: &'static str,
                  color,
                  action: fn(&mut Workspace, &mut Context<Workspace>)| {
        let entity = entity.clone();
        rustyedit_ui::glyph_button(
            (id, asset_ix),
            glyph,
            tip,
            color,
            true,
            theme,
            move |_: &ClickEvent, _window, cx| {
                cx.stop_propagation();
                entity.update(cx, |this, cx| this.act_on_card(asset_ix, cx, action));
            },
        )
    };
    let pill = || {
        div()
            .flex()
            .flex_row()
            .items_center()
            .rounded_md()
            .bg(theme.background.opacity(0.85))
            .border_1()
            .border_color(theme.border)
    };

    div()
        .absolute()
        .left(px(4.0))
        .bottom(px(4.0))
        .flex()
        .flex_row()
        .gap_1()
        .invisible()
        .group_hover(GRID_CARD_GROUP, |style| style.visible())
        .child(
            pill()
                .child(button(
                    "grid-pick",
                    "\u{2691}",
                    "Pick (P)",
                    theme.pick,
                    |this, cx| this.apply_cull(CullOp::Flag(Flag::Picked), cx),
                ))
                .child(button(
                    "grid-reject",
                    "\u{2715}",
                    "Reject (X)",
                    theme.reject,
                    |this, cx| this.apply_cull(CullOp::Flag(Flag::Rejected), cx),
                )),
        )
        .child(
            pill()
                .child(button(
                    "grid-rotate-left",
                    "\u{21BA}",
                    "Rotate left (\u{2318}[)",
                    theme.text,
                    |this, cx| this.rotate_selected(-90.0, cx),
                ))
                .child(button(
                    "grid-rotate-right",
                    "\u{21BB}",
                    "Rotate right (\u{2318}])",
                    theme.text,
                    |this, cx| this.rotate_selected(90.0, cx),
                )),
        )
        .into_any_element()
}

/// Capture date, camera and pixel size, for the Detailed footer.
fn card_facts(asset: &Asset) -> String {
    let mut facts = Vec::new();
    if let Some(date) = grid_capture_date(asset.meta.captured) {
        facts.push(date);
    }
    if !asset.meta.camera.trim().is_empty() {
        facts.push(asset.meta.camera.trim().to_string());
    }
    if asset.width > 0 && asset.height > 0 {
        facts.push(format!("{}×{}", asset.width, asset.height));
    }
    facts.join("  ·  ")
}

/// Everything about a photo in a few lines, so even Bare cards can be
/// identified by hovering.
/// The small ⓘ in a card's top-right corner, always visible. Clicking it
/// opens the photo's metadata; nothing appears just from hovering.
fn info_button(asset_ix: usize, entity: &Entity<Workspace>, theme: &Theme) -> AnyElement {
    let entity = entity.clone();
    let theme = *theme;
    div()
        .id(("grid-info", asset_ix))
        .absolute()
        .top(px(4.0))
        .right(px(4.0))
        .flex()
        .items_center()
        .justify_center()
        .size(px(14.0))
        .rounded_full()
        .bg(theme.background.opacity(0.7))
        .text_size(px(9.0))
        .italic()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(theme.text_muted)
        .cursor_pointer()
        .hover(|el| el.bg(theme.accent_muted).text_color(theme.text))
        .tooltip(rustyedit_ui::tip("Photo info", &theme))
        .child("i")
        .on_click(move |event: &ClickEvent, _window, cx| {
            cx.stop_propagation();
            let position = event.position();
            entity.update(cx, |this, cx| this.open_card_info(asset_ix, position, cx));
        })
        .into_any_element()
}

#[allow(dead_code)]
fn card_tooltip(asset: &Asset) -> String {
    let mut lines = vec![asset.file_name()];
    if !asset.title.trim().is_empty() {
        lines.push(asset.title.clone());
    }
    if let Some(date) = grid_capture_date(asset.meta.captured) {
        lines.push(date);
    }
    if !asset.meta.camera.trim().is_empty() {
        lines.push(asset.meta.camera.clone());
    }
    if !asset.meta.lens.trim().is_empty() {
        lines.push(asset.meta.lens.clone());
    }
    if !asset.keywords.is_empty() {
        lines.push(asset.keywords.join(", "));
    }
    lines.join("\n")
}

/// Formats a POSIX timestamp as `YYYY-MM-DD` using the same civil-date
/// conversion as `PhotoMeta::year`, without pulling in a date crate.
fn grid_capture_date(captured: i64) -> Option<String> {
    if captured <= 0 {
        return None;
    }
    let z = captured.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    Some(format!("{year:04}-{month:02}-{day:02}"))
}
