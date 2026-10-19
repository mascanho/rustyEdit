//! The non-destructive edit pipeline: turns a decoded source image plus a
//! `DevelopSettings` into a rendered preview. Pure CPU/`image`-crate based
//! for now; the pixel math lives in small, independently testable modules
//! (`tone`, `color`, `detail`) so a future GPU shader backend could replace
//! any one of them without touching the others.

mod color;
mod detail;
mod geometry;
mod tone;

use image::{DynamicImage, RgbImage};
use rustyedit_core::DevelopSettings;

/// Applies the full develop pipeline to `source`, returning a new image.
/// `source` is never mutated, so callers can keep a cached decoded copy
/// around and re-render cheaply whenever a slider changes.
pub fn apply(source: &DynamicImage, settings: &DevelopSettings) -> RgbImage {
    let rotated = geometry::apply_geometry(source, settings);
    let mut buffer = rotated.to_rgb8();

    if settings.is_edited() {
        for pixel in buffer.pixels_mut() {
            let with_tone = tone::apply_tone_rgb8(*pixel, settings);
            *pixel = color::apply_color_rgb8(with_tone, settings);
        }
        detail::apply_detail(&mut buffer, settings);
    }

    buffer
}

/// Downscales `source` so its longest edge is at most `max_edge`, using a
/// fast filter. Intended to keep interactive Develop-panel previews quick
/// even on very large source files; full quality is only needed at export.
pub fn downscale_for_preview(source: &DynamicImage, max_edge: u32) -> DynamicImage {
    if source.width() <= max_edge && source.height() <= max_edge {
        return source.clone();
    }
    source.resize(max_edge, max_edge, image::imageops::FilterType::Triangle)
}
