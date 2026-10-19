use gpui::{
    div, hsla, linear_color_stop, linear_gradient, prelude::*, px, AnyElement, ClickEvent, Context,
    MouseButton, SharedString, Window,
};
use rustyedit_core::settings::{
    CardOption, CardPreview, FilmstripPosition, LayoutPreset, PanelKind, SettingControl,
    SettingItem, SETTINGS_PAGES,
};
use rustyedit_core::Settings;
use rustyedit_ui::{FilmLook, Theme};

use crate::{actions::CloseSettings, workspace::Workspace};

const MODAL_WIDTH: f32 = 820.0;
const MODAL_HEIGHT: f32 = 600.0;

// Card picker geometry: three cards per row fit the content column.
const CARD_WIDTH: f32 = 186.0;
const CARD_PREVIEW_HEIGHT: f32 = 104.0;
const SIDEBAR_WIDTH: f32 = 180.0;

impl Workspace {
    pub(crate) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_open = true;
        window.focus(&self.settings_focus);
        cx.notify();
    }

    pub(crate) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_open = false;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    /// Applies an edit to the settings, persists them, and pushes any
    /// change that should take effect immediately into live state.
    pub(crate) fn update_settings(
        &mut self,
        edit: impl FnOnce(&mut Settings),
        cx: &mut Context<Self>,
    ) {
        let previous = self.settings.clone();
        edit(&mut self.settings);
        if self.settings == previous {
            return;
        }
        if let Err(err) = rustyedit_io::save_settings(&self.settings) {
            self.status_message = Some(format!("Couldn't save settings: {err}").into());
        }
        if self.settings.develop.preview_quality != previous.develop.preview_quality
            && self.source_image.is_some()
        {
            self.load_source_image(cx);
        }
        cx.notify();
    }

    /// Zed-style settings window: a modal over the workspace with a page
    /// list on the left and that page's settings on the right. Entirely
    /// driven by `SETTINGS_PAGES`, so new options appear here automatically.
    pub(crate) fn render_settings_modal(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let page_ix = self
            .settings_page
            .min(SETTINGS_PAGES.len().saturating_sub(1));
        let page = &SETTINGS_PAGES[page_ix];

        let sidebar = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(SIDEBAR_WIDTH))
            .p_2()
            .gap_0p5()
            .bg(theme.background)
            .border_r_1()
            .border_color(theme.border_subtle)
            .children(SETTINGS_PAGES.iter().enumerate().map(|(ix, page)| {
                let is_active = ix == page_ix;
                div()
                    .id(("settings-page", ix))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_sm()
                    .cursor_pointer()
                    .when(is_active, |el| {
                        el.bg(theme.accent_muted).text_color(theme.text)
                    })
                    .when(!is_active, |el| {
                        el.text_color(theme.text_muted)
                            .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                    })
                    .child(page.title)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.settings_scroll
                            .set_offset(gpui::point(px(0.0), px(0.0)));
                        this.settings_page = ix;
                        cx.notify();
                    }))
            }));

        let sections = page
            .sections
            .iter()
            .enumerate()
            .map(|(section_ix, section)| {
                div()
                    .flex()
                    .flex_col()
                    .pb_4()
                    .child(
                        div()
                            .pb_1()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_faint)
                            .child(section.title.to_uppercase()),
                    )
                    .children(section.items.iter().enumerate().map(|(item_ix, item)| {
                        let id = format!("setting-{page_ix}-{section_ix}-{item_ix}");
                        self.render_setting_row(id, item, cx)
                    }))
            });

        let content = div()
            .id(("settings-content", page_ix))
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.settings_scroll)
            .px_5()
            .py_4()
            .child(
                div()
                    .pb_3()
                    .text_lg()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(page.title),
            )
            .children(sections);

        let path_hint = rustyedit_io::settings_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default();

        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .flex_shrink_0()
            .h(px(40.0))
            .px_4()
            .border_b_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Settings"),
            )
            .child(rustyedit_ui::glyph_button(
                "settings-close",
                "\u{2715}",
                theme.text_muted,
                true,
                &theme,
                cx.listener(|this, _: &ClickEvent, window, cx| this.close_settings(window, cx)),
            ));

        let footer = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .flex_shrink_0()
            .h(px(36.0))
            .px_4()
            .border_t_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .text_xs()
                    .text_color(theme.text_faint)
                    .child(path_hint),
            )
            .child(rustyedit_ui::chrome_button(
                "settings-reset",
                "Reset to defaults",
                false,
                &theme,
                cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.update_settings(|s| *s = Settings::default(), cx);
                }),
            ));

        let card = div()
            .id("settings-card")
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
            // Keep clicks inside the card from reaching the backdrop,
            // which closes the modal.
            .on_mouse_down(MouseButton::Left, |_, _window, cx| cx.stop_propagation())
            .child(header)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(sidebar)
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(content)
                            .child(rustyedit_ui::vertical_scrollbar(
                                "settings-content-scrollbar",
                                self.settings_scroll.clone(),
                                &theme,
                            )),
                    ),
            )
            .child(footer);

        div()
            .id("settings-backdrop")
            .key_context("SettingsModal")
            .track_focus(&self.settings_focus)
            .on_action(cx.listener(|this, _: &CloseSettings, window, cx| {
                this.close_settings(window, cx);
            }))
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.55))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.close_settings(window, cx)),
            )
            .child(card)
            .into_any_element()
    }

    /// One row: label and description on the left, its control on the right.
    fn render_setting_row(
        &self,
        id: String,
        item: &'static SettingItem,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let settings = &self.settings;

        let control: AnyElement = match &item.control {
            SettingControl::Toggle { get, set } => {
                let (get, set) = (*get, *set);
                let on = get(settings);
                rustyedit_ui::toggle_switch(
                    SharedString::from(id),
                    on,
                    &theme,
                    cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.update_settings(|s| set(s, !on), cx);
                    }),
                )
                .into_any_element()
            }
            SettingControl::Choice { options, get, set } => {
                let set = *set;
                let current = get(settings);
                div()
                    .flex()
                    .flex_row()
                    .flex_shrink_0()
                    .gap_0p5()
                    .p_0p5()
                    .rounded_md()
                    .bg(theme.background)
                    .children(options.iter().enumerate().map(|(ix, option)| {
                        rustyedit_ui::chrome_button(
                            SharedString::from(format!("{id}-{ix}")),
                            *option,
                            ix == current,
                            &theme,
                            cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                this.update_settings(|s| set(s, ix), cx);
                            }),
                        )
                    }))
                    .into_any_element()
            }
            SettingControl::Stepper {
                min,
                max,
                step,
                unit,
                get,
                set,
            } => {
                let (min, max, set) = (*min, *max, *set);
                let entity = cx.entity();
                rustyedit_ui::stepper(
                    id,
                    get(settings),
                    min,
                    max,
                    *step,
                    unit,
                    &theme,
                    move |value, _window, cx| {
                        entity.update(cx, |this, cx| {
                            this.update_settings(|s| set(s, value.clamp(min, max)), cx);
                        });
                    },
                )
                .into_any_element()
            }
            SettingControl::Cards { options, get, set } => {
                return self.render_card_picker(id, item, options, get(settings), *set, cx);
            }
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_4()
            .py_2()
            .border_b_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .min_w(px(0.0))
                    .child(div().text_sm().text_color(theme.text).child(item.label))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text_muted)
                            .child(item.description),
                    ),
            )
            .child(control)
            .into_any_element()
    }

    /// A row of large, clickable preview cards (wrapping onto more rows as
    /// needed) under the setting's label and description.
    fn render_card_picker(
        &self,
        id: String,
        item: &'static SettingItem,
        options: &'static [CardOption],
        current: usize,
        set: fn(&mut Settings, usize),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let current_look = self.film_look();

        let cards = options.iter().enumerate().map(|(ix, option)| {
            let selected = ix == current;
            let preview: AnyElement = match option.preview {
                CardPreview::Layout(preset) => layout_preview(
                    preset,
                    self.settings.layout.filmstrip_position,
                    &current_look,
                    &theme,
                )
                .into_any_element(),
                CardPreview::Filmstrip(style) => {
                    filmstrip_preview(&FilmLook::for_style(style, &theme)).into_any_element()
                }
            };

            div()
                .id(SharedString::from(format!("{id}-card-{ix}")))
                .flex()
                .flex_col()
                .gap_1p5()
                .w(px(CARD_WIDTH))
                .p_2()
                .rounded_lg()
                .border_2()
                .cursor_pointer()
                .when(selected, |el| {
                    el.border_color(theme.accent)
                        .bg(theme.accent_muted.opacity(0.18))
                })
                .when(!selected, |el| {
                    el.border_color(theme.border_subtle)
                        .bg(theme.background.opacity(0.4))
                        .hover(|el| el.border_color(theme.border).bg(theme.surface_elevated))
                })
                .child(
                    div()
                        .relative()
                        .child(preview)
                        .when(selected, |el| el.child(check_badge(&theme))),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .px_0p5()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(if selected {
                                    theme.text
                                } else {
                                    theme.text_muted
                                })
                                .child(option.title),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.text_faint)
                                .child(option.description),
                        ),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    this.update_settings(|s| set(s, ix), cx);
                }))
        });

        div()
            .flex()
            .flex_col()
            .gap_3()
            .py_3()
            .border_b_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(div().text_sm().text_color(theme.text).child(item.label))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text_muted)
                            .child(item.description),
                    ),
            )
            .child(div().flex().flex_row().flex_wrap().gap_3().children(cards))
            .into_any_element()
    }
}

