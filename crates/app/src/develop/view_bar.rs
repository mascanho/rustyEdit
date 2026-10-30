//! The Before / After bar under the photo (Develop and the Library loupe): which photo this is and where it sits
//! in the filmstrip, and the Before / After views, always in the same
//! place. Whatever the photo is showing (the edit, the original, a split)
//! is named here, so it's never a guess.

use gpui::{div, prelude::*, AnyElement, ClickEvent, Context, SharedString};
use rustyedit_ui::{sp, Icon};

use crate::{
    before_after::BaMode,
    workspace::{LibraryView, Mode, Workspace},
};

impl Workspace {
    /// Whether the bar is under the photo: switched on, or a comparison is
    /// showing (so one left on is never hidden).
    pub(crate) fn compare_bar_shown(&self) -> bool {
        self.ba_screen_or_loupe() && (self.settings.layout.compare_bar || self.ba_active())
    }

    /// Develop, or the Library's loupe: the screens with the bar.
    fn ba_screen_or_loupe(&self) -> bool {
        match self.mode {
            Mode::Develop => true,
            Mode::Library => self.library_view == LibraryView::Loupe,
            Mode::Export => false,
        }
    }

    /// The rail button that shows or hides the bar. Hiding it also ends a
    /// comparison, so nothing is left on out of sight.
    pub(crate) fn compare_bar_rail_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let on = self.settings.layout.compare_bar || self.ba_active();
        let comparing = self.ba_active();
        div()
            .id("rail-compare")
            .flex()
            .items_center()
            .justify_center()
            .size(gpui::px(rustyedit_ui::RAIL_BUTTON_SIZE))
            .rounded_md()
            .cursor_pointer()
            .when(on, |el| el.bg(theme.accent_muted))
            .when(!on, |el| el.hover(|el| el.bg(theme.surface_elevated)))
            .child(rustyedit_ui::icon(
                Icon::BeforeAfter,
                16.0,
                if comparing {
                    theme.accent
                } else if on {
                    theme.text
                } else {
                    theme.text_muted
                },
            ))
            .tooltip(rustyedit_ui::tip(
                match (on, self.mode) {
                    (true, _) => "Hide the Before / After bar",
                    (false, Mode::Library) => "Before / After bar (in the loupe)",
                    _ => "Show the Before / After bar",
                },
                &theme,
            ))
            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                let show = !(this.settings.layout.compare_bar || this.ba_active());
                if !show && this.ba.mode != BaMode::Off {
                    this.set_ba_mode(BaMode::Off, cx);
                }
                this.update_settings(|s| s.layout.compare_bar = show, cx);
                cx.notify();
            }))
            .into_any_element()
    }

    pub(crate) fn render_develop_view_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let catalog = &self.catalog;
        let selected = catalog.selected;
        let name = catalog
            .selected_asset()
            .and_then(|a| a.path.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let position = catalog.visible_position(selected).map(|p| {
            format!("{} of {}", p + 1, catalog.visible_indices().len())
        });
        let can_compare = self.ba_can_compare();
        let active = self.ba_active();
        let mode = if active { self.ba.mode } else { BaMode::Off };
        let jpeg = self.ba_reference().is_some();

        // Where you are: the photo and its place in the filmstrip.
        let place = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .flex_1()
            .min_w(sp(0.0))
            .child(
                div()
                    .min_w(sp(0.0))
                    .truncate()
                    .text_size(sp(11.5))
                    .text_color(theme.text)
                    .child(name),
            )
            .children(position.map(|p| {
                div()
                    .flex_shrink_0()
                    .text_size(sp(10.5))
                    .text_color(theme.text_faint)
                    .child(p)
            }));

        // What the photo is showing, in words; the accent while it isn't
        // the edit, so a comparison left on is hard to miss.
        let (showing, alert) = match (mode, jpeg) {
            (BaMode::Off, _) if !can_compare => ("No edits yet", false),
            (BaMode::Off, _) => ("Showing your edit", false),
            (BaMode::Split, false) => ("Before | After: drag the divider", true),
            (BaMode::SideBySide, false) => ("Before and after, side by side", true),
            (BaMode::Before, false) => ("Showing the original: edits hidden", true),
            (BaMode::Before, true) => ("Showing the camera's JPEG", true),
            (_, true) => ("Comparing with the camera's JPEG", true),
        };

        let segment = |option: BaMode, label: &'static str| {
            let current = mode == option;
            div()
                .id(SharedString::from(format!("view-bar-{}", option.label())))
                .flex()
                .items_center()
                .h(sp(22.0))
                .px_2p5()
                .rounded_sm()
                .text_size(sp(10.5))
                .when(current, |el| {
                    el.bg(if option == BaMode::Off {
                        theme.surface_elevated
                    } else {
                        theme.accent_muted
                    })
                    .text_color(theme.text)
                })
                .when(!current && can_compare, |el| {
                    el.cursor_pointer()
                        .text_color(theme.text_muted)
                        .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                })
                .when(!can_compare && !current, |el| {
                    el.text_color(theme.text_faint.opacity(0.6))
                })
                .tooltip(rustyedit_ui::tip(
                    if can_compare {
                        option.description()
                    } else {
                        "Edit the photo first: there's nothing to compare yet"
                    },
                    &theme,
                ))
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.set_ba_mode(option, cx)
                }))
        };

        let compare = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .flex_shrink_0()
            .child(rustyedit_ui::icon(
                Icon::BeforeAfter,
                14.0,
                if alert { theme.accent } else { theme.text_faint },
            ))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_0p5()
                    .p_0p5()
                    .rounded_md()
                    .bg(theme.background)
                    .border_1()
                    .border_color(if alert { theme.accent.opacity(0.6) } else { theme.border_subtle })
                    .child(segment(BaMode::Off, "Edited"))
                    .child(segment(BaMode::Split, "Split"))
                    .child(segment(BaMode::SideBySide, "Side by side"))
                    .child(segment(BaMode::Before, "Before")),
            )
            // A RAW shot with a JPEG: compare with the camera's own version.
            .when(self.companion_jpeg(selected).is_some(), |el| {
                el.child(
                    div()
                        .id("view-bar-jpeg")
                        .flex()
                        .items_center()
                        .h(sp(22.0))
                        .px_2()
                        .rounded_sm()
                        .cursor_pointer()
                        .border_1()
                        .text_size(sp(10.5))
                        .when(jpeg, |el| {
                            el.bg(theme.accent_muted)
                                .border_color(theme.accent)
                                .text_color(theme.text)
                        })
                        .when(!jpeg, |el| {
                            el.border_color(theme.border_subtle)
                                .text_color(theme.text_muted)
                                .hover(|el| el.text_color(theme.text))
                        })
                        .tooltip(rustyedit_ui::tip(
                            "Compare with the JPEG the camera saved alongside this RAW",
                            &theme,
                        ))
                        .child("vs camera JPEG")
                        .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                            this.compare_with_jpeg(cx)
                        })),
                )
            });

        let status = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .gap_2()
            .flex_1()
            .min_w(sp(0.0))
            .child(
                div()
                    .min_w(sp(0.0))
                    .truncate()
                    .text_size(sp(10.5))
                    .text_color(if alert { theme.accent } else { theme.text_faint })
                    .child(showing),
            )
            .when(can_compare, |el| {
                el.child(
                    div()
                        .flex_shrink_0()
                        .px_1()
                        .rounded_sm()
                        .border_1()
                        .border_color(theme.border_subtle)
                        .text_size(sp(9.5))
                        .text_color(theme.text_faint)
                        .child("\\"),
                )
            });

        div()
            .id("develop-view-bar")
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .flex_shrink_0()
            .h(sp(36.0))
            .px_3()
            .bg(theme.surface)
            .border_t_1()
            .border_color(theme.border_subtle)
            .child(place)
            .child(compare)
            .child(status)
            .into_any_element()
    }
}
