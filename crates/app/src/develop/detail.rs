//! The Detail section (clarity, sharpening with radius and edge masking,
//! noise reduction) and the Lens Corrections section (distortion,
//! vignetting, chromatic aberration, with a profile read from the photo's
//! lens EXIF).

use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context};
use rustyedit_core::{lens_profile, Adjustment};

use crate::workspace::Workspace;

impl Workspace {
    pub(super) fn detail_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let heading = |text: &'static str| {
            div()
                .pt_2()
                .text_size(px(10.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(theme.text_faint)
                .child(text)
                .into_any_element()
        };
        let mut rows = self.adjustment_sliders(
            &[
                Adjustment::Clarity,
                Adjustment::Sharpening,
                Adjustment::SharpenRadius,
                Adjustment::SharpenMasking,
            ],
            cx,
        );
        rows.push(
            div()
                .text_size(px(10.0))
                .text_color(theme.text_faint)
                .child("Masking holds sharpening back from flat areas, so only edges are sharpened and noise isn't.")
                .into_any_element(),
        );
        rows.push(heading("NOISE REDUCTION"));
        rows.extend(self.adjustment_sliders(&[Adjustment::NoiseLuma, Adjustment::NoiseColor], cx));
        rows
    }

    pub(super) fn lens_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(asset) = self.catalog.selected_asset() else {
            return Vec::new();
        };
        let lens = asset.meta.lens.trim().to_string();
        let focal = asset.meta.focal_mm;
        let profile = lens_profile(&lens, focal);

        let lens_line = if lens.is_empty() {
            "No lens recorded in this photo".to_string()
        } else if focal > 0.0 {
            format!("{lens} \u{00B7} {focal:.0} mm")
        } else {
            lens.clone()
        };

        let mut rows: Vec<AnyElement> = vec![div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .child(lens_line),
            )
            .into_any_element()];

        match profile {
            Some(profile) => {
                let correction = profile.correction;
                let name = profile.name.clone();
                rows.push(
                    div()
                        .id("lens-apply-profile")
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .px_2()
                        .py_1p5()
                        .cursor_pointer()
                        .border_1()
                        .border_color(theme.accent.opacity(0.5))
                        .bg(theme.accent.opacity(0.08))
                        .hover(|el| el.bg(theme.accent.opacity(0.16)).border_color(theme.accent))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(theme.accent)
                                .child("Apply lens profile"),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(theme.text_faint)
                                .child(if profile.exact {
                                    profile.name.clone()
                                } else {
                                    format!("{} (lens not listed)", profile.name)
                                }),
                        )
                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            let detail = name.clone();
                            this.edit_develop(
                                "Lens profile",
                                |d| {
                                    d.lens_distortion = correction.distortion;
                                    d.lens_vignetting = correction.vignetting;
                                    d.lens_ca = correction.chromatic_aberration;
                                },
                                move |_| (String::new(), detail),
                                cx,
                            );
                        }))
                        .into_any_element(),
                );
            }
            None => rows.push(
                div()
                    .text_size(px(10.0))
                    .text_color(theme.text_faint)
                    .child("No lens or focal length in the photo's EXIF, so there's no profile to apply. Set the sliders by hand.")
                    .into_any_element(),
            ),
        }

        rows.extend(self.adjustment_sliders(&Adjustment::LENS, cx));
        rows.push(
            div()
                .text_size(px(10.0))
                .text_color(theme.text_faint)
                .child("Distortion: positive straightens bulging edges. Vignetting: positive brightens the corners. Profiles are approximate; adjust to taste.")
                .into_any_element(),
        );
        rows
    }
}