/// Accent check mark pinned to the corner of the selected card's preview.
fn check_badge(theme: &Theme) -> impl IntoElement {
    div()
        .absolute()
        .top(px(6.0))
        .right(px(6.0))
        .size(px(18.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(theme.accent)
        .border_2()
        .border_color(theme.surface)
        .text_size(px(10.0))
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(theme.background)
        .child("\u{2713}")
}

/// A miniature wireframe of the whole window under `preset`: title bar,
/// docks with their panels, the photo, and the filmstrip on its current
/// edge — so each card shows exactly what choosing it will look like.
fn layout_preview(
    preset: LayoutPreset,
    filmstrip: FilmstripPosition,
    look: &FilmLook,
    theme: &Theme,
) -> impl IntoElement {
    let arrangement = preset.arrangement();

    let photo = div()
        .flex_1()
        .min_w(px(0.0))
        .p(px(5.0))
        .child(div().size_full().rounded_sm().bg(linear_gradient(
            180.0,
            linear_color_stop(theme.label_blue.opacity(0.55), 0.0),
            linear_color_stop(theme.film_edge_print.opacity(0.65), 1.0),
        )));

    let body = div()
        .flex()
        .flex_row()
        .flex_1()
        .min_h(px(0.0))
        .when(!arrangement.left.is_empty(), |el| {
            el.child(mini_dock(arrangement.left, true, theme))
        })
        .child(photo)
        .when(!arrangement.right.is_empty(), |el| {
            el.child(mini_dock(arrangement.right, false, theme))
        });

    let at_top = filmstrip == FilmstripPosition::Top;

    div()
        .flex()
        .flex_col()
        .w_full()
        .h(px(CARD_PREVIEW_HEIGHT))
        .rounded_md()
        .overflow_hidden()
        .bg(theme.background)
        .border_1()
        .border_color(theme.border)
        // Title bar, with the three macOS window buttons.
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(2.0))
                .flex_shrink_0()
                .h(px(8.0))
                .px(px(3.0))
                .bg(theme.surface_elevated)
                .children(
                    [theme.label_red, theme.label_yellow, theme.label_green]
                        .into_iter()
                        .map(|color| div().size(px(3.0)).rounded_full().bg(color.opacity(0.8))),
                ),
        )
        .when(at_top, |el| el.child(mini_filmstrip(look, theme)))
        .child(body)
        .when(!at_top, |el| el.child(mini_filmstrip(look, theme)))
}

