//! The histogram graph: filled curves for the chosen channel(s), the
//! tonal-zone guides, and the hover / drag highlights. Interaction lives
//! in the app; this only paints what it's told.

use crate::sp;
use std::{cell::Cell, rc::Rc, sync::Arc};

use gpui::{
    canvas, div, fill, hsla, point, prelude::*, px, size, Bounds, Hsla, PathBuilder, Pixels, Window,
};
use rustyedit_core::{Histogram, HistogramChannel, TonalZone};

use crate::theme::Theme;

pub const HISTOGRAM_HEIGHT: f32 = 110.0;

/// What to draw.
pub struct HistogramProps {
    /// `None` while it's being computed: an empty frame.
    pub histogram: Option<Arc<Histogram>>,
    pub channel: HistogramChannel,
    /// Logarithmic heights, so sparse tones stay visible next to a peak.
    pub log_scale: bool,
    /// The level under the pointer, as a fraction of the range.
    pub hover: Option<f32>,
    /// A tonal zone to shade (hovered, or being dragged).
    pub zone: Option<TonalZone>,
    /// Receives where the graph was painted, for mapping the pointer.
    pub bounds: Option<Rc<Cell<Bounds<Pixels>>>>,
}

pub fn histogram_view(theme: &Theme, props: HistogramProps) -> impl IntoElement {
    histogram_view_with_height(theme, props, HISTOGRAM_HEIGHT)
}

/// The same graph, in a panel of its own height, for the places that
/// want a strip of it rather than a full box.
pub fn histogram_view_with_height(
    theme: &Theme,
    props: HistogramProps,
    height: f32,
) -> impl IntoElement {
    let theme = *theme;
    let record = props.bounds.clone();

    div()
        .debug_selector(|| "histogram".into())
        .relative()
        .w_full()
        .h(px(height))
        .rounded_md()
        .overflow_hidden()
        .bg(theme.background)
        .border_1()
        .border_color(theme.border_subtle)
        .child(
            canvas(
                move |bounds, _, _| {
                    if let Some(record) = &record {
                        record.set(bounds);
                    }
                },
                move |bounds, _, window, _| paint(bounds, &props, &theme, window),
            )
            .size_full(),
        )
}

fn paint(bounds: Bounds<Pixels>, props: &HistogramProps, theme: &Theme, window: &mut Window) {
    let width = f32::from(bounds.size.width);
    let x_at = |fraction: f32| bounds.origin.x + px(fraction * width);

    if let Some(zone) = props.zone {
        let (from, to) = zone.span();
        window.paint_quad(fill(
            Bounds::new(
                point(x_at(from), bounds.origin.y),
                size(px((to - from) * width), bounds.size.height),
            ),
            theme.accent.opacity(0.10),
        ));
    }
    // Zone boundaries.
    for zone in &TonalZone::ALL[1..] {
        window.paint_quad(fill(
            Bounds::new(
                point(x_at(zone.span().0), bounds.origin.y),
                size(sp(1.0), bounds.size.height),
            ),
            theme.border_subtle.opacity(0.7),
        ));
    }

    if let Some(histogram) = props.histogram.as_deref() {
        paint_histogram(
            window,
            bounds,
            HistogramPaint {
                histogram,
                channel: props.channel,
                log_scale: props.log_scale,
                opacity: 1.0,
            },
            theme,
        );
    }

    if let Some(level) = props.hover {
        window.paint_quad(fill(
            Bounds::new(
                point(x_at(level), bounds.origin.y),
                size(sp(1.0), bounds.size.height),
            ),
            theme.text.opacity(0.7),
        ));
    }
}

/// What to paint, for the panels that draw the histogram inside a canvas
/// of their own — behind a tone curve, for instance.
pub struct HistogramPaint<'a> {
    pub histogram: &'a Histogram,
    pub channel: HistogramChannel,
    pub log_scale: bool,
    /// Fades the whole graph, so it can sit behind another one.
    pub opacity: f32,
}

