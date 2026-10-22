//! Colour management.
//!
//! Photos are edited in a wide-gamut **working space**: Display P3
//! primaries with the sRGB curve. Every slider was tuned on curve-encoded
//! values, so keeping that curve keeps the tools behaving as before, while
//! the P3 primaries hold the saturated colours of RAW files and P3 photos
//! that sRGB would clip.
//!
//! - On the way in, a photo is converted from its embedded ICC profile
//!   (sRGB when it has none) into the working space, at 16 bits.
//! - On the way out it's converted into the output space: the display's
//!   own profile for the live preview, the chosen export space (whose
//!   profile is embedded in the file) for exports. Both use a 3D lookup
//!   table built once per space with moxcms, which the GPU and the CPU
//!   fallback sample identically.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use image::{DynamicImage, RgbImage};
use moxcms::{ColorProfile, Layout, TransformOptions};

/// Linear working-space RGB -> CIE XYZ (D65): Display P3.
pub(crate) const WORKING_TO_XYZ: [[f32; 3]; 3] = [
    [0.486_570_9, 0.265_667_7, 0.198_217_3],
    [0.228_974_6, 0.691_738_5, 0.079_286_9],
    [0.0, 0.045_113_4, 1.043_944_4],
];

/// Grid points per axis of the output lookup tables.
const LUT_SIZE: usize = 33;

fn working() -> ColorProfile {
    ColorProfile::new_display_p3()
}

/// A colour space rendered pixels can be converted into.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum OutputSpace {
    /// The web's (and most viewers') default.
    #[default]
    Srgb,
    /// Wide gamut, as on Apple displays.
    DisplayP3,
    /// Wide gamut, common in print workflows.
    AdobeRgb,
    /// Any ICC profile, e.g. the display's.
    Profile(Vec<u8>),
}

impl OutputSpace {
    fn profile(&self) -> Option<ColorProfile> {
        Some(match self {
            OutputSpace::Srgb => ColorProfile::new_srgb(),
            OutputSpace::DisplayP3 => ColorProfile::new_display_p3(),
            OutputSpace::AdobeRgb => ColorProfile::new_adobe_rgb(),
            OutputSpace::Profile(bytes) => ColorProfile::new_from_slice(bytes).ok()?,
        })
    }

    /// The ICC profile to embed in a file in this space.
    pub fn icc(&self) -> Option<Vec<u8>> {
        match self {
            OutputSpace::Profile(bytes) => Some(bytes.clone()),
            other => other.profile()?.encode().ok(),
        }
    }
}

/// Entries in each channel's encoding curve (matrix conversions).
const CURVE_SIZE: usize = 4096;

/// A working space -> output space conversion, in the form the shader's
/// `apply_lut` reads from `aux`:
/// - **matrix profiles** (nearly all displays and export spaces): the
///   working curve undone, a 3x3 matrix into the output's linear RGB, a
///   clip to its gamut, and its own curve as a table per channel. Exact.
///   Flagged to the shader by a size of 1.
/// - anything else: a `size`^3 grid sampled with moxcms, interpolated.
pub(crate) struct Lut3d {
    pub(crate) size: u32,
    pub(crate) data: Vec<f32>,
}

impl Lut3d {
    fn build(dest: &ColorProfile) -> Option<Self> {
        Self::matrix(dest).or_else(|| Self::grid(dest))
    }

    fn matrix(dest: &ColorProfile) -> Option<Self> {
        if !dest.is_matrix_shaper() {
            return None;
        }
        let m = working().transform_matrix(dest);
        let mut data: Vec<f32> = m.v.iter().flatten().map(|v| *v as f32).collect();
        // The output's own curve, from its linear-light twin.
        let mut linear = dest.clone();
        let flat = moxcms::ToneReprCurve::Parametric(vec![1.0]);
        linear.red_trc = Some(flat.clone());
        linear.green_trc = Some(flat.clone());
        linear.blue_trc = Some(flat);
        linear.cicp = None;
        let encode = linear
            .create_transform_f32(Layout::Rgb, dest, Layout::Rgb, TransformOptions::default())
            .ok()?;
        let ramp: Vec<f32> = (0..CURVE_SIZE)
            .flat_map(|i| [i as f32 / (CURVE_SIZE - 1) as f32; 3])
            .collect();
        let mut curves = vec![0.0; ramp.len()];
        encode.transform(&ramp, &mut curves).ok()?;
        for channel in 0..3 {
            data.extend(curves.iter().skip(channel).step_by(3));
        }
        Some(Self { size: 1, data })
    }

