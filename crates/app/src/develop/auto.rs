//! One-click starting points in Develop:
//!
//! - Auto Tone (the Basic section's Auto button, ⌘U) sets Exposure,
//!   Contrast, Highlights, Shadows, Whites and Blacks from the photo
//!   itself. With several photos selected it does each of them, as one
//!   undo step.
//! - The white-balance eyedropper (the Color section's pipette, W) sets
//!   Temperature and Tint so whatever is clicked becomes neutral grey.
//!
//! Both measure renders of the photo (see `rustyedit_processing::auto`),
//! so they run on a background thread.

use rustyedit_ui::sp;
use gpui::{
    div, prelude::*, AnyElement, App, ClickEvent, Context, MouseButton, MouseDownEvent,
    Pixels, Point, SharedString, Window,
};
use rustyedit_core::{DevelopSection, DevelopSettings};
use rustyedit_processing::DevelopSource;
use rustyedit_ui::{Icon, Theme};

use super::tool_strip::PhotoTool;
use crate::{
    actions::AutoTone,
    develop::HistoryNote,
    workspace::{Mode, Workspace},
};

/// The six sliders Auto Tone sets.
fn with_tone(own: &DevelopSettings, tuned: &DevelopSettings) -> DevelopSettings {
    let mut out = *own;
    out.exposure = tuned.exposure;
    out.contrast = tuned.contrast;
    out.highlights = tuned.highlights;
    out.shadows = tuned.shadows;
    out.whites = tuned.whites;
    out.blacks = tuned.blacks;
    out.set_section_enabled(DevelopSection::Basic, true);
    out
}

impl Workspace {
    pub fn auto_tone_action(&mut self, _: &AutoTone, _w: &mut Window, cx: &mut Context<Self>) {
        self.run_auto_tone(cx);
    }

