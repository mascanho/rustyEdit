//! The Lens Blur section: Amount, Boost and Falloff, the bokeh's shape,
//! and what's in focus.
//!
//! The focus is the default oval around the middle, or any mask layer
//! (picked here, or made from the oval with "Edit focus", which then
//! shows its handles on the photo). The layer is marked as the focus, so
//! it follows the layer when layers are reordered or removed.

use gpui::{div, prelude::*, AnyElement, ClickEvent, Context, SharedString};
use rustyedit_core::{Adjustment, BokehShape, GradientKind, LensBlur};
use rustyedit_ui::{sp, Icon, Theme};

use super::{auto::small_button, tool_strip::PhotoTool};
use crate::workspace::Workspace;

impl Workspace {
    pub(super) fn lens_blur_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(develop) = self.catalog.selected_asset().map(|a| a.develop) else {
            return Vec::new();
        };
        let lens = develop.lens_blur;
        let focus_ix = develop.gradients.as_slice().iter().position(|g| g.lens_focus);

        let mut rows = vec![div()
            .text_size(sp(10.0))
            .text_color(theme.text_faint)
            .child("Keeps the focus sharp and melts the rest into a fast lens's blur, highlights blooming into bokeh.")
            .into_any_element()];
        rows.extend(self.adjustment_sliders(&Adjustment::LENS_BLUR, cx));

