//! Waveform, RGB parade and vectorscope views of a photo's tones and
//! colours, painted from the density grids in [`Histogram::scopes`].

use std::sync::Arc;

use gpui::{
    canvas, div, fill, hsla, point, prelude::*, px, size, Bounds, Hsla, PathBuilder, Pixels, Rgba,
    Window,
};
use rustyedit_core::{
    Histogram, HistogramChannel, ScopeMode, Scopes, VECTOR_SIZE, WAVE_COLS, WAVE_ROWS,
};

use crate::theme::Theme;

pub const SCOPE_HEIGHT: f32 = 130.0;

fn red() -> Hsla {
    hsla(0.0, 0.85, 0.58, 1.0)
}
fn green() -> Hsla {
    hsla(125.0 / 360.0, 0.7, 0.5, 1.0)
}
fn blue() -> Hsla {
    hsla(215.0 / 360.0, 0.9, 0.62, 1.0)
}

/// The graph for `mode` (Waveform, Parade or Vectorscope). `channel`
/// picks what the waveform shows: RGB overlaid, luminance, or one colour.
pub fn scope_view(
    theme: &Theme,
    histogram: Option<Arc<Histogram>>,
    mode: ScopeMode,
    channel: HistogramChannel,
) -> impl IntoElement {
    let theme = *theme;
    div()
        .relative()
        .w_full()
        .h(px(SCOPE_HEIGHT))
        .rounded_md()
        .overflow_hidden()
        .bg(theme.background)
        .border_1()
        .border_color(theme.border_subtle)
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let Some(histogram) = &histogram else {
                        return;
                    };
                    if histogram.scopes.is_empty() {
                        return;
                    }
                    match mode {
                        ScopeMode::Waveform => {
                            paint_waveform(window, bounds, &histogram.scopes, channel, &theme)
                        }
                        ScopeMode::Parade => {
                            paint_parade(window, bounds, &histogram.scopes, &theme)
                        }
                        ScopeMode::Vectorscope => {
                            paint_vectorscope(window, bounds, &histogram.scopes, &theme)
                        }
                        ScopeMode::Histogram => {}
                    }
                },
            )
            .size_full(),
        )
}

fn quad(window: &mut Window, x: f32, y: f32, w: f32, h: f32, colour: Hsla) {
    window.paint_quad(fill(
        Bounds::new(point(px(x), px(y)), size(px(w), px(h))),
        colour,
    ));
}

/// Horizontal level lines at 0, 25, 50, 75 and 100%.
fn level_lines(window: &mut Window, bounds: Bounds<Pixels>, theme: &Theme) {
    let (x, y) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    for i in 0..=4 {
        let at = y + h - (h - 1.0) * i as f32 / 4.0 - 1.0;
        quad(
            window,
            x,
            at,
            w,
            1.0,
            theme
                .text
                .opacity(if i == 0 || i == 4 { 0.16 } else { 0.08 }),
        );
    }
}

/// One waveform grid drawn into the `x..x+w` strip of `bounds`.
fn paint_grid(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    x0: f32,
    w: f32,
    grid: &[u32],
    colour: Hsla,
) {
    let peak = grid.iter().copied().max().unwrap_or(0).max(1) as f32;
    let (y0, h) = (f32::from(bounds.origin.y), f32::from(bounds.size.height));
    let (cw, ch) = (w / WAVE_COLS as f32, h / WAVE_ROWS as f32);
    for row in 0..WAVE_ROWS {
        for col in 0..WAVE_COLS {
            let count = grid[row * WAVE_COLS + col];
            if count == 0 {
                continue;
            }
            // Square root keeps faint areas visible next to dense ones.
            let density = (count as f32 / peak).sqrt().clamp(0.0, 1.0);
            quad(
                window,
                x0 + col as f32 * cw,
                y0 + h - (row as f32 + 1.0) * ch,
                cw + 0.5,
                ch + 0.5,
                colour.opacity(0.12 + 0.88 * density),
            );
        }
    }
}

fn paint_waveform(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    scopes: &Scopes,
    channel: HistogramChannel,
    theme: &Theme,
) {
    level_lines(window, bounds, theme);
    let (x, w) = (f32::from(bounds.origin.x), f32::from(bounds.size.width));
    match channel {
        HistogramChannel::Rgb => {
            // Overlaid, each channel at reduced strength so overlaps mix.
            for (grid, colour) in [
                (&scopes.wave_red, red()),
                (&scopes.wave_green, green()),
                (&scopes.wave_blue, blue()),
            ] {
                paint_grid(window, bounds, x, w, grid, colour.opacity(0.75));
            }
        }
        HistogramChannel::Luma => paint_grid(
            window,
            bounds,
            x,
            w,
            &scopes.wave_luma,
            hsla(0.0, 0.0, 0.95, 1.0),
        ),
        HistogramChannel::Red => paint_grid(window, bounds, x, w, &scopes.wave_red, red()),
        HistogramChannel::Green => paint_grid(window, bounds, x, w, &scopes.wave_green, green()),
        HistogramChannel::Blue => paint_grid(window, bounds, x, w, &scopes.wave_blue, blue()),
    }
}

