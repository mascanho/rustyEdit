//! The tool strip pinned under the histogram: the tools that work on the
//! photo itself (Heal, Clone, Linear and Radial gradients), then view
//! toggles (clipping, zoom). It stays put while the sections scroll.
//!
//! Every way of picking a tool (the strip, the Retouch and Gradients
//! sections, Q / M / Shift+M) goes through [`Workspace::toggle_photo_tool`],
//! so only one is ever active and its section is open.

use gpui::{
    deferred, div, prelude::*, px, AnyElement, App, ClickEvent, Context, MouseDownEvent,
    SharedString, Window,
};
use rustyedit_core::{settings::LoupeBackground, GradientKind, SpotKind};
use rustyedit_ui::Icon;

use super::grid_overlay::GridOverlay;
use crate::before_after::BaMode;

use crate::{
    actions::{ToolLinearGradient, ToolRadialGradient, ToolSpotRemoval},
    workspace::{Mode, Workspace},
};

/// The tools that work by clicking or dragging on the photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhotoTool {
    Heal,
    Clone,
    Linear,
    Radial,
}

impl PhotoTool {
    fn icon(&self) -> Icon {
        match self {
            PhotoTool::Heal => Icon::Heal,
            PhotoTool::Clone => Icon::Clone,
            PhotoTool::Linear => Icon::LinearGradient,
            PhotoTool::Radial => Icon::RadialGradient,
        }
    }

    fn tooltip(&self) -> &'static str {
        match self {
            PhotoTool::Heal => "Heal spots (Q)",
            PhotoTool::Clone => "Clone spots",
            PhotoTool::Linear => "Linear gradient (M)",
            PhotoTool::Radial => "Radial gradient (Shift+M)",
        }
    }

    /// The fold key of the section holding its settings.
    fn section_key(&self) -> &'static str {
        match self {
            PhotoTool::Heal | PhotoTool::Clone => "develop-retouch",
            PhotoTool::Linear | PhotoTool::Radial => "develop-gradients",
        }
    }
}

impl From<SpotKind> for PhotoTool {
    fn from(kind: SpotKind) -> Self {
        match kind {
            SpotKind::Heal => PhotoTool::Heal,
            SpotKind::Clone => PhotoTool::Clone,
        }
    }
}

impl From<GradientKind> for PhotoTool {
    fn from(kind: GradientKind) -> Self {
        match kind {
            GradientKind::Linear => PhotoTool::Linear,
            GradientKind::Radial => PhotoTool::Radial,
        }
    }
}

impl Workspace {
    pub(crate) fn active_photo_tool(&self) -> Option<PhotoTool> {
        let ui = &self.develop_ui;
        ui.retouch
            .tool
            .map(PhotoTool::from)
            .or(ui.gradients.tool.map(PhotoTool::from))
    }

    /// Picks `tool` (putting any other down and opening its section), or
    /// puts it down if it's already picked.
    pub(crate) fn toggle_photo_tool(&mut self, tool: PhotoTool, cx: &mut Context<Self>) {
        let picking = self.active_photo_tool() != Some(tool);
        let ui = &mut self.develop_ui;
        ui.retouch.tool = None;
        ui.gradients.tool = None;
        if picking {
            match tool {
                PhotoTool::Heal => ui.retouch.tool = Some(SpotKind::Heal),
                PhotoTool::Clone => ui.retouch.tool = Some(SpotKind::Clone),
                PhotoTool::Linear => ui.gradients.tool = Some(GradientKind::Linear),
                PhotoTool::Radial => ui.gradients.tool = Some(GradientKind::Radial),
            }
            self.collapsed_sections.remove(tool.section_key());
        }
        cx.notify();
    }

    /// Esc: puts down the active photo tool. Returns whether there was one
    /// (otherwise Esc keeps its usual meaning).
    pub(crate) fn put_down_photo_tool(&mut self, cx: &mut Context<Self>) -> bool {
        match self.active_photo_tool() {
            Some(tool) => {
                self.toggle_photo_tool(tool, cx);
                true
            }
            None => false,
        }
    }

    pub fn tool_spot_removal(
        &mut self,
        _: &ToolSpotRemoval,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode == Mode::Develop {
            // Q picks spot removal with whichever kind was used last time.
            let tool = match self.active_photo_tool() {
                Some(tool @ (PhotoTool::Heal | PhotoTool::Clone)) => tool,
                _ => PhotoTool::Heal,
            };
            self.toggle_photo_tool(tool, cx);
        }
    }

    pub fn tool_linear_gradient(
        &mut self,
        _: &ToolLinearGradient,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode == Mode::Develop {
            self.toggle_photo_tool(PhotoTool::Linear, cx);
        }
    }

