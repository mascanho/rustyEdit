//! The Crop & Straighten section: shape presets and the straightening
//! angle. The crop rectangle itself is changed by dragging on the photo.

use rustyedit_ui::sp;
use gpui::{div, prelude::*, AnyElement, ClickEvent, Context, SharedString};
use rustyedit_core::{AspectLock, MAX_PERSPECTIVE, MAX_STRAIGHTEN};
use rustyedit_ui::{SliderSpec, SliderTrack};

use crate::workspace::Workspace;

impl Workspace {
    pub(super) fn crop_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(asset) = self.catalog.selected_asset() else {
            return Vec::new();
        };
        let crop = asset.develop.crop;

        let aspects = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_1()
            .children(
                AspectLock::PRESETS
                    .iter()
                    .map(|(label, aspect)| self.crop_aspect_button(*label, *aspect, crop.aspect, cx)),
            );

        let spec = SliderSpec {
            min: -MAX_STRAIGHTEN,
            max: MAX_STRAIGHTEN,
            default: 0.0,
            signed: true,
            track: SliderTrack::Accent,
        };
        let entity = cx.entity();
        let straighten = rustyedit_ui::labeled_slider(
            "crop-straighten",
            "Straighten",
            crop.angle,
            spec,
            &self.theme,
            move |angle, _window, cx| {
                entity.update(cx, |this, cx| this.set_crop_angle(angle, cx));
            },
        );

        let upright = self.upright_row(cx);
        let perspective_spec = SliderSpec {
            min: -MAX_PERSPECTIVE,
            max: MAX_PERSPECTIVE,
            default: 0.0,
            signed: true,
            track: SliderTrack::Accent,
        };
        let perspective = |id: &'static str, label: &'static str, value: f32, vertical: bool| {
            let entity = cx.entity();
            rustyedit_ui::labeled_slider(
                id,
                label,
                value,
                perspective_spec,
                &self.theme,
                move |value, _window, cx| {
                    entity.update(cx, |this, cx| {
                        this.set_crop_perspective(vertical, value, cx)
                    });
                },
            )
            .into_any_element()
        };

        vec![
            upright,
            aspects.into_any_element(),
            straighten.into_any_element(),
            perspective("crop-vertical", "Vertical", crop.vertical, true),
            perspective("crop-horizontal", "Horizontal", crop.horizontal, false),
        ]
    }

    fn crop_aspect_button(
        &self,
        label: &'static str,
        aspect: AspectLock,
        current: AspectLock,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let active = aspect == current;
        div()
            .id(SharedString::from(format!("crop-aspect-{label}")))
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .h(sp(24.0))
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
            .child(label)
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                this.set_crop_aspect(aspect, cx);
            }))
            .into_any_element()
    }

    fn set_crop_aspect(&mut self, aspect: AspectLock, cx: &mut Context<Self>) {
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        let (width, height) = (asset.width.max(1) as f32, asset.height.max(1) as f32);
        self.edit_develop(
            "Crop Aspect",
            |develop| {
                develop.crop.aspect = aspect;
                develop.crop = develop.crop.reframed(width, height).clamped();
            },
            |_| (String::new(), format!("{aspect:?}")),
            cx,
        );
    }

    fn set_crop_angle(&mut self, angle: f32, cx: &mut Context<Self>) {
        let Some(before) = self
            .catalog
            .selected_asset()
            .map(|asset| asset.develop.crop.angle)
        else {
            return;
        };
        self.edit_develop(
            "Straighten",
            |develop| {
                develop.crop.angle = angle.clamp(-MAX_STRAIGHTEN, MAX_STRAIGHTEN);
                develop.crop = develop.crop.clamped();
            },
            |develop| {
                (
                    format!("{:+.1}\u{b0}", before),
                    format!("{:+.1}\u{b0}", develop.crop.angle),
                )
            },
            cx,
        );
    }

    /// Sets the vertical or horizontal perspective correction. The crop is
    /// left where it is, so the photo isn't zoomed; empty corners show black.
    fn set_crop_perspective(
        &mut self,
        vertical: bool,
        value: f32,
        cx: &mut Context<Self>,
    ) {
        let Some(before) = self.catalog.selected_asset().map(|asset| {
            let crop = asset.develop.crop;
            if vertical { crop.vertical } else { crop.horizontal }
        }) else {
            return;
        };
        let value = value.clamp(-MAX_PERSPECTIVE, MAX_PERSPECTIVE);
        self.edit_develop(
            if vertical { "Vertical Perspective" } else { "Horizontal Perspective" },
            |develop| {
                if vertical {
                    develop.crop.vertical = value;
                } else {
                    develop.crop.horizontal = value;
                }
                develop.crop = develop.crop.clamped();
            },
            |_| (format!("{before:+.0}"), format!("{value:+.0}")),
            cx,
        );
    }
}