/// Draws the histogram's curves across `bounds`.
pub fn paint_histogram(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    paint: HistogramPaint<'_>,
    theme: &Theme,
) {
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    let columns = (width.floor() as usize).max(2);
    let scale = Scale::new(paint.histogram.peak(), paint.log_scale);
    let heights = |bins: &[u32; 256]| column_heights(bins, columns, &scale, height);
    let opacity = paint.opacity.clamp(0.0, 1.0);
    match paint.channel {
        HistogramChannel::Rgb => {
            let r = heights(&paint.histogram.red);
            let g = heights(&paint.histogram.green);
            let b = heights(&paint.histogram.blue);
            // Each channel, then where two overlap (yellow, cyan,
            // magenta), then where all three do: the classic look,
            // without needing blend modes.
            let min2 = |a: &[f32], b: &[f32]| -> Vec<f32> {
                a.iter().zip(b).map(|(a, b)| a.min(*b)).collect()
            };
            let rg = min2(&r, &g);
            let layers = [
                (r.clone(), hsla(0.0, 0.75, 0.52, 0.9)),
                (g.clone(), hsla(120.0 / 360.0, 0.6, 0.45, 0.9)),
                (b.clone(), hsla(215.0 / 360.0, 0.8, 0.56, 0.9)),
                (rg.clone(), hsla(55.0 / 360.0, 0.85, 0.52, 0.95)),
                (min2(&g, &b), hsla(185.0 / 360.0, 0.7, 0.5, 0.95)),
                (min2(&r, &b), hsla(300.0 / 360.0, 0.6, 0.58, 0.95)),
                (min2(&rg, &b), hsla(0.0, 0.0, 0.72, 0.95)),
            ];
            for (series, color) in layers {
                fill_series(window, bounds, &series, color.opacity(opacity));
            }
        }
        channel => {
            let color = match channel {
                HistogramChannel::Red => hsla(0.0, 0.75, 0.55, 1.0),
                HistogramChannel::Green => hsla(120.0 / 360.0, 0.6, 0.48, 1.0),
                HistogramChannel::Blue => hsla(215.0 / 360.0, 0.8, 0.6, 1.0),
                _ => theme.text_muted,
            };
            let series = heights(paint.histogram.bins(channel));
            fill_series(window, bounds, &series, color.opacity(0.55 * opacity));
            stroke_series(window, bounds, &series, color.opacity(opacity));
        }
    }
}

/// Maps bin counts to heights: linear against the peak, or logarithmic.
struct Scale {
    peak: f32,
    log: bool,
}

impl Scale {
    fn new(peak: u32, log: bool) -> Self {
        let peak = peak.max(1) as f32;
        Self {
            peak: if log { (1.0 + peak).ln() } else { peak },
            log,
        }
    }

    fn fraction(&self, count: f32) -> f32 {
        let value = if self.log { (1.0 + count).ln() } else { count };
        (value / self.peak).min(1.0)
    }
}

/// One height per pixel column. A column spanning several bins takes the
/// tallest, so narrow spikes survive a small panel; a light smoothing pass
/// then evens out the 256-step staircase on wide ones.
fn column_heights(bins: &[u32; 256], columns: usize, scale: &Scale, height: f32) -> Vec<f32> {
    let raw: Vec<f32> = (0..columns)
        .map(|column| {
            let first = column * 256 / columns;
            let last = ((column + 1) * 256 / columns).max(first + 1).min(256);
            let count = bins[first..last].iter().copied().max().unwrap_or(0);
            scale.fraction(count as f32) * height
        })
        .collect();
    (0..columns)
        .map(|i| {
            let prev = raw[i.saturating_sub(1)];
            let next = raw[(i + 1).min(columns - 1)];
            0.25 * prev + 0.5 * raw[i] + 0.25 * next
        })
        .collect()
}

/// The filled area under `series`.
fn fill_series(window: &mut Window, bounds: Bounds<Pixels>, series: &[f32], color: Hsla) {
    let bottom = bounds.origin.y + bounds.size.height;
    let mut points = vec![point(bounds.origin.x, bottom)];
    points.extend(series_points(bounds, series));
    points.push(point(bounds.origin.x + bounds.size.width, bottom));
    let mut path = PathBuilder::fill();
    path.add_polygon(&points, true);
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

/// The outline along the top of `series`.
fn stroke_series(window: &mut Window, bounds: Bounds<Pixels>, series: &[f32], color: Hsla) {
    let mut path = PathBuilder::stroke(sp(1.25));
    path.add_polygon(&series_points(bounds, series), false);
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

fn series_points(bounds: Bounds<Pixels>, series: &[f32]) -> Vec<gpui::Point<Pixels>> {
    let width = f32::from(bounds.size.width);
    let bottom = bounds.origin.y + bounds.size.height;
    let last = (series.len() - 1).max(1) as f32;
    series
        .iter()
        .enumerate()
        .map(|(i, h)| {
            point(
                bounds.origin.x + px(i as f32 / last * width),
                bottom - px(*h),
            )
        })
        .collect()
}
