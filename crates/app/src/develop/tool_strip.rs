//! The tool strip pinned under the histogram: the tools that work on the
//! photo itself (Heal, Clone, Linear and Radial gradients, the
//! white-balance eyedropper), then view
//! toggles (clipping, zoom). It stays put while the sections scroll.
//!
//! Every way of picking a tool (the strip, the Retouch and Gradients
//! sections, Q / M / Shift+M / W) goes through [`Workspace::toggle_photo_tool`],
//! so only one is ever active and its section is open.

use rustyedit_ui::sp;
use gpui::{
    deferred, div, prelude::*, px, AnyElement, App, ClickEvent, Context, MouseDownEvent,
    SharedString, Window,
};
use rustyedit_core::{
    settings::{DockSide, LoupeBackground},
    GradientKind, SpotKind,
};
use rustyedit_ui::Icon;

use super::grid_overlay::GridOverlay;

use crate::{
    actions::{
        ToolLinearGradient, ToolRadialGradient, ToolRemove, ToolSpotRemoval, ToolWhiteBalance,
    },
    workspace::{Mode, Workspace},
};

/// The tools that work by clicking or dragging on the photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhotoTool {
    Heal,
    Clone,
    /// Content-aware remove: paint over an object to fill it in.
    Remove,
    Linear,
    Radial,
    Brush,
    Crop,
    WhiteBalance,
    /// Picks the brightness or colour a mask layer's range should select.
    RangePicker,
    /// Picks a tone in the photo to put a point on the tone curve at.
    CurveTarget,
}

impl PhotoTool {
    fn icon(&self) -> Icon {
        match self {
            PhotoTool::Heal => Icon::Heal,
            PhotoTool::Clone => Icon::Clone,
            PhotoTool::Remove => Icon::Remove,
            PhotoTool::Linear => Icon::LinearGradient,
            PhotoTool::Radial => Icon::RadialGradient,
            PhotoTool::Brush => Icon::Brush,
            PhotoTool::Crop => Icon::Crop,
            PhotoTool::WhiteBalance | PhotoTool::RangePicker | PhotoTool::CurveTarget => {
                Icon::Eyedropper
            }
        }
    }

    fn tooltip(&self) -> &'static str {
        match self {
            PhotoTool::Heal => "Heal spots (Q)",
            PhotoTool::Clone => "Clone spots",
            PhotoTool::Remove => "Content-aware remove: paint over an object (Shift+Q)",
            PhotoTool::Linear => "Linear gradient (M)",
            PhotoTool::Radial => "Radial gradient (Shift+M)",
            PhotoTool::Brush => "Brush mask: paint where to adjust (Alt erases)",
            PhotoTool::Crop => "Crop: drag the box on the photo (R)",
            PhotoTool::WhiteBalance => "White balance: click something neutral (W)",
            PhotoTool::RangePicker => "Pick a range: click a tone or colour in the photo",
            PhotoTool::CurveTarget => "Curve target: click a tone in the photo to add a curve point",
        }
    }

    /// The fold key of the section holding its settings.
    fn section_key(&self) -> &'static str {
        match self {
            PhotoTool::Heal | PhotoTool::Clone | PhotoTool::Remove => "develop-retouch",
            PhotoTool::Linear | PhotoTool::Radial | PhotoTool::Brush | PhotoTool::RangePicker => {
                "develop-gradients"
            },
            PhotoTool::Crop => "develop-crop",
            PhotoTool::CurveTarget => "develop-curve",
            PhotoTool::WhiteBalance => "develop-color",
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
            GradientKind::Brush => PhotoTool::Brush,
            // Range layers are added from the panel, not painted on.
            GradientKind::Range => PhotoTool::Linear,
        }
    }
}

impl Workspace {
    pub(crate) fn active_photo_tool(&self) -> Option<PhotoTool> {
        let ui = &self.develop_ui;
        ui.retouch
            .tool
            .map(|kind| {
                if ui.retouch.remove {
                    PhotoTool::Remove
                } else {
                    PhotoTool::from(kind)
                }
            })
            .or(ui.gradients.tool.map(PhotoTool::from))
            .or(ui.gradients.range_pick.map(|_| PhotoTool::RangePicker))
            .or(ui.crop_tool.then_some(PhotoTool::Crop))
            .or(ui.curve_target.then_some(PhotoTool::CurveTarget))
            .or(ui.wb_picker.then_some(PhotoTool::WhiteBalance))
    }