        // Bokeh shape.
        let shapes = BokehShape::ALL.map(|shape| {
            chip(
                SharedString::from(format!("lens-shape-{}", shape.label())),
                shape.label(),
                lens.shape == shape,
                &theme,
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                this.edit_lens_blur("Bokeh shape", move |l| l.shape = shape, cx)
            }))
            .into_any_element()
        });
        rows.push(labelled("Bokeh", chips(shapes.into_iter().collect()), &theme));

        // What's in focus.
        let mut focus = vec![chip(
            "lens-focus-centre".into(),
            "Centre",
            focus_ix.is_none(),
            &theme,
        )
        .tooltip(rustyedit_ui::tip("An oval around the middle of the photo", &theme))
        .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.set_lens_focus(None, cx)))
        .into_any_element()];
        for (ix, g) in develop.gradients.as_slice().iter().enumerate() {
            focus.push(
                chip(
                    SharedString::from(format!("lens-focus-{ix}")),
                    &format!("{} {}", g.kind.label(), ix + 1),
                    focus_ix == Some(ix),
                    &theme,
                )
                .tooltip(rustyedit_ui::tip("Keep this mask layer in focus", &theme))
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.set_lens_focus(Some(ix), cx)
                }))
                .into_any_element(),
            );
        }
        rows.push(labelled("Focus", chips(focus), &theme));

        let showing = self.develop_ui.gradients.show_mask
            && focus_ix.is_some()
            && self.develop_ui.gradients.selected == focus_ix;
        rows.push(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_1()
                .pt_1()
                .child(small_button(
                    "lens-edit-focus",
                    Icon::Edit,
                    "Edit focus",
                    "Move and resize the focus on the photo",
                    false,
                    true,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| this.edit_lens_focus(false, cx)),
                ))
                .child(small_button(
                    "lens-show-focus",
                    Icon::View,
                    "Show focus",
                    "Paint what's in focus over the photo in red",
                    showing,
                    true,
                    &theme,
                    cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        if showing {
                            this.develop_ui.gradients.show_mask = false;
                            this.refresh_live_preview(cx);
                            cx.notify();
                        } else {
                            this.edit_lens_focus(true, cx);
                        }
                    }),
                ))
                .child(small_button(
                    "lens-invert",
                    Icon::BeforeAfter,
                    "Invert",
                    "Blur the focus area instead of everything else",
                    lens.invert,
                    true,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| {
                        this.edit_lens_blur("Lens blur invert", |l| l.invert = !l.invert, cx)
                    }),
                ))
                .into_any_element(),
        );
        if !lens.is_active() {
            rows.push(
                div()
                    .text_size(sp(10.0))
                    .text_color(theme.text_faint)
                    .child("Raise Amount to start.")
                    .into_any_element(),
            );
        }
        rows
    }

    fn edit_lens_blur(
        &mut self,
        label: &str,
        edit: impl FnOnce(&mut LensBlur),
        cx: &mut Context<Self>,
    ) {
        self.edit_develop(
            label,
            |d| edit(&mut d.lens_blur),
            |d| (String::new(), d.lens_blur.shape.label().to_string()),
            cx,
        );
    }

    /// Makes layer `ix` the focus (only one can be), or the centre oval.
    fn set_lens_focus(&mut self, ix: Option<usize>, cx: &mut Context<Self>) {
        self.edit_develop(
            "Lens blur focus",
            |d| {
                for i in 0..d.gradients.len() {
                    if let Some(g) = d.gradients.get_mut(i) {
                        g.lens_focus = Some(i) == ix;
                    }
                }
            },
            |_| {
                (
                    String::new(),
                    ix.map_or("Centre".to_string(), |i| format!("Mask {}", i + 1)),
                )
            },
            cx,
        );
    }

    /// Selects the focus layer to edit on the photo (making one from the
    /// centre oval first if there's none), with `show` painting it red.
    fn edit_lens_focus(&mut self, show: bool, cx: &mut Context<Self>) {
        let Some(develop) = self.catalog.selected_asset().map(|a| a.develop) else {
            return;
        };
        let mut ix = develop.gradients.as_slice().iter().position(|g| g.lens_focus);
        if ix.is_none() {
            let mut added = None;
            self.edit_develop(
                "Lens blur focus",
                |d| {
                    added = d.gradients.push(rustyedit_core::Gradient {
                        lens_focus: true,
                        ..LensBlur::centre_focus()
                    })
                },
                |_| (String::new(), "Focus area".to_string()),
                cx,
            );
            ix = added;
            if ix.is_none() {
                self.status_message = Some("A photo can have up to 8 mask layers".into());
                cx.notify();
                return;
            }
        }
        let Some(ix) = ix else { return };
        let kind = self
            .catalog
            .selected_asset()
            .and_then(|a| a.develop.gradients.as_slice().get(ix).map(|g| g.kind));
        let tool = match kind {
            Some(GradientKind::Linear) => Some(PhotoTool::Linear),
            Some(GradientKind::Brush) => Some(PhotoTool::Brush),
            Some(GradientKind::Radial) => Some(PhotoTool::Radial),
            _ => None,
        };
        if let Some(tool) = tool.filter(|t| self.active_photo_tool() != Some(*t)) {
            self.toggle_photo_tool(tool, cx);
        }
        let ui = &mut self.develop_ui.gradients;
        ui.selected = Some(ix);
        ui.show_mask = show;
        self.collapsed_sections.remove("develop-gradients");
        self.refresh_live_preview(cx);
        cx.notify();
    }
}

fn chip(id: SharedString, label: &str, on: bool, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .h(sp(20.0))
        .px_1p5()
        .rounded_md()
        .border_1()
        .cursor_pointer()
        .text_size(sp(10.5))
        .when(on, |el| {
            el.bg(theme.accent_muted)
                .border_color(theme.accent)
                .text_color(theme.text)
        })
        .when(!on, |el| {
            el.border_color(theme.border_subtle)
                .text_color(theme.text_muted)
                .hover(|el| el.text_color(theme.text).border_color(theme.border))
        })
        .child(label.to_string())
}

fn chips(children: Vec<AnyElement>) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap_1()
        .children(children)
        .into_any_element()
}

fn labelled(label: &'static str, control: AnyElement, theme: &Theme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_start()
        .gap_2()
        .pt_1()
        .child(
            div()
                .w(sp(48.0))
                .flex_shrink_0()
                .pt(sp(3.0))
                .text_size(sp(10.5))
                .text_color(theme.text_muted)
                .child(label),
        )
        .child(div().flex_1().min_w(sp(0.0)).child(control))
        .into_any_element()
}
