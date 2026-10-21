//! The tool strip pinned under the histogram: the tools that work on the
//! photo itself (Heal, Clone, Linear and Radial gradients), then view
//! toggles (clipping, zoom). It stays put while the sections scroll.
//!
//! Every way of picking a tool (the strip, the Retouch and Gradients
//! sections, Q / M / Shift+M) goes through [`Workspace::toggle_photo_tool`],
//! so only one is ever active and its section is open.

use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context, SharedString, Window};
use rustyedit_core::{GradientKind, SpotKind};
use rustyedit_ui::Icon;

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

        let ui = &self.histogram_ui;
        let clipping = self.strip_button(
            "tool-clipping".into(),
            Icon::Clipping,
            "Show clipping (J)",
            ui.show_shadow_clipping && ui.show_highlight_clipping,
            cx.listener(|this, _: &ClickEvent, _w, cx| this.toggle_clipping_overlay(cx)),
        );
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
                    .child(clipping)
                    .child(zoom),
            )
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
