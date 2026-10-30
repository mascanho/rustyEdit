//! Quick Develop, in the Library's Adjust tab: fast fixes without leaving
//! the Library, Lightroom-style. Every control acts on all the selected
//! photos at once (or the one shown), moving each by the same amount from
//! where it is, so a batch keeps its differences:
//!
//! - Tone: Auto (each photo's own auto tone), Presets…, Reset.
//! - White balance: As Shot.
//! - Steps for the main sliders: ‹ › small, « » large (⅓ and 1 stop for
//!   Exposure). Clicking a name resets it. The value shown is the photo on
//!   screen's, or "mixed" when the selection differs.
//!
//! Quick clicks on the same control fold into one undo step.

use gpui::{div, prelude::*, AnyElement, ClickEvent, Context, SharedString};
use rustyedit_core::{Adjustment, DevelopSettings};
use rustyedit_ui::{sp, Icon, Theme};

use super::{
    auto::{section_row, small_button},
    HistoryNote,
};
use crate::workspace::Workspace;

/// A slider with its small and large step.
type Step = (Adjustment, f32, f32);

/// The stepped sliders, in groups.
const GROUPS: [(&str, &[Step]); 3] = [
    (
        "Tone",
        &[
            // Exposure is 50 units a stop.
            (Adjustment::Exposure, 50.0 / 3.0, 50.0),
            (Adjustment::Contrast, 5.0, 20.0),
            (Adjustment::Highlights, 5.0, 20.0),
            (Adjustment::Shadows, 5.0, 20.0),
            (Adjustment::Whites, 5.0, 20.0),
            (Adjustment::Blacks, 5.0, 20.0),
        ],
    ),
    (
        "Colour",
        &[
            (Adjustment::Temperature, 5.0, 15.0),
            (Adjustment::Tint, 5.0, 15.0),
            (Adjustment::Vibrance, 5.0, 20.0),
            (Adjustment::Saturation, 5.0, 20.0),
        ],
    ),
    ("Presence", &[(Adjustment::Clarity, 5.0, 20.0), (Adjustment::Dehaze, 5.0, 20.0)]),
];

/// A value as the slider would show it; Exposure in stops.
fn shown(adjustment: Adjustment, value: f32) -> String {
    let value = value * adjustment.display_sign();
    if adjustment == Adjustment::Exposure {
        return format!("{:+.2} EV", value / 50.0);
    }
    if value.abs() < 0.5 {
        "0".to_string()
    } else {
        format!("{value:+.0}")
    }
}

impl Workspace {
    pub(crate) fn quick_develop_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let targets = self.target_indices();
        let count = targets.len();
        let develops: Vec<DevelopSettings> = targets
            .iter()
            .map(|&ix| self.catalog.assets[ix].develop)
            .collect();
        let Some(&primary) = develops.first() else {
            return Vec::new();
        };
        let busy = self.develop_ui.auto_busy;
        let edited = develops.iter().any(|d| d.is_edited());

