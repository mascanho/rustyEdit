//! The Effects section: Dehaze, Glow with its style and tuning, Vignette,
//! Grain, and Blemish removal.

use gpui::{div, prelude::*, AnyElement, Context};
use rustyedit_core::{Adjustment, GlowStyle};
use rustyedit_ui::sp;

use super::{dropdown::choice, lens_blur_ui::labelled};
use crate::workspace::Workspace;

impl Workspace {
    pub(super) fn effects_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(develop) = self.catalog.selected_asset().map(|a| a.develop) else {
            return Vec::new();
        };
        let hint = |text: &'static str| {
            div()
                .text_size(sp(rustyedit_ui::type_size::CAPTION))
                .text_color(theme.text_faint)
                .child(text)
                .into_any_element()
        };
        let heading = |text: &'static str| {
            div()
                .pt_1()
                .text_size(sp(rustyedit_ui::type_size::SMALL))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text_muted)
                .child(text)
                .into_any_element()
        };

        let mut rows = vec![self.adjustment_slider(Adjustment::Dehaze, cx)];

        // Glow: the style first, then how much and how it spreads.
        rows.push(heading("Glow"));
        let styles = GlowStyle::ALL
            .iter()
            .map(|&style| {
                choice(style.label(), Some(style_tip(style)), move |this, cx| {
                    this.set_glow_style(style, cx)
                })
            })
            .collect();
        let selected = GlowStyle::ALL.iter().position(|s| *s == develop.glow_style).unwrap_or(0);
        rows.push(labelled(
            "Style",
            self.dropdown("glow-style", develop.glow_style.label(), selected, styles, cx),
            &theme,
        ));
        rows.push(self.adjustment_slider(Adjustment::Glow, cx));
        rows.push(self.adjustment_slider(Adjustment::GlowSize, cx));
        if develop.glow_style.from_highlights() {
            rows.push(self.adjustment_slider(Adjustment::GlowThreshold, cx));
        }

        rows.push(heading("Vignette"));
        for adjustment in [
            Adjustment::Vignette,
            Adjustment::VignetteMidpoint,
            Adjustment::VignetteFeather,
            Adjustment::VignetteRoundness,
        ] {
            rows.push(self.adjustment_slider(adjustment, cx));
        }

        rows.push(heading("Grain"));
        rows.push(self.adjustment_slider(Adjustment::Grain, cx));
        rows.push(self.adjustment_slider(Adjustment::GrainSize, cx));

        rows.push(heading("Blemish removal"));
        rows.push(hint(
            "Blends small red spots on skin into the skin around them. Freckles and moles stay.",
        ));
        rows.push(self.adjustment_slider(Adjustment::BlemishAmount, cx));
        rows.push(self.adjustment_slider(Adjustment::BlemishFade, cx));
        rows
    }

    /// Picks a glow style, turning the glow up from nothing so the choice
    /// shows straight away.
    fn set_glow_style(&mut self, style: GlowStyle, cx: &mut Context<Self>) {
        self.edit_develop(
            "Glow style",
            move |d| {
                d.glow_style = style;
                if d.glow < 0.5 {
                    d.glow = 40.0;
                }
            },
            |d| (String::new(), d.glow_style.label().to_string()),
            cx,
        );
    }
}

fn style_tip(style: GlowStyle) -> &'static str {
    match style {
        GlowStyle::Bloom => "Highlights spill soft light around themselves",
        GlowStyle::Diffusion => "The whole picture softens, as through a pro-mist filter",
        GlowStyle::Halation => "Bright edges glow warm red-orange, like light scattered in film",
        GlowStyle::Dreamy => "A dreamy, saturated softness",
        GlowStyle::Mist => "A low, milky haze: shadows lift and contrast falls",
        GlowStyle::Cinema => "A warm, wide bloom for a cinematic look",
    }
}
