//! The Color Grading section: a hue and strength for the shadows,
//! midtones and highlights, each headed by a swatch of its tint, and the
//! balance between them. A histogram of the photo sits above them, so the
//! levels being tinted can be seen while they are graded.

use gpui::{div, prelude::*, px, AnyElement, Context};
use rustyedit_core::{Adjustment, GradeProp, HistogramChannel, ToneZone};
use rustyedit_ui::HistogramProps;

use super::adjustments::wheel;
use crate::workspace::Workspace;

/// Height of the histogram strip above the levels.
const HISTOGRAM_STRIP: f32 = 64.0;

impl Workspace {
    pub(super) fn color_grading_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(grading) = self.catalog.selected_asset().map(|a| a.develop.grading) else {
            return Vec::new();
        };

        // The live histogram of the photo, so the levels being graded can
        // be read against the tones they cover.
        let (histogram, _) = self.current_histogram();

        let mut rows = vec![rustyedit_ui::histogram_view_with_height(
            &theme,
            HistogramProps {
                histogram,
                channel: HistogramChannel::Rgb,
                log_scale: false,
                hover: None,
                zone: None,
                bounds: None,
            },
            HISTOGRAM_STRIP,
        )
        .into_any_element()];
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
