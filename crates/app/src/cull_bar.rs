//! The quick-cull bar under the loupe/grid: position counter and previous /
//! next on the left, flag + rating + colour label in the middle, and the
//! active filter on the right. All state lives on the catalog; this file
//! only lays the controls out and routes clicks to the existing setters.

use gpui::{div, prelude::*, AnyElement, App, ClickEvent, Context, SharedString, Window};
use rustyedit_core::{ColorLabel, Flag};

use crate::{
    selection::CullOp,
    workspace::{LibraryView, Workspace},
};

impl Workspace {
    pub(crate) fn render_cull_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let Some(asset) = self.catalog.selected_asset() else {
            return div().into_any_element();
        };
        let (flag, rating, label) = (asset.flag, asset.rating, asset.color_label);
        let marked = asset.marked_delete;
        let visible = self.catalog.visible_indices().len();
        let position = self.catalog.visible_position(self.catalog.selected);
        let selected_count = self.target_indices().len();

        // --- left: where am I -------------------------------------------------
        // The loupe is about the photo itself, so it drops the pager; arrow
        // keys still move through the set.
        let left = if self.library_view == LibraryView::Loupe {
            Vec::new()
        } else {
            vec![
                rustyedit_ui::cull_nav_button(
                    "cull-prev",
                    "\u{2039}",
                    "Previous photo (\u{2190})",
                    position.is_some_and(|p| p > 0),
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| this.step_selection(-1, cx)),
                )
                .into_any_element(),
                div()
                    .text_color(theme.text)
                    .child(match (position, selected_count) {
                        (Some(p), n) if n > 1 => {
                            format!("{n} selected \u{00B7} {} / {visible}", p + 1)
                        }
                        (Some(p), _) => format!("{} / {}", p + 1, visible),
                        (None, _) => format!("\u{2013} / {visible}"),
                    })
                    .into_any_element(),
                rustyedit_ui::cull_nav_button(
                    "cull-next",
                    "\u{203A}",
                    "Next photo (\u{2192})",
                    position.is_some_and(|p| p + 1 < visible),
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| this.step_selection(1, cx)),
                )
                .into_any_element(),
            ]
        };

        // --- centre: the decisions --------------------------------------------
        let flag_button = |id: &'static str,
                           target: Flag,
                           label: &'static str,
                           glyph: &'static str,
                           key: &'static str,
                           color| {
            // Clicking the active flag clears it, like toggling in Lightroom.
            let entity = cx.entity();
            rustyedit_ui::cull_button(
                id,
                glyph,
                label,
                format!("{label} ({key})"),
                color,
                flag == target,
                &theme,
                move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                    entity.update(cx, |this, cx| this.set_flag_value(target, cx));
                },
            )
            .into_any_element()
        };
        let flags = rustyedit_ui::cull_group(
            &theme,
            vec![
                flag_button(
                    "cull-reject",
                    Flag::Rejected,
                    "Reject",
                    "\u{2715}",
                    "X",
                    theme.reject,
                ),
                flag_button(
                    "cull-pick",
                    Flag::Picked,
                    "Pick",
                    "\u{2691}",
                    "P",
                    theme.pick,
                ),
                {
                    // Queue for deletion from disk; nothing is deleted until
                    // the user confirms.
                    let entity = cx.entity();
                    rustyedit_ui::cull_button(
                        "cull-mark-delete",
                        "\u{2326}",
                        "Delete",
                        "Mark for deletion from disk (asks before deleting)",
                        theme.danger,
                        marked,
                        &theme,
                        move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                            entity.update(cx, |this, cx| this.toggle_mark_for_deletion(cx));
                        },
                    )
                    .into_any_element()
                },
            ],
        )
        .into_any_element();

        let stars = {
            let entity = cx.entity();
            rustyedit_ui::cull_group(
                &theme,
                vec![div()
                    .px_2()
                    .child(rustyedit_ui::rating_stars(
                        "cull-rating",
                        rating,
                        &theme,
                        18.0,
                        Some(move |value: u8, _w: &mut Window, cx: &mut App| {
                            entity.update(cx, |this, cx| {
                                // Clicking the current rating clears it.
                                let current = this
                                    .catalog
                                    .selected_asset()
                                    .map(|a| a.rating.value())
                                    .unwrap_or(0);
                                this.set_rating_value(if current == value { 0 } else { value }, cx);
                            })
                        }),
                    ))
                    .into_any_element()],
            )
            .into_any_element()
        };

        let swatches = rustyedit_ui::cull_group(
            &theme,
            ColorLabel::ALL
                .iter()
                .filter_map(|&candidate| {
                    let color = theme.color_label(candidate)?;
                    let entity = cx.entity();
                    Some(
                        rustyedit_ui::label_swatch(
                            SharedString::from(format!("cull-label-{}", candidate.label())),
                            color,
                            format!("{} label", candidate.label()),
                            label == candidate,
                            &theme,
                            move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    this.set_color_label_value(candidate, cx)
                                });
                            },
                        )
                        .into_any_element(),
                    )
                })
                .collect(),
        )
        .into_any_element();

        // --- right: what am I looking at --------------------------------------
        let mut right = Vec::new();
        // Backdrop colour only matters when looking at a single photo.
        if self.library_view == LibraryView::Loupe {
            right.push(self.render_background_button(cx));
        }
        // Zoom (loupe) and card size (grid) share one dropdown.
        if matches!(self.library_view, LibraryView::Loupe | LibraryView::Grid) {
            right.push(self.render_size_button(cx));
        }
        right.push(
            rustyedit_ui::chrome_icon_button(
                "cull-auto-advance",
                "\u{23ED}",
                Some("Auto-advance after flag or rating".into()),
                self.settings.library.auto_advance,
                &theme,
                cx.listener(|this, _: &ClickEvent, _w, cx| {
                    let on = !this.settings.library.auto_advance;
                    this.update_settings(|s| s.library.auto_advance = on, cx);
                }),
            )
            .into_any_element(),
        );
        right.push(self.render_sort_button(cx));
        right.push(self.render_filter_button(cx));

        rustyedit_ui::cull_bar(&theme, left, vec![flags, stars, swatches], right).into_any_element()
    }

    fn step_selection(&mut self, delta: i32, cx: &mut Context<Self>) {
        if self.catalog.select_relative(delta).is_some() {
            self.on_selection_changed(cx);
        }
    }

    fn set_flag_value(&mut self, flag: Flag, cx: &mut Context<Self>) {
        self.apply_cull(CullOp::Flag(flag), cx);
    }
}
