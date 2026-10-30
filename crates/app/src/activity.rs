//! Background activity in the status bar: whatever is running (an import,
//! an export, preview thumbnails being re-rendered, culling measuring
//! sharpness or exposure, a duplicate search, photo analysis) shows as a
//! chip with its progress. Several at once show the busiest and "+N";
//! clicking the chip lists them all, with Cancel for an export.
//!
//! Progress is read straight from the jobs' own counters each frame, and
//! while anything runs a light ticker repaints a few times a second so it
//! moves. Nothing is tracked when nothing runs. Settings ▸ Appearance ▸
//! Background activity switches the chip off.

use std::{sync::atomic::Ordering, time::Duration};

use gpui::{
    div, prelude::*, px, Animation, AnimationExt, AnyElement, ClickEvent, Context, MouseButton,
    SharedString,
};
use rustyedit_ui::{sp, Icon, Theme};

use crate::workspace::Workspace;

/// Fewer re-renders pending than this are routine (one edit) and not shown.
const THUMBNAIL_THRESHOLD: usize = 4;
/// How often progress repaints while something runs.
const TICK: Duration = Duration::from_millis(250);

/// One running job, as shown.
pub(crate) struct Activity {
    id: &'static str,
    icon: Icon,
    title: &'static str,
    /// What and how far, e.g. "120 of 800 photos".
    detail: String,
    /// 0..=1 when the size of the job is known.
    progress: Option<f32>,
    /// Exports can be stopped.
    cancellable: bool,
}

fn of(done: usize, total: usize, noun: &str) -> (String, Option<f32>) {
    let total = total.max(1);
    (
        format!("{} of {total} {noun}", done.min(total)),
        Some((done as f32 / total as f32).clamp(0.0, 1.0)),
    )
}

impl Workspace {
    /// Everything running now, the most important first.
    pub(crate) fn activities(&self) -> Vec<Activity> {
        let mut out = Vec::new();
        if self.is_importing {
            let (detail, progress) = match &self.import_progress {
                Some((done, total)) => of(done.load(Ordering::Relaxed), *total, "thumbnails"),
                None => ("Scanning folders\u{2026}".to_string(), None),
            };
            out.push(Activity {
                id: "import",
                icon: Icon::Download,
                title: "Importing",
                detail,
                progress,
                cancellable: false,
            });
        }
        if let Some((done, total)) = self.export_run.as_ref().and_then(|r| r.progress()) {
            let (detail, progress) = of(done, total, "photos");
            out.push(Activity {
                id: "export",
                icon: Icon::Upload,
                title: "Exporting",
                detail,
                progress,
                cancellable: true,
            });
        }
        if self.develop_ui.auto_busy {
            out.push(Activity {
                id: "analysis",
                icon: Icon::Auto,
                title: "Analysing photos",
                detail: "Auto tone and camera matching".to_string(),
                progress: None,
                cancellable: false,
            });
        }
        if let Some((done, total)) = &self.sharpness_progress {
            let (detail, progress) = of(done.load(Ordering::Relaxed), *total, "photos");
            out.push(Activity {
                id: "sharpness",
                icon: Icon::ZoomIn,
                title: "Measuring sharpness",
                detail,
                progress,
                cancellable: false,
            });
        }
        if self.exposure_scan_busy && self.exposure_left > 0 {
            out.push(Activity {
                id: "exposure",
                icon: Icon::ClipHighlights,
                title: "Measuring exposure",
                detail: format!("{} photos to go", self.exposure_left),
                progress: None,
                cancellable: false,
            });
        }
        if self.duplicates_loading {
            out.push(Activity {
                id: "duplicates",
                icon: Icon::Duplicates,
                title: "Finding duplicates",
                detail: "Comparing photos\u{2026}".to_string(),
                progress: None,
                cancellable: false,
            });
        }
        let backlog = self.edited_thumbs.backlog();
        if backlog >= THUMBNAIL_THRESHOLD {
            out.push(Activity {
                id: "thumbnails",
                icon: Icon::Grid,
                title: "Updating previews",
                detail: format!("{backlog} thumbnails to go"),
                progress: None,
                cancellable: false,
            });
        }
        out
    }

    /// Keeps a repaint ticker running exactly while something runs.
    pub(crate) fn sync_activity_ticker(&mut self, cx: &mut Context<Self>) {
        let busy = self.settings.appearance.activity_indicator && !self.activities().is_empty();
        match (busy, self.activity_ticker.is_some()) {
            (true, false) => {
                self.activity_ticker = Some(cx.spawn(async move |this, cx| loop {
                    cx.background_executor().timer(TICK).await;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }));
            }
            (false, true) => {
                self.activity_ticker = None;
                if self.activity_open {
                    self.activity_open = false;
                }
            }
            _ => {}
        }
    }