/// One side dock in a layout preview, its panels stacked top to bottom.
fn mini_dock(panels: &'static [PanelKind], is_left: bool, theme: &Theme) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(px(2.0))
        .w(px(38.0))
        .p(px(2.0))
        .bg(theme.surface)
        .border_color(theme.border)
        .when(is_left, |el| el.border_r_1())
        .when(!is_left, |el| el.border_l_1())
        .children(panels.iter().map(|panel| mini_panel(*panel, theme)))
}

/// A tiny stand-in for a panel's content: the Navigator as a list of
/// rows, the Inspector as a little RGB histogram over metadata lines.
fn mini_panel(panel: PanelKind, theme: &Theme) -> impl IntoElement {
    let line = |width: f32| {
        div()
            .h(px(2.0))
            .w(px(width))
            .rounded_full()
            .bg(theme.text_faint.opacity(0.7))
    };

    let base = div()
        .flex()
        .flex_col()
        .flex_1()
        .gap(px(3.0))
        .p(px(3.0))
        .rounded_sm();

    match panel {
        PanelKind::Navigator => base
            .bg(theme.accent_muted.opacity(0.45))
            .children([24.0, 18.0, 22.0, 14.0].map(line)),
        PanelKind::Inspector => base
            .bg(theme.surface_elevated)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_end()
                    .gap(px(1.0))
                    .h(px(12.0))
                    .children(
                        [
                            (theme.label_red, 5.0),
                            (theme.label_green, 9.0),
                            (theme.label_blue, 12.0),
                            (theme.label_green, 7.0),
                            (theme.label_red, 10.0),
                            (theme.label_blue, 4.0),
                        ]
                        .map(|(color, height)| {
                            div().w(px(3.0)).h(px(height)).bg(color.opacity(0.75))
                        }),
                    ),
            )
            .children([20.0, 14.0].map(line)),
    }
}

