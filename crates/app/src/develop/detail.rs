//! The Detail section (clarity, sharpening with radius and edge masking,
//! noise reduction) and the Lens Corrections section (distortion,
//! vignetting, chromatic aberration, with a profile read from the photo's
//! lens EXIF).

use rustyedit_ui::sp;
use gpui::{div, prelude::*, AnyElement, ClickEvent, Context};
use rustyedit_core::{lens_profile, Adjustment};

use crate::workspace::Workspace;

impl Workspace {
    pub(super) fn detail_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let heading = |text: &'static str| {
            div()
                .pt_2()
                .text_size(sp(10.0))
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
                .text_size(sp(10.0))
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
        // Measured data from a Lensfun database when there is one, else the
        // built-in estimates.
        let aspect = if asset.width > 0 && asset.height > 0 {
            asset.width.max(asset.height) as f32 / asset.width.min(asset.height) as f32
        } else {
            1.5
        };
        let db_folder = self.settings.develop.lens_db_folder.clone();
        let profile = rustyedit_io::find_lens_profile(db_folder.as_deref(), &lens, focal, aspect)
            .or_else(|| lens_profile(&lens, focal));
        let db_size = rustyedit_io::lens_database_size(db_folder.as_deref());

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
                    .text_size(sp(11.0))
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
                                .text_size(sp(11.5))
                                .text_color(theme.accent)
                                .child("Apply lens profile"),
                        )
                        .child(
                            div()
                                .text_size(sp(10.0))
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
                    .text_size(sp(10.0))
                    .text_color(theme.text_faint)
                    .child("No lens or focal length in the photo's EXIF, so there's no profile to apply. Set the sliders by hand.")
                    .into_any_element(),
            ),
        }

        rows.extend(self.adjustment_sliders(&Adjustment::LENS, cx));
        // Where the measured profiles come from.
        rows.push(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_2()
                .pt_1()
                .child(
                    div()
                        .flex_1()
                        .min_w(rustyedit_ui::sp(0.0))
                        .truncate()
                        .text_size(rustyedit_ui::sp(10.0))
                        .text_color(theme.text_faint)
                        .child(if db_size > 0 {
                            format!("Lensfun database: {db_size} lenses")
                        } else {
                            "No Lensfun database found; using built-in estimates".to_string()
                        }),
                )
                .child(rustyedit_ui::chrome_button(
                    "lens-db-choose",
                    "Database\u{2026}",
                    false,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| this.choose_lens_database(cx)),
                ))
                .into_any_element(),
        );
        rows.push(
            div()
                .text_size(sp(10.0))
                .text_color(theme.text_faint)
                .child("Distortion: positive straightens bulging edges. Vignetting: positive brightens the corners. Profiles are approximate; adjust to taste.")
                .into_any_element(),
        );
        rows
    }
}

impl Workspace {
    /// Asks for a folder of Lensfun XML files (the `data/db` folder of a
    /// Lensfun download, say) and remembers it.
    fn choose_lens_database(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let folder = cx
                .background_executor()
                .spawn(async {
                    rfd::FileDialog::new()
                        .set_title("Choose the folder holding the Lensfun XML files")
                        .pick_folder()
                })
                .await;
            let Some(folder) = folder else { return };
            // Read it off the UI thread so the section doesn't stall.
            let probe = folder.clone();
            let lenses = cx
                .background_executor()
                .spawn(async move { rustyedit_io::lens_database_size(Some(&probe)) })
                .await;
            this.update(cx, |this, cx| {
                if lenses == 0 {
                    this.status_message =
                        Some("No Lensfun lens data found in that folder.".into());
                } else {
                    this.status_message =
                        Some(format!("Lens database: {lenses} lenses.").into());
                    this.update_settings(|s| s.develop.lens_db_folder = Some(folder), cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