    pub fn tool_radial_gradient(
        &mut self,
        _: &ToolRadialGradient,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode == Mode::Develop {
            self.toggle_photo_tool(PhotoTool::Radial, cx);
        }
    }

    /// The strip itself.
    pub(super) fn render_tool_strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let active = self.active_photo_tool();
        let tools = [
            PhotoTool::Heal,
            PhotoTool::Clone,
            PhotoTool::Linear,
            PhotoTool::Radial,
        ]
        .map(|tool| {
            self.strip_button(
                SharedString::from(format!("tool-{tool:?}")),
                tool.icon(),
                tool.tooltip(),
                active == Some(tool),
                cx.listener(move |this, _: &ClickEvent, _w, cx| this.toggle_photo_tool(tool, cx)),
            )
        });

        let zoom = self.strip_button(
            "tool-zoom".into(),
            Icon::ZoomIn,
            "Zoom to 100% / Fit (Z)",
            self.develop_ui.zoom.scale.is_some(),
            cx.listener(|this, _: &ClickEvent, _w, cx| this.toggle_develop_zoom(cx)),
        );

        let theme = self.theme;
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .p_0p5()
            .rounded_md()
            .bg(theme.background)
            .border_1()
            .border_color(theme.border_subtle)
            .child(div().flex().flex_row().gap_0p5().children(tools))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_0p5()
                    .child(div().w(px(1.0)).h(px(16.0)).mx_1().bg(theme.border))
                    .child(zoom)
                    .child(self.before_after_menu_button(cx))
                    .child(self.grid_menu_button(cx))
                    .child(self.view_menu_button(cx)),
            )
            .into_any_element()
    }

    /// The before / after button with its dropdown of comparison views.
    fn before_after_menu_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let open = self.develop_ui.ba_menu_open;
        let active = self.ba.mode != BaMode::Off;

        let button = div()
            .id("tool-before-after")
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .gap_0p5()
            .h(px(28.0))
            .px_1p5()
            .rounded_md()
            .cursor_pointer()
            .when(open || active, |el| el.bg(theme.accent_muted))
            .when(!(open || active), |el| {
                el.hover(|el| el.bg(theme.surface_elevated))
            })
            .child(rustyedit_ui::icon(
                Icon::BeforeAfter,
                16.0,
                if open || active {
                    theme.text
                } else {
                    theme.text_muted
                },
            ))
            .child(rustyedit_ui::icon(
                Icon::ChevronDown,
                10.0,
                theme.text_faint,
            ))
            .tooltip(rustyedit_ui::tip("Before / After (\\)", &theme))
            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                let ui = &mut this.develop_ui;
                ui.ba_menu_open = !ui.ba_menu_open;
                ui.view_menu_open = false;
                ui.grid_menu_open = false;
                cx.notify();
            }));

        let rows = BaMode::ALL.iter().map(|&option| {
            let entity = cx.entity();
            let current = self.ba.mode == option;
            div()
                .id(SharedString::from(format!("ba-menu-{}", option.label())))
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .min_h(px(34.0))
                .px_2()
                .cursor_pointer()
                .text_size(px(11.5))
                .text_color(if current {
                    theme.text
                } else {
                    theme.text_muted
                })
                .hover(|el| el.bg(theme.surface).text_color(theme.text))
                .child(
                    div()
                        .w(px(12.0))
                        .text_color(theme.accent)
                        .child(if current { "\u{2713}" } else { "" }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .child(option.label())
                        .child(
                            div()
                                .text_size(px(9.5))
                                .text_color(theme.text_faint)
                                .child(option.description()),
                        ),
                )
                .on_click(move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                    entity.update(cx, |this, cx| {
                        this.develop_ui.ba_menu_open = false;
                        this.set_ba_mode(option, cx);
                    });
                })
        });

        let menu = div()
            .id("ba-menu")
            .absolute()
            .top(px(30.0))
            .right_0()
            .w(px(230.0))
            .py_1()
            .bg(theme.surface_elevated)
            .border_1()
            .border_color(theme.border)
            .rounded_md()
            .shadow_lg()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _w, cx| {
                this.develop_ui.ba_menu_open = false;
                cx.notify();
            }))
            .children(rows)
            .child(div().h(px(1.0)).my_1().bg(theme.border_subtle))
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_size(px(10.0))
                    .text_color(theme.text_faint)
                    .child("Press \\ to switch on and off"),
            );

        div()
            .relative()
            .child(button)
            .when(open, |el| el.child(deferred(menu).with_priority(1)))
            .into_any_element()
    }

    /// The grid button with its dropdown of composition guides.
    fn grid_menu_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let ui = &self.develop_ui;
        let open = ui.grid_menu_open;
        let active = ui.grid_overlay != GridOverlay::Off;

        let button = div()
            .id("tool-grid")
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .gap_0p5()
            .h(px(28.0))
            .px_1p5()
            .rounded_md()
            .cursor_pointer()
            .when(open || active, |el| el.bg(theme.accent_muted))
            .when(!(open || active), |el| {
                el.hover(|el| el.bg(theme.surface_elevated))
            })
            .child(rustyedit_ui::icon(
                Icon::Grid,
                16.0,
                if open || active {
                    theme.text
                } else {
                    theme.text_muted
                },
            ))
            .child(rustyedit_ui::icon(
                Icon::ChevronDown,
                10.0,
                theme.text_faint,
            ))
            .tooltip(rustyedit_ui::tip("Composition guides", &theme))
            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                this.develop_ui.grid_menu_open = !this.develop_ui.grid_menu_open;
                this.develop_ui.view_menu_open = false;
                this.develop_ui.ba_menu_open = false;
                cx.notify();
            }));

        let rows = GridOverlay::ALL.iter().map(|&option| {
            let entity = cx.entity();
            let current = ui.grid_overlay == option;
            div()
                .id(SharedString::from(format!("grid-{}", option.label())))
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .h(px(26.0))
                .px_2()
                .cursor_pointer()
                .text_size(px(11.5))
                .text_color(if current {
                    theme.text
                } else {
                    theme.text_muted
                })
                .hover(|el| el.bg(theme.surface).text_color(theme.text))
                .child(
                    div()
                        .w(px(12.0))
                        .text_color(theme.accent)
                        .child(if current { "\u{2713}" } else { "" }),
                )
                .child(div().flex_1().child(option.label()))
                .on_click(move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                    entity.update(cx, |this, cx| {
                        this.develop_ui.grid_overlay = option;
                        this.develop_ui.grid_menu_open = false;
                        cx.notify();
                    });
                })
        });

        let menu = div()
            .id("grid-menu")
            .absolute()
            .top(px(30.0))
            .right_0()
            .w(px(200.0))
            .py_1()
            .bg(theme.surface_elevated)
            .border_1()
            .border_color(theme.border)
            .rounded_md()
            .shadow_lg()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _w, cx| {
                this.develop_ui.grid_menu_open = false;
                cx.notify();
            }))
            .children(rows)
            .when(ui.grid_overlay.has_orientations(), |el| {
                let entity = cx.entity();
                el.child(div().h(px(1.0)).my_1().bg(theme.border_subtle))
                    .child(
                        div()
                            .id("grid-rotate")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .h(px(26.0))
                            .px_2()
                            .cursor_pointer()
                            .text_size(px(11.5))
                            .text_color(theme.text_muted)
                            .hover(|el| el.bg(theme.surface).text_color(theme.text))
                            .child(div().w(px(12.0)))
                            .child(div().flex_1().child("Flip orientation"))
                            .on_click(move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    let ui = &mut this.develop_ui;
                                    ui.grid_orientation = (ui.grid_orientation + 1) % 4;
                                    cx.notify();
                                });
                            }),
                    )
            });

        div()
            .relative()
            .child(button)
            .when(open, |el| el.child(deferred(menu).with_priority(1)))
            .into_any_element()
    }

    /// The eye button with its dropdown of view options.
    fn view_menu_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let open = self.develop_ui.view_menu_open;
        let ui = &self.histogram_ui;

        let button = div()
            .id("tool-view")
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .gap_0p5()
            .h(px(28.0))
            .px_1p5()
            .rounded_md()
            .cursor_pointer()
            .when(open, |el| el.bg(theme.accent_muted))
            .when(!open, |el| el.hover(|el| el.bg(theme.surface_elevated)))
            .child(rustyedit_ui::icon(
                Icon::View,
                16.0,
                if open { theme.text } else { theme.text_muted },
            ))
            .child(rustyedit_ui::icon(
                Icon::ChevronDown,
                10.0,
                theme.text_faint,
            ))
            .tooltip(rustyedit_ui::tip("View options", &theme))
            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                this.develop_ui.view_menu_open = !this.develop_ui.view_menu_open;
                this.develop_ui.grid_menu_open = false;
                this.develop_ui.ba_menu_open = false;
                cx.notify();
            }));

        let entity = cx.entity();
        let item = |id: &'static str,
                    label: &'static str,
                    hint: &'static str,
                    on: bool,
                    close: bool,
                    action: fn(&mut Workspace, &mut Context<Workspace>)| {
            let entity = entity.clone();
            div()
                .id(id)
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .h(px(26.0))
                .px_2()
                .cursor_pointer()
                .text_size(px(11.5))
                .text_color(theme.text_muted)
                .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                .child(div().w(px(12.0)).text_color(theme.accent).child(if on {
                    "\u{2713}"
                } else {
                    ""
                }))
                .child(div().flex_1().child(label))
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme.text_faint)
                        .child(hint),
                )
                .on_click(move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                    entity.update(cx, |this, cx| {
                        action(this, cx);
                        if close {
                            this.develop_ui.view_menu_open = false;
                        }
                        cx.notify();
                    });
                })
        };

        let menu = div()
            .id("view-menu")
            .absolute()
            .top(px(30.0))
            .right_0()
            .w(px(220.0))
            .py_1()
            .bg(theme.surface_elevated)
            .border_1()
            .border_color(theme.border)
            .rounded_md()
            .shadow_lg()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _w, cx| {
                this.develop_ui.view_menu_open = false;
                cx.notify();
            }))
            .child(item(
                "view-shadows",
                "Shadow clipping",
                "",
                ui.show_shadow_clipping,
                false,
                |this, cx| {
                    this.histogram_ui.show_shadow_clipping =
                        !this.histogram_ui.show_shadow_clipping;
                    this.request_preview_update(cx);
                },
            ))
            .child(item(
                "view-highlights",
                "Highlight clipping",
                "",
                ui.show_highlight_clipping,
                false,
                |this, cx| {
                    this.histogram_ui.show_highlight_clipping =
                        !this.histogram_ui.show_highlight_clipping;
                    this.request_preview_update(cx);
                },
            ))
            .child(div().h(px(1.0)).my_1().bg(theme.border_subtle))
            .child(
                div()
                    .px_2()
                    .pb_1()
                    .text_size(px(10.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.text_faint)
                    .child("BACKGROUND (B)"),
            )
            .children(LoupeBackground::ALL.iter().map(|&option| {
                let entity = cx.entity();
                let current = self.settings.develop.viewer_background == option;
                let swatch = match option.hsl() {
                    Some((h, s, l)) => gpui::hsla(h, s, l, 1.0),
                    None => theme.background,
                };
                div()
                    .id(SharedString::from(format!("view-bg-{}", option.label())))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .h(px(26.0))
                    .px_2()
                    .cursor_pointer()
                    .text_size(px(11.5))
                    .text_color(if current {
                        theme.text
                    } else {
                        theme.text_muted
                    })
                    .hover(|el| el.bg(theme.surface).text_color(theme.text))
                    .child(
                        div()
                            .w(px(12.0))
                            .text_color(theme.accent)
                            .child(if current { "\u{2713}" } else { "" }),
                    )
                    .child(
                        div()
                            .size(px(14.0))
                            .rounded_sm()
                            .bg(swatch)
                            .border_1()
                            .border_color(theme.border),
                    )
                    .child(div().flex_1().child(option.label()))
                    .on_click(move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.update_settings(|s| s.develop.viewer_background = option, cx);
                        });
                    })
            }))
            .child(div().h(px(1.0)).my_1().bg(theme.border_subtle))
            .child(item(
                "view-before-after",
                "Before / After",
                "\\",
                self.ba.mode != crate::before_after::BaMode::Off,
                true,
                |this, cx| {
                    let next = if this.ba.mode == crate::before_after::BaMode::Off {
                        crate::before_after::BaMode::Split
                    } else {
                        crate::before_after::BaMode::Off
                    };
                    this.set_ba_mode(next, cx);
                },
            ))
            .child(item(
                "view-zoom",
                "Zoom to 100%",
                "Z",
                self.develop_ui.zoom.scale.is_some(),
                true,
                |this, cx| this.toggle_develop_zoom(cx),
            ))
            .child(item(
                "view-info",
                "Photo info",
                "I",
                self.info_visible,
                true,
                |this, cx| {
                    this.info_visible = !this.info_visible;
                    if this.info_visible {
                        this.request_metadata(cx);
                    }
                },
            ));

        div()
            .relative()
            .child(button)
            .when(open, |el| el.child(deferred(menu).with_priority(1)))
            .into_any_element()
    }

    fn strip_button(
        &self,
        id: SharedString,
        icon: Icon,
        tooltip: &'static str,
        active: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> AnyElement {
        let theme = self.theme;
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .size(px(28.0))
            .rounded_md()
            .cursor_pointer()
            .when(active, |el| el.bg(theme.accent_muted))
            .when(!active, |el| el.hover(|el| el.bg(theme.surface_elevated)))
            .child(rustyedit_ui::icon(
                icon,
                16.0,
                if active { theme.text } else { theme.text_muted },
            ))
            .tooltip(rustyedit_ui::tip(tooltip, &theme))
            .on_click(on_click)
            .into_any_element()
    }
}