/// The filmstrip in a layout preview: a band in the current film style
/// with tiny frames.
fn mini_filmstrip(look: &FilmLook, theme: &Theme) -> impl IntoElement {
    let frame = look
        .mat
        .or(look.sprockets.map(|s| s.color.opacity(0.5)))
        .unwrap_or(theme.text_faint.opacity(0.5));
    div()
        .flex()
        .flex_row()
        .items_center()
        .flex_shrink_0()
        .gap(px(2.0))
        .h(px(13.0))
        .px(px(3.0))
        .bg(look.base_bottom)
        .children((0..9).map(move |ix| {
            div()
                .w(px(13.0))
                .h(px(8.0))
                .rounded(px(look.picture_radius.min(2.0)))
                .bg(if ix == 2 {
                    look.selected.opacity(0.9)
                } else {
                    frame
                })
        }))
}

/// Three frames of a filmstrip style at reduced scale, the middle one
/// selected, so each style card shows its real palette and structure.
fn filmstrip_preview(look: &FilmLook) -> impl IntoElement {
    let look = *look;

    // Each mini frame gets its own little "scene" so the strip reads as
    // a roll of different photos.
    let scenes = [
        (
            hsla(210.0 / 360.0, 0.6, 0.6, 1.0),
            hsla(30.0 / 360.0, 0.8, 0.55, 1.0),
        ),
        (
            hsla(175.0 / 360.0, 0.45, 0.5, 1.0),
            hsla(110.0 / 360.0, 0.4, 0.35, 1.0),
        ),
        (
            hsla(270.0 / 360.0, 0.4, 0.55, 1.0),
            hsla(340.0 / 360.0, 0.6, 0.6, 1.0),
        ),
    ];

    let frames = scenes
        .into_iter()
        .enumerate()
        .map(move |(ix, (sky, ground))| {
            let selected = ix == 1;
            let (sky, ground) = if look.monochrome {
                (hsla(0.0, 0.0, 0.75, 1.0), hsla(0.0, 0.0, 0.25, 1.0))
            } else {
                (sky, ground)
            };

            let picture = div()
                .flex_1()
                .min_h(px(0.0))
                .mx(px(look.picture_inset * 0.5))
                .rounded(px(look.picture_radius * 0.6))
                .border_1()
                .border_color(if selected {
                    look.selected
                } else {
                    look.picture_border
                })
                .bg(linear_gradient(
                    180.0,
                    linear_color_stop(sky, 0.0),
                    linear_color_stop(ground, 1.0),
                ));

            let stars = div()
                .flex_shrink_0()
                .h(px(9.0))
                .px(px(3.0))
                .text_size(px(6.0))
                .text_color(look.star_filled)
                .child(
                    [
                        "\u{2605}\u{2605}\u{2605}",
                        "\u{2605}\u{2605}\u{2605}\u{2605}\u{2605}",
                        "\u{2605}\u{2605}",
                    ][ix],
                );

            let body = match look.mat {
                Some(mat) => div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .m(px(4.0))
                    .pt(px(3.0))
                    .px(px(3.0))
                    .bg(mat)
                    .border_1()
                    .border_color(if selected { look.selected } else { mat })
                    .shadow_sm()
                    .child(picture)
                    .child(stars),
                None => div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(picture)
                    .child(stars),
            };

            let sprockets = move || {
                look.sprockets.map(|s| {
                    div()
                        .flex()
                        .flex_row()
                        .flex_shrink_0()
                        .items_center()
                        .justify_around()
                        .h(px(8.0))
                        .children((0..s.per_frame).map(move |_| {
                            div()
                                .w(px(s.width * 0.5))
                                .h(px(s.height * 0.5))
                                .rounded(px(s.radius * 0.5))
                                .bg(s.color)
                        }))
                })
            };

            let edge = look.edge_print.map(|edge| {
                let (left, _) = (edge.label)(ix + 11);
                div()
                    .flex_shrink_0()
                    .h(px(8.0))
                    .px(px(3.0))
                    .text_size(px(6.0))
                    .when(look.monospace, |el| el.font_family("Menlo"))
                    .text_color(if selected {
                        edge.selected_color
                    } else {
                        edge.color
                    })
                    .child(SharedString::from(left))
            });

            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.0))
                .when(selected, |el| el.bg(look.frame_selected_bg))
                .when_some(look.frame_line, |el, color| {
                    el.border_r_1().border_color(color)
                })
                .when(look.sprockets.is_none(), |el| el.py(px(2.0)))
                .children(sprockets())
                .children(edge)
                .child(body)
                .children(sprockets())
        });

    div()
        .flex()
        .flex_col()
        .w_full()
        .h(px(CARD_PREVIEW_HEIGHT))
        .rounded_md()
        .overflow_hidden()
        .border_1()
        .border_color(look.divider)
        .bg(linear_gradient(
            180.0,
            linear_color_stop(look.base_top, 0.0),
            linear_color_stop(look.base_bottom, 1.0),
        ))
        .child(
            div()
                .flex_shrink_0()
                .h(px(11.0))
                .px(px(4.0))
                .bg(look.chrome_bg)
                .text_size(px(6.5))
                .when(look.monospace, |el| el.font_family("Menlo"))
                .text_color(look.chrome_text)
                .child(look.title),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_1()
                .min_h(px(0.0))
                .children(frames),
        )
}
