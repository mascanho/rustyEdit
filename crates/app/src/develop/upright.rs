//! Auto upright and auto crop, from the Crop & Straighten section.
//!
//! The photo is measured as it looks with only its rotation and flips
//! applied (no crop, turn or perspective), on a background thread: its
//! strong edges say how far it's turned and how its verticals and
//! horizontals converge (see `rustyedit_processing::upright`), and those
//! become the Straighten, Vertical and Horizontal sliders.

use gpui::{div, prelude::*, AnyElement, ClickEvent, Context, SharedString};
use rustyedit_core::DevelopSettings;
use rustyedit_processing::{ClipWarnings, UprightMode};
use rustyedit_ui::sp;

use crate::workspace::Workspace;

/// What a button of the Auto row does.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum UprightAction {
    /// Level, make verticals and horizontals parallel, and trim the corners.
    Auto,
    Level,
    Vertical,
    /// Only trim the empty corners.
    Crop,
}

impl UprightAction {
    fn label(&self) -> &'static str {
        match self {
            UprightAction::Auto => "Auto",
            UprightAction::Level => "Level",
            UprightAction::Vertical => "Vertical",
            UprightAction::Crop => "Auto crop",
        }
    }

    fn tip(&self) -> &'static str {
        match self {
            UprightAction::Auto => "Level the photo, straighten its verticals and horizontals, and crop the empty corners",
            UprightAction::Level => "Level the horizon or verticals (Straighten only)",
            UprightAction::Vertical => "Level, and make converging verticals parallel",
            UprightAction::Crop => "Crop away the empty corners left by straightening or perspective",
        }
    }
}

impl Workspace {
    /// The row of Auto buttons at the top of the Crop section.
    pub(super) fn upright_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let busy = self.develop_ui.auto_busy;
        let buttons = [
            UprightAction::Auto,
            UprightAction::Level,
            UprightAction::Vertical,
            UprightAction::Crop,
        ]
        .map(|action| {
            div()
                .id(SharedString::from(format!("upright-{}", action.label())))
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .h(sp(26.0))
                .rounded_md()
                .border_1()
                .border_color(if action == UprightAction::Auto {
                    theme.accent
                } else {
                    theme.border_subtle
                })
                .bg(if action == UprightAction::Auto {
                    theme.accent_muted
                } else {
                    theme.background
                })
                .text_size(sp(11.0))
                .text_color(if action == UprightAction::Auto {
                    theme.text
                } else {
                    theme.text_muted
                })
                .when(!busy, |el| {
                    el.cursor_pointer()
                        .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                })
                .when(busy, |el| el.opacity(0.5))
                .tooltip(rustyedit_ui::tip(action.tip(), &theme))
                .child(action.label())
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.run_upright(action, cx);
                }))
        });
        div()
            .flex()
            .flex_row()
            .gap_1()
            .children(buttons)
            .into_any_element()
    }

    fn run_upright(&mut self, action: UprightAction, cx: &mut Context<Self>) {
        if self.develop_ui.auto_busy {
            return;
        }
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        if action == UprightAction::Crop {
            self.auto_crop(cx);
            return;
        }
        let Some(source) = self
            .source_image
            .clone()
            .filter(|_| self.source_image_for == Some(asset.id))
        else {
            self.status_message = Some("The photo is still loading.".into());
            cx.notify();
            return;
        };
        let (id, develop) = (asset.id, asset.develop);
        let geometry = DevelopSettings {
            rotation_degrees: develop.rotation_degrees,
            flip_horizontal: develop.flip_horizontal,
            flip_vertical: develop.flip_vertical,
            ..DevelopSettings::default()
        };
        self.develop_ui.auto_busy = true;
        self.status_message = Some("Finding lines\u{2026}".into());
        cx.notify();

        let work = cx.background_executor().spawn(async move {
            let frame = rustyedit_processing::render_preview(&source, &geometry, ClipWarnings::default());
            // BGRA in a container, to RGB.
            let bgra = frame.bgra;
            let rgb = image::RgbImage::from_fn(bgra.width(), bgra.height(), |x, y| {
                let p = bgra.get_pixel(x, y).0;
                image::Rgb([p[2], p[1], p[0]])
            });
            rustyedit_processing::detect_upright(&rgb)
        });
        cx.spawn(async move |this, cx| {
            let found = work.await;
            this.update(cx, |this, cx| {
                this.develop_ui.auto_busy = false;
                // Still the same photo?
                if this.catalog.selected_asset().map(|a| a.id) != Some(id) {
                    return;
                }
                let sure = found.vertical_confidence.max(found.horizontal_confidence) >= 0.2;
                if !sure {
                    this.status_message = Some("Couldn't find straight lines to level on.".into());
                    cx.notify();
                    return;
                }
                let mode = match action {
                    UprightAction::Level => UprightMode::Level,
                    UprightAction::Vertical => UprightMode::Vertical,
                    _ => UprightMode::Full,
                };
                let (width, height) = this
                    .catalog
                    .selected_asset()
                    .map(|a| (a.width.max(1) as f32, a.height.max(1) as f32))
                    .unwrap_or((1.0, 1.0));
                let trim = action == UprightAction::Auto;
                this.edit_develop(
                    "Auto upright",
                    |d| {
                        // Starting from the whole photo, so the old crop
                        // doesn't hide what the new angle needs.
                        let mut crop = found.applied_to(rustyedit_core::Crop { rect: (0.0, 0.0, 1.0, 1.0), ..d.crop }, mode);
                        if trim {
                            crop = crop.reframed(width, height);
                        }
                        d.crop = crop.clamped();
                    },
                    |d| {
                        (
                            String::new(),
                            format!(
                                "{:+.1}\u{b0}, vertical {:+.0}, horizontal {:+.0}",
                                d.crop.angle, d.crop.vertical, d.crop.horizontal
                            ),
                        )
                    },
                    cx,
                );
                this.status_message = Some(
                    format!(
                        "Upright: {:+.1}\u{b0}, vertical {:+.0}, horizontal {:+.0}",
                        found.angle,
                        if mode == UprightMode::Level { 0.0 } else { found.vertical },
                        if mode == UprightMode::Full { found.horizontal } else { 0.0 },
                    )
                    .into(),
                );
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Crops the empty corners straightening or perspective left (keeping
    /// the aspect lock, if there is one).
    fn auto_crop(&mut self, cx: &mut Context<Self>) {
        let Some(asset) = self.catalog.selected_asset() else {
            return;
        };
        let (width, height) = (asset.width.max(1) as f32, asset.height.max(1) as f32);
        if asset.develop.crop.angle == 0.0 && !asset.develop.crop.has_perspective() {
            self.status_message = Some("Nothing to crop away.".into());
            cx.notify();
            return;
        }
        self.edit_develop(
            "Auto crop",
            |d| d.crop = d.crop.reframed(width, height).clamped(),
            |_| (String::new(), "corners trimmed".to_string()),
            cx,
        );
    }
}
