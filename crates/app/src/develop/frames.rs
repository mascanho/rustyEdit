//! The Frames section: a border drawn around the finished photo, from a
//! plain mat to instant film, film rebates and gilded mouldings.
//!
//! A drop-down names the current style and opens a list of them, grouped;
//! picking one starts from that style's own look, and the sliders below it
//! (only the ones the style uses) change it from there. While an editing
//! tool is picked, or the view is zoomed in, the frame steps aside so the
//! tool's overlays line up with the photo (see `frame_in_view`).

use gpui::{div, prelude::*, AnyElement, ClickEvent, Context, SharedString};
use rustyedit_core::{FrameSettings, FrameStyle, FRAME_GROUPS};
use rustyedit_ui::{sp, SliderSpec, SliderTrack};

use super::adjustments::wheel;
use crate::workspace::Workspace;

impl Workspace {
    pub(super) fn frames_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(frame) = self.catalog.selected_asset().map(|a| a.develop.frame) else {
            return Vec::new();
        };
        let open = self.develop_ui.frame_picker_open;

        // The drop-down's button: the style in use.
        let button = div()
            .id("frame-style-button")
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .h(sp(30.0))
            .px_2()
            .rounded_md()
            .border_1()
            .border_color(if open { theme.accent } else { theme.border_subtle })
            .bg(theme.background)
            .cursor_pointer()
            .text_size(sp(12.0))
            .text_color(theme.text)
            .hover(|el| el.border_color(theme.border))
            .child(frame.style.label())
            .child(rustyedit_ui::icon(
                if open {
                    rustyedit_ui::Icon::ChevronDown
                } else {
                    rustyedit_ui::Icon::ChevronRight
                },
                12.0,
                theme.text_muted,
            ))
            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                this.develop_ui.frame_picker_open = !this.develop_ui.frame_picker_open;
                cx.notify();
            }))
            .into_any_element();
        let mut rows = vec![button];

        if open {
            rows.push(self.frame_style_chip(FrameStyle::None, frame.style, cx));
            for (group, styles) in FRAME_GROUPS {
                rows.push(
                    div()
                        .pt_1()
                        .text_size(sp(10.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_faint)
                        .child(group.to_uppercase())
                        .into_any_element(),
                );
                rows.push(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap_1()
                        .children(
                            styles
                                .iter()
                                .map(|style| self.frame_style_chip(*style, frame.style, cx)),
                        )
                        .into_any_element(),
                );
            }
        }

        if frame.style == FrameStyle::None {
            rows.push(
                div()
                    .text_size(sp(10.0))
                    .text_color(theme.text_faint)
                    .child("Pick a frame to draw a border around the photo.")
                    .into_any_element(),
            );
            return rows;
        }

        let (uses_colour, uses_corner, uses_line, uses_depth) = frame.style.uses();
        let amount = |default: f32| SliderSpec {
            min: 0.0,
            max: 100.0,
            default,
            signed: false,
            track: SliderTrack::Accent,
        };
        let defaults = frame.style.defaults();
        rows.push(self.frame_slider(
            "size",
            "Size",
            frame.size,
            amount(defaults.size),
            |f, v| f.size = v,
            cx,
        ));
        if uses_colour {
            rows.push(self.frame_slider(
                "hue",
                "Hue",
                frame.hue,
                SliderSpec {
                    min: 0.0,
                    max: 360.0,
                    default: defaults.hue,
                    signed: false,
                    track: SliderTrack::Hue,
                },
                |f, v| f.hue = v,
                cx,
            ));
            rows.push(self.frame_slider(
                "saturation",
                "Saturation",
                frame.saturation,
                SliderSpec {
                    track: SliderTrack::Gradient(
                        gpui::hsla(0.0, 0.0, 0.5, 1.0),
                        wheel(frame.hue, 0.9, 0.5),
                    ),
                    ..amount(defaults.saturation)
                },
                |f, v| f.saturation = v,
                cx,
            ));
            rows.push(self.frame_slider(
                "lightness",
                "Lightness",
                frame.lightness,
                SliderSpec {
                    track: SliderTrack::Gradient(
                        gpui::hsla(0.0, 0.0, 0.0, 1.0),
                        gpui::hsla(0.0, 0.0, 1.0, 1.0),
                    ),
                    ..amount(defaults.lightness)
                },
                |f, v| f.lightness = v,
                cx,
            ));
        }
        if uses_corner {
            rows.push(self.frame_slider(
                "corner",
                "Corners",
                frame.corner,
                amount(defaults.corner),
                |f, v| f.corner = v,
                cx,
            ));
        }
        if uses_line {
            rows.push(self.frame_slider(
                "line",
                "Line",
                frame.line,
                amount(defaults.line),
                |f, v| f.line = v,
                cx,
            ));
        }
        if uses_depth {
            rows.push(self.frame_slider(
                "depth",
                "Depth",
                frame.shadow,
                amount(defaults.shadow),
                |f, v| f.shadow = v,
                cx,
            ));
        }
        rows.push(
            div()
                .text_size(sp(10.0))
                .text_color(theme.text_faint)
                .child("Shown in the viewer while no tool is picked and the photo is fitted; always added on export.")
                .into_any_element(),
        );
        rows
    }

    fn frame_style_chip(
        &self,
        style: FrameStyle,
        current: FrameStyle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let active = style == current;
        div()
            .id(SharedString::from(format!("frame-style-{style:?}")))
            .flex()
            .items_center()
            .h(sp(24.0))
            .px_2()
            .rounded_md()
            .border_1()
            .cursor_pointer()
            .text_size(sp(11.0))
            .when(active, |el| {
                el.bg(theme.accent_muted)
                    .border_color(theme.accent)
                    .text_color(theme.text)
            })
            .when(!active, |el| {
                el.border_color(theme.border_subtle)
                    .text_color(theme.text_muted)
                    .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
            })
            .child(style.label())
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                this.set_frame_style(style, cx);
            }))
            .into_any_element()
    }

    fn frame_slider(
        &self,
        id: &'static str,
        label: &'static str,
        value: f32,
        spec: SliderSpec,
        set: fn(&mut FrameSettings, f32),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entity = cx.entity();
        rustyedit_ui::labeled_slider(
            SharedString::from(format!("frame-{id}")),
            label,
            value,
            spec,
            &self.theme,
            move |value, _w, cx| {
                entity.update(cx, |this, cx| {
                    this.edit_develop(
                        &format!("Frame {id}"),
                        |d| {
                            set(&mut d.frame, value.clamp(spec.min, spec.max));
                        },
                        |d| (String::new(), d.frame.style.label().to_string()),
                        cx,
                    );
                });
            },
        )
        .into_any_element()
    }

    /// Picks a style, starting from its own look (and closing the list).
    fn set_frame_style(&mut self, style: FrameStyle, cx: &mut Context<Self>) {
        self.develop_ui.frame_picker_open = false;
        self.edit_develop(
            "Frame",
            |d| d.frame = style.defaults(),
            |d| (String::new(), d.frame.style.label().to_string()),
            cx,
        );
    }
}
