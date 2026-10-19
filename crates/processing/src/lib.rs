//! The non-destructive edit pipeline: turns a decoded source image plus a
//! `DevelopSettings` into a rendered preview. Pure CPU/`image`-crate based
//! with an optional wgpu compute backend (`gpu`); the pixel math lives in small, independently testable modules
//! (`tone`, `color`, `detail`) so a future GPU shader backend could replace
//! any one of them without touching the others.

mod color;
mod detail;
mod geometry;
mod gpu;
mod tone;

use image::{DynamicImage, RgbImage};
use rustyedit_core::DevelopSettings;

/// Applies the full develop pipeline to `source`, returning a new image.
/// `source` is never mutated, so callers can keep a cached decoded copy
/// around and re-render cheaply whenever a slider changes.
pub fn apply(source: &DynamicImage, settings: &DevelopSettings) -> RgbImage {
    apply_with(source, settings, true)
}

fn apply_with(source: &DynamicImage, settings: &DevelopSettings, allow_gpu: bool) -> RgbImage {
    let rotated = geometry::apply_geometry(source, settings);
    let mut buffer = rotated.to_rgb8();

    if settings.is_edited() {
        if allow_gpu {
            if let Some(rendered) = gpu::apply(&buffer, settings) {
                return rendered;
            }
        }
        for pixel in buffer.pixels_mut() {
            let with_tone = tone::apply_tone_rgb8(*pixel, settings);
            *pixel = color::apply_color_rgb8(with_tone, settings);
        }
        detail::apply_detail(&mut buffer, settings);
    }

    buffer
}

/// Name of the GPU the develop pipeline runs on, or `None` when it falls
/// back to the CPU. Initialises the device on first call.
pub use gpu::adapter_name as gpu_adapter_name;

/// Downscales `source` so its longest edge is at most `max_edge`, using a
/// fast filter. Intended to keep interactive Develop-panel previews quick
/// even on very large source files; full quality is only needed at export.
pub fn downscale_for_preview(source: &DynamicImage, max_edge: u32) -> DynamicImage {
    if source.width() <= max_edge && source.height() <= max_edge {
        return source.clone();
    }
    source.resize(max_edge, max_edge, image::imageops::FilterType::Triangle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_matches_cpu() {
        let Some(name) = gpu_adapter_name() else {
            eprintln!("no GPU available; skipping");
            return;
        };
        eprintln!("GPU: {name}");
        let img = RgbImage::from_fn(2560, 1706, |x, y| {
            image::Rgb([
                (x * 255 / 2560) as u8,
                (y * 255 / 1706) as u8,
                ((x * 7 + y * 13) % 256) as u8,
            ])
        });
        let source = DynamicImage::ImageRgb8(img);
        let settings = DevelopSettings {
            exposure: 20.0,
            contrast: 15.0,
            highlights: -30.0,
            shadows: 25.0,
            whites: 10.0,
            blacks: -10.0,
            temperature: 20.0,
            tint: -10.0,
            vibrance: 30.0,
            saturation: 15.0,
            clarity: 40.0,
            sharpening: 50.0,
            ..Default::default()
        };
        let t = std::time::Instant::now();
        let gpu = apply_with(&source, &settings, true);
        let gpu_time = t.elapsed();
        let t = std::time::Instant::now();
        let cpu = apply_with(&source, &settings, false);
        eprintln!("gpu {gpu_time:?} vs cpu {:?}", t.elapsed());
        let total: u64 = gpu
            .as_raw()
            .iter()
            .zip(cpu.as_raw())
            .map(|(a, b)| a.abs_diff(*b) as u64)
            .sum();
        let max = gpu
            .as_raw()
            .iter()
            .zip(cpu.as_raw())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        let mean = total as f64 / gpu.as_raw().len() as f64;
        eprintln!("mean abs diff {mean:.4}, max {max}");
        assert!(mean < 1.0, "mean diff {mean}");
    }
}