        let mut rows = Vec::new();
        if count > 1 {
            rows.push(
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(theme.accent_muted.opacity(0.5))
                    .text_size(sp(10.5))
                    .text_color(theme.text_muted)
                    .child(format!("Changes apply to all {count} selected photos"))
                    .into_any_element(),
            );
        }
        rows.push(section_row(
            "Quick fixes",
            div()
                .flex()
                .flex_row()
                .gap_1()
                .child(small_button(
                    "quick-auto",
                    Icon::Auto,
                    if busy { "Working\u{2026}" } else { "Auto" },
                    "Auto tone each photo from its own content",
                    false,
                    !busy,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| this.run_auto_tone(cx)),
                ))
                .child(small_button(
                    "quick-presets",
                    Icon::Presets,
                    "Presets",
                    "Open the presets to apply one",
                    self.library_presets_open(),
                    true,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| this.toggle_library_presets(cx)),
                ))
                .child(small_button(
                    "quick-reset",
                    Icon::Reset,
                    "Reset",
                    "Back to the original, removing every edit",
                    false,
                    edited,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| this.reset_edits(cx)),
                ))
                .into_any_element(),
            &theme,
        ));
        let as_shot = develops.iter().all(|d| d.temperature == 0.0 && d.tint == 0.0);
        rows.push(section_row(
            "White balance",
            small_button(
                "quick-as-shot",
                Icon::Camera,
                "As Shot",
                "The white balance the camera recorded",
                as_shot,
                !as_shot,
                &theme,
                cx.listener(|this, _: &ClickEvent, _w, cx| {
                    this.quick_set(
                        "White balance",
                        |d| {
                            d.temperature = 0.0;
                            d.tint = 0.0;
                        },
                        "As Shot",
                        cx,
                    )
                }),
            ),
            &theme,
        ));

        for (title, adjustments) in GROUPS {
            rows.push(
                div()
                    .pt_2()
                    .pb_0p5()
                    .text_size(sp(10.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.text_faint)
                    .child(title.to_uppercase())
                    .into_any_element(),
            );
            for &(adjustment, small, large) in adjustments {
                let value = adjustment.get(&primary);
                let mixed = develops.iter().any(|d| (adjustment.get(d) - value).abs() > 0.01);
                rows.push(self.quick_step_row(adjustment, value, mixed, small, large, cx));
            }
        }
        rows
    }

    /// One slider as name, value and four step buttons.
    fn quick_step_row(
        &self,
        adjustment: Adjustment,
        value: f32,
        mixed: bool,
        small: f32,
        large: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let changed = mixed || (value - adjustment.default_value()).abs() > 0.01;
        let label = adjustment.label();
        let step = |id: &str, delta: f32, double: bool, left: bool, tip: String| {
            step_button(
                SharedString::from(format!("quick-{label}-{id}")),
                double,
                left,
                tip,
                &theme,
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                this.quick_step(adjustment, delta, cx)
            }))
        };
        let (small_tip, large_tip) = if adjustment == Adjustment::Exposure {
            ("\u{2153} stop".to_string(), "1 stop".to_string())
        } else {
            (format!("{small:.0}"), format!("{large:.0}"))
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .h(sp(26.0))
            .child(
                div()
                    .id(SharedString::from(format!("quick-{label}-name")))
                    .flex_1()
                    .min_w(sp(0.0))
                    .truncate()
                    .text_size(sp(11.5))
                    .text_color(if changed { theme.text } else { theme.text_muted })
                    .when(changed, |el| {
                        el.cursor_pointer()
                            .hover(|el| el.text_color(theme.accent))
                            .tooltip(rustyedit_ui::tip(format!("Reset {label}"), &theme))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                let neutral = adjustment.default_value();
                                this.quick_set(
                                    adjustment.label(),
                                    move |d| adjustment.set(d, neutral),
                                    "Reset",
                                    cx,
                                )
                            }))
                    })
                    .child(label),
            )
            .child(step("down-large", -large, true, true, format!("\u{2212}{large_tip}")))
            .child(step("down", -small, false, true, format!("\u{2212}{small_tip}")))
            .child(
                div()
                    .w(sp(54.0))
                    .flex()
                    .justify_center()
                    .text_size(sp(10.5))
                    .text_color(if changed { theme.text } else { theme.text_faint })
                    .child(if mixed {
                        "mixed".to_string()
                    } else {
                        shown(adjustment, value)
                    }),
            )
            .child(step("up", small, false, false, format!("+{small_tip}")))
            .child(step("up-large", large, true, false, format!("+{large_tip}")))
            .into_any_element()
    }

    /// Moves `adjustment` by `delta` (as the slider reads) on every target
    /// photo, each from its own value.
    fn quick_step(&mut self, adjustment: Adjustment, delta: f32, cx: &mut Context<Self>) {
        let (min, max) = adjustment.range();
        let stored = delta * adjustment.display_sign();
        let detail = if adjustment == Adjustment::Exposure {
            format!("{:+.2} EV", delta / 50.0)
        } else {
            format!("{delta:+.0}")
        };
        self.quick_set(
            adjustment.label(),
            move |d| {
                let next = (adjustment.get(d) + stored).clamp(min, max);
                adjustment.set(d, next);
            },
            &detail,
            cx,
        );
    }

    /// Applies `edit` to every target photo as one (coalescing) undo step.
    fn quick_set(
        &mut self,
        label: &str,
        edit: impl Fn(&mut DevelopSettings),
        detail: &str,
        cx: &mut Context<Self>,
    ) {
        let updates: Vec<(usize, DevelopSettings)> = self
            .target_indices()
            .into_iter()
            .map(|ix| {
                let mut develop = self.catalog.assets[ix].develop;
                edit(&mut develop);
                (ix, develop)
            })
            .collect();
        self.replace_develop_each_and(
            updates,
            &format!("Quick develop: {label}"),
            HistoryNote::Record {
                title: label,
                detail,
            },
            true,
            cx,
            |_, _| {},
        );
    }
}

/// A small square button with one chevron (a small step) or two (large).
fn step_button(
    id: SharedString,
    double: bool,
    left: bool,
    tip: String,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let icon = if left {
        Icon::ChevronLeft
    } else {
        Icon::ChevronRight
    };
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .justify_center()
        .w(sp(if double { 24.0 } else { 20.0 }))
        .h(sp(20.0))
        .rounded_md()
        .border_1()
        .border_color(theme.border_subtle)
        .cursor_pointer()
        .hover(|el| el.bg(theme.accent_muted).border_color(theme.accent))
        .child(rustyedit_ui::icon(icon, 10.0, theme.text_muted))
        .when(double, |el| {
            el.child(div().ml(sp(-5.0)).child(rustyedit_ui::icon(icon, 10.0, theme.text_muted)))
        })
        .tooltip(rustyedit_ui::tip(tip, theme))
}
