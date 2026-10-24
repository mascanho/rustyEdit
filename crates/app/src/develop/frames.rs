//! The Frames rail tool: a border drawn around the finished photo, from a
//! plain mat to instant film, film rebates and gilded mouldings. It is laid
//! out like Presets: controls pinned under the Navigator, then the styles
//! in grouped cards that apply on a click.
//!
//! Only Develop has it. While an editing tool is picked, or the view is
//! zoomed in, the frame steps aside so the tool's overlays line up with the
//! photo (see `frame_in_view`).

use gpui::{div, prelude::*, AnyElement, App, ClickEvent, Context, SharedString, Window};
use rustyedit_core::{DevelopSection, FrameSettings, FrameStyle, FRAME_GROUPS};
use rustyedit_ui::{sp, Icon, SliderSpec, SliderTrack};

use super::{adjustments::wheel, tools::list_row};
use crate::workspace::Workspace;

impl Workspace {
    /// Pinned under the Navigator: the main action (take the frame off),
    /// icon buttons, and the border's size.
    pub(crate) fn frames_pinned(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let develop = self.catalog.selected_asset().map(|a| a.develop);
        let has_photo = develop.is_some();
        let frame = develop.map(|d| d.frame).unwrap_or_default();
        let shown = develop.is_none_or(|d| d.is_section_enabled(DevelopSection::Frames));
        let framed = frame.style != FrameStyle::None;

        // The main action, styled like Presets' "Save look".
        let remove = {
            let enabled = has_photo && framed;
            div()
                .id("frame-remove")
                .flex_1()
                .min_w(sp(0.0))
                .flex()
                .flex_row()
                .items_center()
                .justify_center()
                .gap_1p5()
                .h(sp(28.0))
                .border_1()
                .border_color(theme.accent)
                .bg(theme.accent_muted)
                .text_size(sp(11.5))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text)
                .when(enabled, |el| {
                    el.cursor_pointer()
                        .hover(|el| el.bg(theme.accent.opacity(0.35)))
                })
                .when(!enabled, |el| el.opacity(0.45))
                .tooltip(rustyedit_ui::tip("Take the frame off the photo", &theme))
                .child(rustyedit_ui::icon(Icon::Frame, 15.0, theme.text))
                .child("No frame")
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    if enabled {
                        this.set_frame_style(FrameStyle::None, cx);
                    }
                }))
                .into_any_element()
        };

        // Square icon buttons sharing the row, like Presets'.
        let tool = |id: &'static str,
                    icon: Icon,
                    tip: &'static str,
                    active: bool,
                    enabled: bool,
                    on_click: Box<dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)>,
                    cx: &mut Context<Self>|
         -> AnyElement {
            let entity = cx.entity();
            div()
                .id(id)
                .flex()
                .flex_1()
                .min_w(sp(22.0))
                .items_center()
                .justify_center()
                .h(sp(28.0))
                .border_1()
                .when(active, |el| el.bg(theme.accent_muted).border_color(theme.accent))
                .when(!active, |el| el.bg(theme.background).border_color(theme.border_subtle))
                .when(enabled, |el| {
                    el.cursor_pointer()
                        .hover(|el| el.bg(theme.surface_elevated).border_color(theme.border))
                })
                .when(!enabled, |el| el.opacity(0.4))
                .tooltip(rustyedit_ui::tip(tip, &theme))
                .child(rustyedit_ui::icon(
                    icon,
                    15.0,
                    if active { theme.text } else { theme.text_muted },
                ))
                .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                    if enabled {
                        entity.update(cx, |this, cx| on_click(this, window, cx));
                    }
                })
                .into_any_element()
        };
        let visible = tool(
            "frame-visible",
            Icon::View,
            if shown { "Hide the frame (keeps its settings)" } else { "Show the frame" },
            shown && framed,
            has_photo && framed,
            Box::new(move |this, _, cx| {
                this.set_section_enabled(DevelopSection::Frames, !shown, cx)
            }),
            cx,
        );
        let reset = tool(
            "frame-reset-style",
            Icon::Reset,
            "Reset the frame to the style's own look",
            false,
            has_photo && framed,
            Box::new(move |this, _, cx| this.set_frame_style(frame.style, cx)),
            cx,
        );

        let size = {
            let defaults = frame.style.defaults();
            self.frame_slider(
                "size",
                "Size",
                frame.size,
                SliderSpec {
                    min: 0.0,
                    max: 100.0,
                    default: defaults.size,
                    signed: false,
                    track: SliderTrack::Amount,
                },
                |f, v| f.size = v,
                cx,
            )
        };

        div()
            .flex()
            .flex_col()
            .gap_2()
            .px_1()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .w_full()
                    .gap_1()
                    .child(remove)
                    .child(visible)
                    .child(reset),
            )
            .when(framed, |el| el.child(size))
            .into_any_element()
    }

    /// The panel's list: the current frame's controls, then every style in
    /// grouped cards (the same cards as Presets).
    pub(crate) fn frames_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(frame) = self.catalog.selected_asset().map(|a| a.develop.frame) else {
            return vec![div()
                .px_3()
                .pt_2()
                .text_size(sp(10.5))
                .text_color(theme.text_faint)
                .child("Select a photo to frame.")
                .into_any_element()];
        };

        // A card in the Presets panel's style: bordered, a raised title
        // band with a count, and an inset body.
        let card = |title: String, count: Option<usize>, body: Vec<AnyElement>| -> AnyElement {
            div()
                .flex()
                .flex_col()
                .mt_2()
                .rounded_md()
                .overflow_hidden()
                .border_1()
                .border_color(theme.border)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .items_center()
                        .px_2()
                        .py_1()
                        .bg(theme.surface_elevated)
                        .child(
                            div()
                                .text_size(sp(10.0))
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(theme.text_muted)
                                .child(title.to_uppercase()),
                        )
                        .children(count.map(|count| {
                            div()
                                .text_size(sp(9.5))
                                .text_color(theme.text_faint)
                                .child(count.to_string())
                        })),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .p_1()
                        .bg(theme.background.opacity(0.45))
                        .children(body.into_iter().enumerate().map(|(i, row)| {
                            div()
                                .when(i > 0, |el| el.border_t_1().border_color(theme.border_subtle))
                                .child(row)
                        })),
                )
                .into_any_element()
        };

        let mut out: Vec<AnyElement> = Vec::new();
        if frame.style != FrameStyle::None {
            let controls = self.frame_controls(&frame, cx);
            if !controls.is_empty() {
                out.push(card(format!("Adjust \u{00B7} {}", frame.style.label()), None, controls));
            }
        }
        for (group, styles) in FRAME_GROUPS {
            let rows: Vec<AnyElement> = styles
                .iter()
                .map(|style| {
                    let style = *style;
                    list_row(
                        SharedString::from(format!("frame-style-{style:?}")),
                        &theme,
                        style.label().to_string(),
                        None,
                        frame.style == style,
                        cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            this.set_frame_style(style, cx)
                        }),
                        None,
                    )
                })
                .collect();
            out.push(card(group.to_string(), Some(styles.len()), rows));
        }
        out.push(
            div()
                .pt_2()
                .text_size(sp(10.0))
                .text_color(theme.text_faint)
                .child("Shown while no editing tool is picked and the photo is fitted; always added on export.")
                .into_any_element(),
        );
        out.push(div().h(sp(12.0)).into_any_element());
        vec![div().flex().flex_col().px_3().children(out).into_any_element()]
    }

    /// The sliders the current style uses, other than Size (which is pinned).
    fn frame_controls(&self, frame: &FrameSettings, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (uses_colour, uses_corner, uses_line, uses_depth) = frame.style.uses();
        let defaults = frame.style.defaults();
        let amount = |default: f32| SliderSpec {
            min: 0.0,
            max: 100.0,
            default,
            signed: false,
            track: SliderTrack::Accent,
        };
        let mut rows = Vec::new();
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
            rows.push(self.frame_slider("corner", "Corners", frame.corner, amount(defaults.corner), |f, v| f.corner = v, cx));
        }
        if uses_line {
            rows.push(self.frame_slider("line", "Line", frame.line, amount(defaults.line), |f, v| f.line = v, cx));
        }
        if uses_depth {
            rows.push(self.frame_slider("depth", "Depth", frame.shadow, amount(defaults.shadow), |f, v| f.shadow = v, cx));
        }
        rows
            .into_iter()
            .map(|row| div().px_1().py_0p5().child(row).into_any_element())
            .collect()
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
                        |d| set(&mut d.frame, value.clamp(spec.min, spec.max)),
                        |d| (String::new(), d.frame.style.label().to_string()),
                        cx,
                    );
                });
            },
        )
        .into_any_element()
    }

    /// Picks a style, starting from its own look.
    fn set_frame_style(&mut self, style: FrameStyle, cx: &mut Context<Self>) {
        self.edit_develop(
            "Frame",
            |d| {
                d.frame = style.defaults();
                // Choosing a frame shows it, even if it had been hidden.
                d.set_section_enabled(DevelopSection::Frames, true);
            },
            |d| (String::new(), d.frame.style.label().to_string()),
            cx,
        );
    }
}