    /// Auto Tone on the target photos. The photo open in Develop uses its
    /// already decoded copy; any others are decoded in the background.
    pub(crate) fn run_auto_tone(&mut self, cx: &mut Context<Self>) {
        if self.develop_ui.auto_busy || self.is_importing || self.mode == Mode::Export {
            return;
        }
        let jobs: Vec<(usize, u64, std::path::PathBuf, DevelopSettings, Option<DevelopSource>)> =
            self.target_indices()
                .into_iter()
                .filter_map(|ix| {
                    let asset = self.catalog.assets.get(ix).filter(|a| !a.missing)?;
                    let decoded = self
                        .source_image
                        .clone()
                        .filter(|_| self.source_image_for == Some(asset.id));
                    Some((ix, asset.id, asset.path.clone(), asset.develop, decoded))
                })
                .collect();
        if jobs.is_empty() {
            return;
        }
        let count = jobs.len();
        self.develop_ui.auto_busy = true;
        self.status_message = Some(if count == 1 {
            "Auto tone\u{2026}".into()
        } else {
            format!("Auto tone on {count} photos\u{2026}").into()
        });
        cx.notify();

        let work = cx.background_executor().spawn(async move {
            jobs.into_iter()
                .filter_map(|(ix, id, path, settings, decoded)| {
                    let source = match decoded {
                        Some(source) => source,
                        None => rustyedit_io::open_for_develop(&path).ok()?,
                    };
                    let tuned = rustyedit_processing::auto_tone(&source, &settings);
                    Some((ix, id, path, tuned))
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let results = work.await;
            this.update(cx, |this, cx| {
                this.develop_ui.auto_busy = false;
                // Only photos that are still where they were.
                let updates: Vec<(usize, DevelopSettings)> = results
                    .into_iter()
                    .filter_map(|(ix, id, path, tuned)| {
                        let asset = this
                            .catalog
                            .assets
                            .get(ix)
                            .filter(|a| a.id == id && a.path == path)?;
                        Some((ix, with_tone(&asset.develop, &tuned)))
                    })
                    .collect();
                let done = updates.len();
                let summary = updates
                    .first()
                    .filter(|_| done == 1)
                    .map(|(_, d)| tone_summary(d))
                    .unwrap_or_else(|| format!("{done} photos"));
                this.replace_develop_each(
                    updates,
                    "auto tone",
                    HistoryNote::Record {
                        title: "Auto tone",
                        detail: &summary,
                    },
                    cx,
                );
                this.status_message = Some(match done {
                    0 => "Auto tone couldn't read the photo.".into(),
                    1 => format!("Auto tone: {summary}").into(),
                    n => format!("Auto tone applied to {n} photos.").into(),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Sets white balance so the photo is neutral at `at` (fractions across
    /// and down the photo as shown).
    fn pick_white_balance(&mut self, at: (f32, f32), cx: &mut Context<Self>) {
        if self.develop_ui.auto_busy {
            return;
        }
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        let (ix, id, settings) = (self.catalog.selected, asset.id, asset.develop);
        let Some(source) = self
            .source_image
            .clone()
            .filter(|_| self.source_image_for == Some(id))
        else {
            self.status_message = Some("Wait for the photo to finish loading.".into());
            cx.notify();
            return;
        };
        self.develop_ui.auto_busy = true;
        self.status_message = Some("Measuring white balance\u{2026}".into());
        cx.notify();

        let work = cx.background_executor().spawn(async move {
            rustyedit_processing::neutral_white_balance(&source, &settings, at)
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| {
                this.develop_ui.auto_busy = false;
                if this.catalog.selected != ix
                    || this.catalog.selected_asset().map(|a| a.id) != Some(id)
                {
                    return;
                }
                match result {
                    Ok(balance) => {
                        let detail = format!(
                            "Temp {:+.0}, Tint {:+.0}",
                            balance.temperature, balance.tint
                        );
                        this.edit_develop(
                            "White balance",
                            |d| {
                                d.temperature = balance.temperature;
                                d.tint = balance.tint;
                                d.set_section_enabled(DevelopSection::Color, true);
                            },
                            |_| (String::new(), detail.clone()),
                            cx,
                        );
                        this.status_message = Some(format!("White balance: {detail}").into());
                        // Done: put the eyedropper down, as Lightroom does.
                        if this.develop_ui.wb_picker {
                            this.toggle_photo_tool(PhotoTool::WhiteBalance, cx);
                        }
                    }
                    Err(err) => this.status_message = Some(err.message().into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn white_balance_click(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(size) = self.develop_image_size() else {
            return;
        };
        let Some(rect) = self.develop_ui.zoom.photo_rect(size) else {
            return;
        };
        if !rect.contains(&position) {
            return;
        }
        let at = (
            f32::from(position.x - rect.origin.x) / f32::from(rect.size.width),
            f32::from(position.y - rect.origin.y) / f32::from(rect.size.height),
        );
        self.pick_white_balance(at, cx);
    }

    /// While the eyedropper is picked: a crosshair over the photo that
    /// samples where it's clicked, and a hint at the top.
    pub(super) fn render_white_balance_overlay(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.develop_ui.wb_picker || self.develop_ui.zoom.space_held {
            return None;
        }
        let theme = self.theme;
        let hint = if self.develop_ui.auto_busy {
            "Measuring\u{2026}"
        } else {
            "Click something that should be neutral grey or white \u{00B7} Esc to cancel"
        };
        Some(
            div()
                .id("wb-overlay")
                .absolute()
                .inset_0()
                .cursor_crosshair()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        this.white_balance_click(event.position, cx);
                    }),
                )
                .child(
                    div()
                        .absolute()
                        .top(sp(8.0))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1p5()
                                .h(sp(24.0))
                                .px_2()
                                .rounded_md()
                                .bg(theme.surface.opacity(0.9))
                                .border_1()
                                .border_color(theme.border)
                                .text_size(sp(11.0))
                                .text_color(theme.text)
                                .child(rustyedit_ui::icon(Icon::Eyedropper, 12.0, theme.accent))
                                .child(hint),
                        ),
                )
                .into_any_element(),
        )
    }

    /// The Basic section's first row: "Tone" and the Auto button.
    pub(super) fn auto_tone_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let busy = self.develop_ui.auto_busy;
        section_row(
            "Tone",
            small_button(
                "develop-auto-tone",
                Icon::Auto,
                if busy { "Working\u{2026}" } else { "Auto" },
                "Set the tone sliders from this photo (\u{2318}U)",
                false,
                !busy,
                &theme,
                cx.listener(|this, _: &ClickEvent, _w, cx| this.run_auto_tone(cx)),
            ),
            &theme,
        )
    }

    /// The Color section's first row: "White balance" and the eyedropper.
    pub(super) fn white_balance_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let picking = self.develop_ui.wb_picker;
        section_row(
            "White balance",
            small_button(
                "develop-wb-picker",
                Icon::Eyedropper,
                "Pick",
                "Click something neutral in the photo to set white balance (W)",
                picking,
                true,
                &theme,
                cx.listener(|this, _: &ClickEvent, _w, cx| {
                    this.toggle_photo_tool(PhotoTool::WhiteBalance, cx)
                }),
            ),
            &theme,
        )
    }
}

/// "Exposure +12 · Contrast +8 · ..." for the sliders Auto Tone moved.
fn tone_summary(d: &DevelopSettings) -> String {
    let parts: Vec<String> = [
        ("Exposure", d.exposure),
        ("Contrast", d.contrast),
        ("Highlights", d.highlights),
        ("Shadows", d.shadows),
        ("Whites", d.whites),
        ("Blacks", d.blacks),
    ]
    .into_iter()
    .filter(|(_, v)| v.abs() >= 0.5)
    .map(|(name, v)| format!("{name} {v:+.0}"))
    .collect();
    if parts.is_empty() {
        "no change needed".to_string()
    } else {
        parts.join(" \u{00B7} ")
    }
}

/// A label on the left and a control on the right.
fn section_row(label: &'static str, control: AnyElement, theme: &Theme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .text_xs()
        .child(div().text_color(theme.text_muted).child(label))
        .child(control)
        .into_any_element()
}

/// A compact icon-and-text button; `active` shows it pressed.
#[allow(clippy::too_many_arguments)]
fn small_button(
    id: &'static str,
    icon: Icon,
    label: &'static str,
    tooltip: &'static str,
    active: bool,
    enabled: bool,
    theme: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let theme = *theme;
    let color = if active { theme.text } else { theme.text_muted };
    div()
        .id(SharedString::from(id))
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .h(sp(20.0))
        .px_1p5()
        .rounded_md()
        .border_1()
        .border_color(if active { theme.accent } else { theme.border })
        .when(active, |el| el.bg(theme.accent_muted))
        .text_size(sp(11.0))
        .text_color(color)
        .child(rustyedit_ui::icon(icon, 11.0, color))
        .child(label)
        .tooltip(rustyedit_ui::tip(tooltip, &theme))
        .when(enabled, |el| {
            el.cursor_pointer()
                .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                .on_click(on_click)
        })
        .when(!enabled, |el| el.opacity(0.5))
        .into_any_element()
}