    fn grid(dest: &ColorProfile) -> Option<Self> {
        let transform = working()
            .create_transform_f32(Layout::Rgb, dest, Layout::Rgb, TransformOptions::default())
            .ok()?;
        let n = LUT_SIZE;
        let step = 1.0 / (n - 1) as f32;
        let mut grid = Vec::with_capacity(n * n * n * 3);
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    grid.extend_from_slice(&[r as f32 * step, g as f32 * step, b as f32 * step]);
                }
            }
        }
        let mut data = vec![0.0; grid.len()];
        transform.transform(&grid, &mut data).ok()?;
        Some(Self {
            size: n as u32,
            data,
        })
    }

    fn at(&self, r: usize, g: usize, b: usize) -> [f32; 3] {
        let n = self.size as usize;
        let i = ((b * n + g) * n + r) * 3;
        [self.data[i], self.data[i + 1], self.data[i + 2]]
    }

    /// The conversion of one working-space pixel, as the shader's
    /// `apply_lut`.
    pub(crate) fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let c = c.map(|v| v.clamp(0.0, 1.0));
        if self.size == 1 {
            let lin = c.map(decode_srgb);
            let m = &self.data;
            let out: [f32; 3] = std::array::from_fn(|k| {
                (m[k * 3] * lin[0] + m[k * 3 + 1] * lin[1] + m[k * 3 + 2] * lin[2]).clamp(0.0, 1.0)
            });
            return std::array::from_fn(|k| {
                let table = &self.data[9 + k * CURVE_SIZE..9 + (k + 1) * CURVE_SIZE];
                let x = out[k] * (CURVE_SIZE - 1) as f32;
                let i0 = x.floor() as usize;
                let i1 = (i0 + 1).min(CURVE_SIZE - 1);
                let v = table[i0] + (table[i1] - table[i0]) * (x - i0 as f32);
                v.clamp(0.0, 1.0)
            });
        }
        let n = (self.size - 1) as f32;
        let x = c.map(|v| v * n);
        let i0 = x.map(|v| v.floor() as usize);
        let i1 = i0.map(|v| (v + 1).min(self.size as usize - 1));
        let t: [f32; 3] = std::array::from_fn(|k| x[k] - i0[k] as f32);
        let lerp = |a: [f32; 3], b: [f32; 3], t: f32| -> [f32; 3] {
            std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t)
        };
        let c00 = lerp(self.at(i0[0], i0[1], i0[2]), self.at(i1[0], i0[1], i0[2]), t[0]);
        let c10 = lerp(self.at(i0[0], i1[1], i0[2]), self.at(i1[0], i1[1], i0[2]), t[0]);
        let c01 = lerp(self.at(i0[0], i0[1], i1[2]), self.at(i1[0], i0[1], i1[2]), t[0]);
        let c11 = lerp(self.at(i0[0], i1[1], i1[2]), self.at(i1[0], i1[1], i1[2]), t[0]);
        lerp(lerp(c00, c10, t[1]), lerp(c01, c11, t[1]), t[2]).map(|v| v.clamp(0.0, 1.0))
    }

    /// Converts an 8-bit working-space image in place (the CPU path).
    pub(crate) fn apply_rgb8(&self, image: &mut RgbImage) {
        let width = image.width() as usize;
        let raw: &mut [u8] = &mut *image;
        crate::parallel::for_row_bands(raw, width * 3, |_, band| {
            for px in band.chunks_exact_mut(3) {
                let out = self.apply([px[0], px[1], px[2]].map(|v| v as f32 / 255.0));
                for (v, o) in px.iter_mut().zip(out) {
                    *v = (o * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8;
                }
            }
        });
    }
}

/// The lookup table into `space`, built on first use. `None` means no
/// conversion (the space is the working space, or its profile is broken).
pub(crate) fn lut(space: &OutputSpace) -> Option<Arc<Lut3d>> {
    if *space == OutputSpace::DisplayP3 {
        return None;
    }
    static LUTS: OnceLock<Mutex<HashMap<OutputSpace, Option<Arc<Lut3d>>>>> = OnceLock::new();
    let mut luts = LUTS.get_or_init(Default::default).lock().ok()?;
    luts.entry(space.clone())
        .or_insert_with(|| space.profile().and_then(|p| Lut3d::build(&p)).map(Arc::new))
        .clone()
}

