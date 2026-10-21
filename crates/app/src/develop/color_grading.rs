//! The Color Grading section: a hue and strength for the shadows,
//! midtones and highlights, each headed by a swatch of its tint, and the
//! balance between them.

use gpui::{div, prelude::*, px, AnyElement, Context};
use rustyedit_core::{Adjustment, GradeProp, ToneZone};

use super::adjustments::wheel;
use crate::workspace::Workspace;

impl Workspace {
    pub(super) fn color_grading_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(grading) = self.catalog.selected_asset().map(|a| a.develop.grading) else {
            return Vec::new();
        };

        let mut rows = Vec::new();
        for zone in ToneZone::ALL {
            let grade = grading.zone(zone);
            let tinted = grade.saturation > 0.0;
            rows.push(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .pt_2()
                    .child(
                        div()
                            .size(px(12.0))
                            .rounded_full()
                            .border_1()
                            .border_color(theme.border)
                            .bg(if tinted {
                                wheel(grade.hue, grade.saturation / 100.0, 0.5)
                            } else {
                                theme.background
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_muted)
                            .child(zone.label()),
                    )
                    .into_any_element(),
            );
            rows.push(self.adjustment_slider(Adjustment::Grade(zone, GradeProp::Hue), cx));
            rows.push(self.adjustment_slider(Adjustment::Grade(zone, GradeProp::Saturation), cx));
        }
        rows.push(div().h(px(4.0)).into_any_element());
        rows.push(self.adjustment_slider(Adjustment::GradingBalance, cx));
        rows
    }
}
