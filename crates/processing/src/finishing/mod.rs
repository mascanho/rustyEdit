//! The finishing stage: everything after tone, colour and detail. Tone
//! curves, the Color Mixer, Color Grading, Dehaze, Vignette and Grain.
//!
//! It runs on the CPU after the (possibly GPU-rendered) base stage, split
//! across threads by bands of rows. Each tool is set up once per render
//! from the settings and is skipped entirely while neutral, so photos that
//! don't use them pay nothing.

mod curves;
mod effects;
mod grading;
mod mixer;

use image::RgbImage;
use rustyedit_core::DevelopSettings;

use curves::CurveLuts;
use effects::{Dehaze, Grain, Vignette};
use grading::Grading;
use mixer::Mixer;

/// The finishing tools a render needs, prepared from its settings.
pub(crate) struct Finishing {
    curves: Option<CurveLuts>,
    mixer: Option<Mixer>,
    grading: Option<Grading>,
    dehaze: Option<Dehaze>,
    vignette: Option<Vignette>,
    grain: Option<Grain>,
}

impl Finishing {
    /// `None` when every finishing tool is neutral.
    pub(crate) fn new(settings: &DevelopSettings, width: u32, height: u32) -> Option<Self> {
        let finishing = Self {
            curves: CurveLuts::new(&settings.curves),
            mixer: Mixer::new(&settings.mixer),
            grading: Grading::new(&settings.grading),
            dehaze: Dehaze::new(settings.dehaze),
            vignette: Vignette::new(settings, width, height),
            grain: Grain::new(settings, width, height),
        };
        let any = finishing.curves.is_some()
            || finishing.mixer.is_some()
            || finishing.grading.is_some()
            || finishing.dehaze.is_some()
            || finishing.vignette.is_some()
            || finishing.grain.is_some();
        any.then_some(finishing)
    }

    /// Applies every active tool to `image` in place.
    pub(crate) fn apply(&self, image: &mut RgbImage) {
        let width = image.width() as usize;
        let height = image.height() as usize;
        if width == 0 || height == 0 {
            return;
        }
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(height);
        let rows_per_band = height.div_ceil(threads);
        let row_bytes = width * 3;

        std::thread::scope(|scope| {
            for (band, chunk) in image.chunks_mut(rows_per_band * row_bytes).enumerate() {
                scope.spawn(move || {
                    let first_row = band * rows_per_band;
                    for (i, row) in chunk.chunks_mut(row_bytes).enumerate() {
                        self.apply_row(row, (first_row + i) as u32);
                    }
                });
            }
        });
    }

    fn apply_row(&self, row: &mut [u8], y: u32) {
        for (x, px) in row.chunks_exact_mut(3).enumerate() {
            if let Some(curves) = &self.curves {
                curves.apply(px);
            }
            let mut rgb = [
                px[0] as f32 / 255.0,
                px[1] as f32 / 255.0,
                px[2] as f32 / 255.0,
            ];
            if let Some(mixer) = &self.mixer {
                rgb = mixer.apply(rgb);
            }
            if let Some(grading) = &self.grading {
                rgb = grading.apply(rgb);
            }
            if let Some(dehaze) = &self.dehaze {
                rgb = dehaze.apply(rgb);
            }
            if let Some(vignette) = &self.vignette {
                rgb = vignette.apply(rgb, x as u32, y);
            }
            if let Some(grain) = &self.grain {
                rgb = grain.apply(rgb, x as u32, y);
            }
            for (out, value) in px.iter_mut().zip(rgb) {
                *out = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
}

/// Rec. 709 luma of a `[0, 1]` RGB pixel.
pub(crate) fn luma([r, g, b]: [f32; 3]) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

pub(crate) fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_settings_need_no_finishing() {
        assert!(Finishing::new(&DevelopSettings::default(), 10, 10).is_none());
    }

    #[test]
    fn vignette_darkens_corners_not_centre() {
        let settings = DevelopSettings {
            vignette: -100.0,
            ..Default::default()
        };
        let mut image = RgbImage::from_pixel(101, 101, image::Rgb([200, 200, 200]));
        Finishing::new(&settings, 101, 101)
            .unwrap()
            .apply(&mut image);
        assert_eq!(image.get_pixel(50, 50)[0], 200);
        assert!(image.get_pixel(0, 0)[0] < 120);
    }

    #[test]
    fn mixer_leaves_greys_alone() {
        let mut settings = DevelopSettings::default();
        for band in settings.mixer.iter_mut() {
            band.hue = 80.0;
            band.saturation = 80.0;
            band.luminance = 80.0;
        }
        let mut image = RgbImage::from_pixel(8, 8, image::Rgb([128, 128, 128]));
        Finishing::new(&settings, 8, 8).unwrap().apply(&mut image);
        assert_eq!(image.get_pixel(3, 3).0, [128, 128, 128]);
    }
}