    /// Picks `tool` (putting any other down and opening its section), or
    /// puts it down if it's already picked.
    pub(crate) fn toggle_photo_tool(&mut self, tool: PhotoTool, cx: &mut Context<Self>) {
        let picking = self.active_photo_tool() != Some(tool);
        let ui = &mut self.develop_ui;
        ui.retouch.tool = None;
        ui.retouch.remove = false;
        ui.gradients.tool = None;
        ui.gradients.range_pick = None;
        ui.crop_tool = false;
        ui.crop_drag = None;
        ui.crop_hover = None;
        ui.curve_target = false;
        ui.wb_picker = false;
        if picking {
            match tool {
                PhotoTool::Heal => ui.retouch.tool = Some(SpotKind::Heal),
                PhotoTool::Clone => ui.retouch.tool = Some(SpotKind::Clone),
                PhotoTool::Remove => {
                    ui.retouch.tool = Some(SpotKind::Heal);
                    ui.retouch.remove = true;
                }
                PhotoTool::Linear => ui.gradients.tool = Some(GradientKind::Linear),
                PhotoTool::Radial => ui.gradients.tool = Some(GradientKind::Radial),
                PhotoTool::Brush => ui.gradients.tool = Some(GradientKind::Brush),
                PhotoTool::WhiteBalance => ui.wb_picker = true,
                PhotoTool::CurveTarget => ui.curve_target = true,
                PhotoTool::Crop => {
                    ui.crop_tool = true;
                    // The whole canvas is shown, so the box can be dragged.
                    ui.zoom.scale = None;
                }
                PhotoTool::RangePicker => ui.gradients.range_pick = ui.gradients.pending_pick.take(),
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

    pub fn tool_remove(&mut self, _: &ToolRemove, _w: &mut Window, cx: &mut Context<Self>) {
        if self.mode == Mode::Develop {
            self.toggle_photo_tool(PhotoTool::Remove, cx);
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

    pub fn tool_white_balance(
        &mut self,
        _: &ToolWhiteBalance,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode == Mode::Develop {
            self.toggle_photo_tool(PhotoTool::WhiteBalance, cx);
        }
    }

    /// The strip itself.
    pub(super) fn render_tool_strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let active = self.active_photo_tool();
        let tools = [
            PhotoTool::Heal,
            PhotoTool::Clone,
            PhotoTool::Remove,
            PhotoTool::Linear,
            PhotoTool::Radial,
            PhotoTool::Brush,
            PhotoTool::Crop,
            PhotoTool::WhiteBalance,
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
            .into_any_element()
    }

    /// The composition guides button for the rail beside the editing
    /// panel, with its menu opening toward the photo. `dock` is the side
    /// the panel (and so the rail) is on.
    pub(crate) fn guides_rail_button(&self, dock: DockSide, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let ui = &self.develop_ui;
        let open = ui.grid_menu_open;
        let active = ui.grid_overlay != GridOverlay::Off;

        let button = div()
            .id("rail-guides")
            .flex()
            .items_center()
            .justify_center()
            .size(px(rustyedit_ui::RAIL_BUTTON_SIZE))
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
            .tooltip(rustyedit_ui::tip(
                if active {
                    format!("Composition guide: {}", ui.grid_overlay.label())
                } else {
                    "Composition guides".to_string()
                },
                &theme,
            ))
            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                this.develop_ui.grid_menu_open = !this.develop_ui.grid_menu_open;
                this.develop_ui.view_menu_open = false;
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
                .h(sp(26.0))
                .px_2()
                .cursor_pointer()
                .text_size(sp(11.5))
                .text_color(if current {
                    theme.text
                } else {
                    theme.text_muted
                })
                .hover(|el| el.bg(theme.surface).text_color(theme.text))
                .child(
                    div()
                        .w(sp(12.0))
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
            .top_0()
            // Beside the rail, over the photo rather than the panel.
            .when(dock == DockSide::Right, |el| el.right(px(rustyedit_ui::RAIL_BUTTON_SIZE + 6.0)))
            .when(dock == DockSide::Left, |el| el.left(px(rustyedit_ui::RAIL_BUTTON_SIZE + 6.0)))
            .w(sp(200.0))
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
                el.child(div().h(sp(1.0)).my_1().bg(theme.border_subtle))
                    .child(
                        div()
                            .id("grid-rotate")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .h(sp(26.0))
                            .px_2()
                            .cursor_pointer()
                            .text_size(sp(11.5))
                            .text_color(theme.text_muted)
                            .hover(|el| el.bg(theme.surface).text_color(theme.text))
                            .child(div().w(sp(12.0)))
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
    /// The eye button for the rail beside the editing panel (backgrounds,
    /// clipping, soft proof, zoom, info), its menu opening toward the photo
    /// like the guides'. `dock` is the side the panel is on.
    pub(crate) fn view_rail_button(&self, dock: DockSide, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let open = self.develop_ui.view_menu_open;
        let ui = &self.histogram_ui;
        let proofing = self.develop_ui.proof.active();

        let button = div()
            .id("rail-view")
            .flex()
            .items_center()
            .justify_center()
            .size(px(rustyedit_ui::RAIL_BUTTON_SIZE))
            .rounded_md()
            .cursor_pointer()
            // Lit while a soft proof is showing, so it isn't mistaken for the
            // photo's own colours.
            .when(open || proofing, |el| el.bg(theme.accent_muted))
            .when(!(open || proofing), |el| {
                el.hover(|el| el.bg(theme.surface_elevated))
            })
            .child(rustyedit_ui::icon(
                Icon::View,
                16.0,
                if open || proofing {
                    theme.text
                } else {
                    theme.text_muted
                },
            ))
            .tooltip(rustyedit_ui::tip(
                if proofing {
                    "View options (soft proof on)"
                } else {
                    "View options"
                },
                &theme,
            ))
            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                this.develop_ui.view_menu_open = !this.develop_ui.view_menu_open;
                this.develop_ui.grid_menu_open = false;
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
                .h(sp(26.0))
                .px_2()
                .cursor_pointer()
                .text_size(sp(11.5))
                .text_color(theme.text_muted)
                .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                .child(div().w(sp(12.0)).text_color(theme.accent).child(if on {
                    "\u{2713}"
                } else {
                    ""
                }))
                .child(div().flex_1().child(label))
                .child(
                    div()
                        .text_size(sp(10.0))
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
            .top_0()
            .when(dock == DockSide::Right, |el| el.right(px(rustyedit_ui::RAIL_BUTTON_SIZE + 6.0)))
            .when(dock == DockSide::Left, |el| el.left(px(rustyedit_ui::RAIL_BUTTON_SIZE + 6.0)))
            .w(sp(230.0))
            .max_h(sp(560.0))
            .overflow_y_scroll()
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
            .child(div().h(sp(1.0)).my_1().bg(theme.border_subtle))
            .child(
                div()
                    .px_2()
                    .pb_1()
                    .text_size(sp(10.0))
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
                    .h(sp(26.0))
                    .px_2()
                    .cursor_pointer()
                    .text_size(sp(11.5))
                    .text_color(if current {
                        theme.text
                    } else {
                        theme.text_muted
                    })
                    .hover(|el| el.bg(theme.surface).text_color(theme.text))
                    .child(
                        div()
                            .w(sp(12.0))
                            .text_color(theme.accent)
                            .child(if current { "\u{2713}" } else { "" }),
                    )
                    .child(
                        div()
                            .size(sp(14.0))
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
            .child(div().h(sp(1.0)).my_1().bg(theme.border_subtle))
            .children(self.proof_menu_rows(cx))
            .child(div().h(sp(1.0)).my_1().bg(theme.border_subtle))
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
            .size(sp(28.0))
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
