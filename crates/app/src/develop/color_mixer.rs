//! The Color Mixer section: Hue / Saturation / Luminance tabs, each with a
//! slider per colour range whose track shows the colours it moves between.

use gpui::{div, prelude::*, px, AnyElement, ClickEvent, Context};
use rustyedit_core::{Adjustment, ColorBand, MixerProp};

use crate::workspace::Workspace;

impl Workspace {
    pub(super) fn color_mixer_section(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let prop = self.develop_ui.mixer_prop;

        let tabs = MixerProp::ALL.iter().map(|&tab| {
            let active = tab == prop;
            div()
                .id(("mixer-tab", tab as usize))
                .flex_1()
                .flex()
                .justify_center()
                .h(px(20.0))
                .items_center()
                .rounded_sm()
                .cursor_pointer()
                .text_size(px(10.5))
                .when(active, |el| {
                    el.bg(theme.surface_elevated).text_color(theme.text)
                })
                .when(!active, |el| {
                    el.text_color(theme.text_muted)
                        .hover(|el| el.text_color(theme.text))
                })
                .child(tab.label())
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.develop_ui.mixer_prop = tab;
                    cx.notify();
                }))
        });

        let mut rows = vec![div()
            .flex()
            .flex_row()
            .gap_0p5()
            .p_0p5()
            .mb_1()
            .rounded_md()
            .bg(theme.background)
            .children(tabs)
            .into_any_element()];
        rows.extend(
            ColorBand::ALL
                .iter()
                .map(|band| self.adjustment_slider(Adjustment::Mixer(*band, prop), cx)),
        );
        rows
    }
}
