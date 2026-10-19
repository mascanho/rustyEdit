use std::sync::Arc;

use gpui::{canvas, div, fill, point, prelude::*, px, size, Bounds, Hsla, Pixels, Window};
use rustyedit_core::Histogram;

use crate::theme::Theme;

pub const HISTOGRAM_HEIGHT: f32 = 96.0;

/// A Lightroom-style histogram: translucent red, green and blue channels
/// layered over a faint luminance silhouette, with guides splitting the
/// tonal range into quarters (shadows → highlights). Pass `None` while the
/// histogram is still being computed to show an empty frame.
pub fn histogram_view(theme: &Theme, histogram: Option<Arc<Histogram>>) -> impl IntoElement {
    let theme = *theme;

    div()
        .debug_selector(|| "histogram".into())
        .relative()
        .w_full()
        .h(px(HISTOGRAM_HEIGHT))
        .rounded_md()
        .overflow_hidden()
        .bg(theme.background)
        .border_1()
        .border_color(theme.border_subtle)
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    paint_histogram(bounds, histogram.as_deref(), &theme, window)
                },
            )
            .size_full(),
        )
}

fn paint_histogram(
    bounds: Bounds<Pixels>,
    histogram: Option<&Histogram>,
    theme: &Theme,
    window: &mut Window,
) {
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    eprintln!("TEMP-DEBUG paint_histogram bounds={bounds:?}");

    for quarter in 1..4 {
        let x = bounds.origin.x + px(width * quarter as f32 / 4.0);
        window.paint_quad(fill(
            Bounds::new(point(x, bounds.origin.y), size(px(1.0), bounds.size.height)),
            theme.border_subtle,
        ));
    }

    let Some(histogram) = histogram else {
        return;
    };
    let peak = histogram.peak().max(1) as f32;
    let columns = width.floor().max(1.0) as usize;

    let layers: [(&[u32; 256], Hsla); 4] = [
        (&histogram.luma, theme.text.opacity(0.12)),
        (&histogram.red, theme.label_red.opacity(0.5)),
        (&histogram.green, theme.label_green.opacity(0.5)),
        (&histogram.blue, theme.label_blue.opacity(0.5)),
    ];

    for (bins, color) in layers {
        for column in 0..columns {
            // Each pixel column covers one or more bins; show the tallest so
            // narrow spikes don't vanish when the panel is under 256px wide.
            let first = column * 256 / columns;
            let last = ((column + 1) * 256 / columns).max(first + 1).min(256);
            let count = bins[first..last].iter().copied().max().unwrap_or(0);
            let bar = (count as f32 / peak).min(1.0) * height;
            if bar < 0.5 {
                continue;
            }
            window.paint_quad(fill(
                Bounds::new(
                    point(
                        bounds.origin.x + px(column as f32),
                        bounds.origin.y + px(height - bar),
                    ),
                    size(px(1.0), px(bar)),
                ),
                color,
            ));
        }
    }
}