    /// The chip at the left of the status bar's right side.
    pub(crate) fn render_activity_chip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.settings.appearance.activity_indicator {
            return None;
        }
        let all = self.activities();
        let main = all.first()?;
        let theme = self.theme;
        let more = all.len() - 1;
        Some(
            div()
                .id("activity-chip")
                .flex()
                .flex_row()
                .items_center()
                .gap_1p5()
                .h(sp(20.0))
                .px_2()
                .rounded_md()
                .cursor_pointer()
                .when(self.activity_open, |el| el.bg(theme.surface_elevated))
                .hover(|el| el.bg(theme.surface_elevated))
                .child(pulse("activity-pulse", &theme))
                .child(
                    div()
                        .text_color(theme.text_muted)
                        .whitespace_nowrap()
                        .child(match main.progress {
                            Some(p) => format!("{} {:.0}%", main.title, p * 100.0),
                            None => main.title.to_string(),
                        }),
                )
                .when_some(main.progress, |el, p| el.child(bar(p, 56.0, &theme)))
                .when(more > 0, |el| {
                    el.child(
                        div()
                            .px_1()
                            .rounded_sm()
                            .bg(theme.accent_muted)
                            .text_size(sp(10.0))
                            .text_color(theme.text)
                            .child(format!("+{more}")),
                    )
                })
                .tooltip(rustyedit_ui::tip("Background activity", &theme))
                .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                    this.activity_open = !this.activity_open;
                    cx.notify();
                }))
                .into_any_element(),
        )
    }

    /// Every running job, above the status bar.
    pub(crate) fn render_activity_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.activity_open || !self.settings.appearance.activity_indicator {
            return None;
        }
        let all = self.activities();
        if all.is_empty() {
            return None;
        }
        let theme = self.theme;
        let rows = all.into_iter().map(|job| {
            div()
                .id(SharedString::from(format!("activity-{}", job.id)))
                .flex()
                .flex_col()
                .gap_1()
                .px_3()
                .py_2()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(rustyedit_ui::icon(job.icon, 14.0, theme.accent))
                        .child(
                            div()
                                .flex_1()
                                .text_size(sp(12.0))
                                .text_color(theme.text)
                                .child(job.title),
                        )
                        .when(job.cancellable, |el| {
                            el.child(
                                div()
                                    .id(SharedString::from(format!("activity-cancel-{}", job.id)))
                                    .px_1p5()
                                    .h(sp(18.0))
                                    .flex()
                                    .items_center()
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .text_size(sp(10.5))
                                    .text_color(theme.text_muted)
                                    .hover(|el| el.bg(theme.danger.opacity(0.15)).text_color(theme.danger))
                                    .child("Cancel")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                                        if let Some(run) = &this.export_run {
                                            run.cancel();
                                        }
                                        cx.notify();
                                    })),
                            )
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .pl(sp(22.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w(sp(0.0))
                                .truncate()
                                .text_size(sp(10.5))
                                .text_color(theme.text_faint)
                                .child(job.detail),
                        )
                        .when_some(job.progress, |el, p| {
                            el.child(
                                div()
                                    .text_size(sp(10.5))
                                    .text_color(theme.text_muted)
                                    .child(format!("{:.0}%", p * 100.0)),
                            )
                        }),
                )
                .child(div().pl(sp(22.0)).child(match job.progress {
                    Some(p) => bar(p, 218.0, &theme),
                    None => indeterminate_bar(job.id, 218.0, &theme),
                }))
        });
        Some(
            div()
                .id("activity-panel")
                .absolute()
                .right(sp(12.0))
                .bottom(px(rustyedit_ui::STATUS_BAR_HEIGHT + 6.0))
                .w(sp(270.0))
                .py_1()
                .flex()
                .flex_col()
                .rounded_lg()
                .bg(theme.surface_elevated)
                .border_1()
                .border_color(theme.border)
                .shadow_lg()
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _, _w, cx| {
                    this.activity_open = false;
                    cx.notify();
                }))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .px_3()
                        .pt_1()
                        .pb_0p5()
                        .text_size(sp(10.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.text_faint)
                        .child("BACKGROUND ACTIVITY"),
                )
                .children(rows)
                .into_any_element(),
        )
    }
}

/// A small accent dot that breathes while work runs.
fn pulse(id: &'static str, theme: &Theme) -> impl IntoElement {
    div()
        .id(id)
        .size(sp(7.0))
        .rounded_full()
        .bg(theme.accent)
        .with_animation(
            id,
            Animation::new(Duration::from_millis(1100)).repeat(),
            |el, t| {
                let wave = (t * std::f32::consts::TAU).sin() * 0.5 + 0.5;
                el.opacity(0.35 + 0.65 * wave)
            },
        )
}

/// A thin progress bar `width` wide, filled to `progress`.
fn bar(progress: f32, width: f32, theme: &Theme) -> AnyElement {
    div()
        .w(sp(width))
        .h(sp(4.0))
        .rounded_full()
        .bg(theme.border)
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .w(sp(width * progress.clamp(0.0, 1.0)))
                .rounded_full()
                .bg(theme.accent),
        )
        .into_any_element()
}

/// A bar with a segment sliding across, for work of unknown size.
fn indeterminate_bar(id: &'static str, width: f32, theme: &Theme) -> AnyElement {
    let segment = width * 0.3;
    div()
        .relative()
        .w(sp(width))
        .h(sp(4.0))
        .rounded_full()
        .bg(theme.border)
        .overflow_hidden()
        .child(
            div()
                .id(SharedString::from(format!("activity-slide-{id}")))
                .absolute()
                .top_0()
                .h_full()
                .w(sp(segment))
                .rounded_full()
                .bg(theme.accent)
                .with_animation(
                    SharedString::from(format!("activity-slide-anim-{id}")),
                    Animation::new(Duration::from_millis(1400)).repeat(),
                    move |el, t| el.left(sp(-segment + (width + segment) * t)),
                ),
        )
        .into_any_element()
}