fn paint_parade(window: &mut Window, bounds: Bounds<Pixels>, scopes: &Scopes, theme: &Theme) {
    level_lines(window, bounds, theme);
    let (x, w) = (f32::from(bounds.origin.x), f32::from(bounds.size.width));
    let third = w / 3.0;
    for (i, (grid, colour)) in [
        (&scopes.wave_red, red()),
        (&scopes.wave_green, green()),
        (&scopes.wave_blue, blue()),
    ]
    .into_iter()
    .enumerate()
    {
        paint_grid(window, bounds, x + third * i as f32, third, grid, colour);
        if i > 0 {
            quad(
                window,
                x + third * i as f32,
                f32::from(bounds.origin.y),
                1.0,
                f32::from(bounds.size.height),
                theme.text.opacity(0.14),
            );
        }
    }
}

/// The chroma cell's own colour, so the plot reads as a colour wheel.
fn chroma_colour(cb: f32, cr: f32) -> Hsla {
    let y = 0.62;
    let r = y + 1.5748 * cr;
    let b = y + 1.8556 * cb;
    let g = (y - 0.2126 * r - 0.0722 * b) / 0.7152;
    Hsla::from(Rgba {
        r: r.clamp(0.0, 1.0),
        g: g.clamp(0.0, 1.0),
        b: b.clamp(0.0, 1.0),
        a: 1.0,
    })
}

fn paint_vectorscope(window: &mut Window, bounds: Bounds<Pixels>, scopes: &Scopes, theme: &Theme) {
    let (bx, by) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
    let (bw, bh) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let side = bw.min(bh) - 6.0;
    let (x0, y0) = (bx + (bw - side) / 2.0, by + (bh - side) / 2.0);
    let (cx, cy) = (x0 + side / 2.0, y0 + side / 2.0);
    let cell = side / VECTOR_SIZE as f32;

    // Graticule: the outer circle, a half-way circle and the axes.
    let circle = |window: &mut Window, radius: f32, alpha: f32| {
        let mut path = PathBuilder::stroke(px(1.0));
        for i in 0..=72 {
            let a = i as f32 / 72.0 * std::f32::consts::TAU;
            let p = point(px(cx + radius * a.cos()), px(cy + radius * a.sin()));
            if i == 0 {
                path.move_to(p);
            } else {
                path.line_to(p);
            }
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, theme.text.opacity(alpha));
        }
    };
    circle(window, side / 2.0, 0.18);
    circle(window, side / 4.0, 0.08);
    quad(window, x0, cy, side, 1.0, theme.text.opacity(0.08));
    quad(window, cx, y0, 1.0, side, theme.text.opacity(0.08));

    // The skin-tone line (about 123 degrees from the blue axis).
    let angle = 123.0f32.to_radians();
    let mut skin = PathBuilder::stroke(px(1.0));
    skin.move_to(point(px(cx), px(cy)));
    skin.line_to(point(
        px(cx + side / 2.0 * angle.cos()),
        px(cy - side / 2.0 * angle.sin()),
    ));
    if let Ok(path) = skin.build() {
        window.paint_path(path, hsla(30.0 / 360.0, 0.8, 0.6, 0.55));
    }

    // The data.
    let peak = scopes.vector.iter().copied().max().unwrap_or(0).max(1) as f32;
    for row in 0..VECTOR_SIZE {
        for col in 0..VECTOR_SIZE {
            let count = scopes.vector[row * VECTOR_SIZE + col];
            if count == 0 {
                continue;
            }
            let density = (count as f32 / peak).powf(0.4).clamp(0.0, 1.0);
            let cb = (col as f32 + 0.5) / VECTOR_SIZE as f32 - 0.5;
            let cr = 0.5 - (row as f32 + 0.5) / VECTOR_SIZE as f32;
            quad(
                window,
                x0 + col as f32 * cell,
                y0 + row as f32 * cell,
                cell + 0.5,
                cell + 0.5,
                chroma_colour(cb, cr).opacity(0.25 + 0.75 * density),
            );
        }
    }

    // Targets where the six primary and secondary colours sit at 75%.
    for (r, g, b) in [
        (1.0, 0.0, 0.0),
        (1.0, 1.0, 0.0),
        (0.0, 1.0, 0.0),
        (0.0, 1.0, 1.0),
        (0.0, 0.0, 1.0),
        (1.0, 0.0, 1.0),
    ] {
        let (cb, cr) = Scopes::chroma(r * 0.75, g * 0.75, b * 0.75);
        let (tx, ty) = (cx + cb * side, cy - cr * side);
        let colour = Hsla::from(Rgba { r, g, b, a: 1.0 });
        let t = 7.0;
        for (dx, dy, w, h) in [
            (-t / 2.0, -t / 2.0, t, 1.0),
            (-t / 2.0, t / 2.0 - 1.0, t, 1.0),
            (-t / 2.0, -t / 2.0, 1.0, t),
            (t / 2.0 - 1.0, -t / 2.0, 1.0, t),
        ] {
            quad(window, tx + dx, ty + dy, w, h, colour.opacity(0.85));
        }
    }
}