/// sRGB's curve undone (the working space's).
pub(crate) fn decode_srgb(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

static DISPLAY: Mutex<OutputSpace> = Mutex::new(OutputSpace::Srgb);

/// Tells the live preview which display it's shown on: its ICC profile, or
/// `None` for an sRGB display.
pub fn set_display_profile(icc: Option<Vec<u8>>) {
    let space = match icc {
        Some(bytes) if ColorProfile::new_from_slice(&bytes).is_ok() => OutputSpace::Profile(bytes),
        _ => OutputSpace::Srgb,
    };
    if let Ok(mut display) = DISPLAY.lock() {
        *display = space;
    }
}

/// The colour space live previews are rendered in.
pub(crate) fn display_space() -> OutputSpace {
    DISPLAY.lock().map(|d| d.clone()).unwrap_or_default()
}

/// Converts `image`, in the colour space its ICC profile `icc` describes
/// (sRGB without one), into the working space at 16 bits per channel.
/// Profiles moxcms can't use (grey, CMYK) are treated as sRGB.
pub(crate) fn to_working(image: &DynamicImage, icc: Option<&[u8]>) -> DynamicImage {
    let (width, height) = (image.width(), image.height());
    let rgb16 = image.to_rgb16();
    let source = icc
        .and_then(|bytes| ColorProfile::new_from_slice(bytes).ok())
        .unwrap_or_else(ColorProfile::new_srgb);
    let transform = source
        .create_transform_16bit(Layout::Rgb, &working(), Layout::Rgb, TransformOptions::default())
        .or_else(|_| {
            ColorProfile::new_srgb().create_transform_16bit(
                Layout::Rgb,
                &working(),
                Layout::Rgb,
                TransformOptions::default(),
            )
        });
    let Ok(transform) = transform else {
        return DynamicImage::ImageRgb16(rgb16);
    };
    let src = rgb16.as_raw();
    let mut out = vec![0u16; src.len()];
    let row = width as usize * 3;
    let failed = std::sync::atomic::AtomicBool::new(false);
    crate::parallel::for_row_bands(&mut out, row, |first_row, band| {
        let start = first_row * row;
        if transform
            .transform(&src[start..start + band.len()], band)
            .is_err()
        {
            failed.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    });
    if failed.into_inner() {
        return DynamicImage::ImageRgb16(rgb16);
    }
    image::ImageBuffer::from_raw(width, height, out)
        .map(DynamicImage::ImageRgb16)
        .unwrap_or(DynamicImage::ImageRgb16(rgb16))
}

/// sRGB's transfer curve, continued above 1 (as the shader's
/// `encode_srgb`).
pub(crate) fn encode_srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v.max(0.0) * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_round_trips_through_the_working_space() {
        let colours = [[255u8, 0, 0], [12, 200, 90], [128, 128, 128], [250, 240, 10]];
        let image = DynamicImage::ImageRgb8(RgbImage::from_fn(4, 1, |x, _| {
            image::Rgb(colours[x as usize])
        }));
        let working = to_working(&image, None).to_rgb16();
        // Pure sRGB red is inside P3, so it's no longer at the edge.
        assert!(working.get_pixel(0, 0)[1] > 5000, "{:?}", working.get_pixel(0, 0));
        let lut = lut(&OutputSpace::Srgb).unwrap();
        for (x, expected) in colours.iter().enumerate() {
            let p = working.get_pixel(x as u32, 0).0.map(|v| v as f32 / 65535.0);
            let back = lut.apply(p).map(|v| (v * 255.0).round() as i32);
            for c in 0..3 {
                assert!((back[c] - expected[c] as i32).abs() <= 2, "{x}: {back:?} vs {expected:?}");
            }
        }
    }

    #[test]
    fn export_profiles_encode() {
        for space in [OutputSpace::Srgb, OutputSpace::DisplayP3, OutputSpace::AdobeRgb] {
            assert!(space.icc().is_some_and(|icc| icc.len() > 100), "{space:?}");
        }
    }
}
