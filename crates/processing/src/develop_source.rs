//! The photo a develop render starts from.
//!
//! - An ordinary image (JPEG, PNG, TIFF...) is converted into the working
//!   space at 16 bits when it's opened.
//! - A RAW file keeps its sensor data: the GPU demosaics it, and white
//!   balance, exposure and highlight recovery then work on the camera's
//!   own linear values, like a camera would.
//! - A preview made from either (see `lib::prepare_preview`) is GPU-format
//!   half floats, so it uploads without conversion.
//!
//! Each source gets a unique id, so the GPU knows when it already holds it.

use std::{
    borrow::Cow,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use half::f16;
use image::{DynamicImage, ImageBuffer, RgbImage};
use rawler::{rawimage::RawPhotometricInterpretation, RawImage, RawImageData};

use crate::cms;

/// How a RAW file's linear camera RGB becomes working-space RGB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RawColor {
    /// Camera RGB (white balanced) -> linear working RGB, row-major.
    pub(crate) matrix: [f32; 9],
    /// The white-balanced value at which each channel clips.
    pub(crate) clip: [f32; 3],
}

impl RawColor {
    /// The shader's `develop_raw` without the user's gains (the CPU
    /// fallback applies white balance and exposure as ordinary edits).
    fn develop(&self, c: [f32; 3]) -> [f32; 3] {
        let near: [f32; 3] = std::array::from_fn(|k| {
            let t = ((c[k] / self.clip[k] - 0.92) / 0.08).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        });
        let brightest = c[0].max(c[1]).max(c[2]);
        let c: [f32; 3] = std::array::from_fn(|k| c[k] + (c[k].max(brightest) - c[k]) * near[k]);
        let m = &self.matrix;
        [
            m[0] * c[0] + m[1] * c[1] + m[2] * c[2],
            m[3] * c[0] + m[4] * c[1] + m[5] * c[2],
            m[6] * c[0] + m[7] * c[1] + m[8] * c[2],
        ]
        .map(cms::encode_srgb)
    }
}

/// A RAW file's sensor data and what the demosaic needs to know about it.
pub(crate) struct RawMosaic {
    pub(crate) raw: RawImage,
    /// Black and white levels per 2x2 position.
    pub(crate) black: [f32; 4],
    pub(crate) white: [f32; 4],
    /// As-shot white balance, green = 1.
    pub(crate) wb: [f32; 3],
    /// The colour filter pattern: width, height, colour of each cell.
    pub(crate) cfa: (u32, u32, [f32; 36]),
    /// The picture area: x, y, width, height.
    pub(crate) crop: [u32; 4],
    /// EXIF orientation as transpose, flip x, flip y.
    pub(crate) orient: [bool; 3],
    pub(crate) exif_orientation: u8,
    pub(crate) color: RawColor,
}

impl RawMosaic {
    pub(crate) fn sensor_size(&self) -> (u32, u32) {
        (self.raw.width as u32, self.raw.height as u32)
    }

    pub(crate) fn data(&self) -> &[u16] {
        match &self.raw.data {
            RawImageData::Integer(data) => data,
            RawImageData::Float(_) => &[],
        }
    }
}

pub(crate) enum Pixels {
    /// Working-space pixels (any pixel type; 16-bit RGB when converted).
    Image(DynamicImage),
    /// GPU-format pixels: RGB as half floats in two u32 words. Linear
    /// camera RGB when `raw` is set, working-space RGB otherwise.
    Half {
        data: Vec<u32>,
        raw: Option<RawColor>,
    },
    Raw(RawMosaic),
}

/// A photo ready to develop. Cheap to clone.
#[derive(Clone)]
pub struct DevelopSource {
    pub(crate) id: u64,
    width: u32,
    height: u32,
    pub(crate) pixels: Arc<Pixels>,
}

fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl DevelopSource {
    fn new(width: u32, height: u32, pixels: Pixels) -> Self {
        Self {
            id: next_id(),
            width,
            height,
            pixels: Arc::new(pixels),
        }
    }

    /// An ordinary image in the colour space its ICC profile `icc`
    /// describes (sRGB without one), converted into the working space.
    pub fn from_image(image: &DynamicImage, icc: Option<&[u8]>) -> Self {
        Self::new(
            image.width(),
            image.height(),
            Pixels::Image(cms::to_working(image, icc)),
        )
    }

    /// An image already in the working space (no conversion).
    pub fn from_working(image: DynamicImage) -> Self {
        Self::new(image.width(), image.height(), Pixels::Image(image))
    }

    pub(crate) fn from_half(width: u32, height: u32, data: Vec<u32>, raw: Option<RawColor>) -> Self {
        Self::new(width, height, Pixels::Half { data, raw })
    }

    /// A RAW file's sensor data, shown upright for EXIF `orientation`.
    /// `None` for layouts the GPU demosaic doesn't handle (non-RGB or
    /// float data), which should be opened as ordinary images instead.
    pub fn from_raw(raw: RawImage, orientation: u8) -> Option<Self> {
        let RawPhotometricInterpretation::Cfa(config) = &raw.photometric else {
            return None;
        };
        let cfa = &config.cfa;
        if raw.cpp != 1 || !cfa.is_rgb() || cfa.width * cfa.height > 36 {
            return None;
        }
        if !matches!(raw.data, RawImageData::Integer(_)) {
            return None;
        }
        let mut pattern = [0.0; 36];
        for row in 0..cfa.height {
            for col in 0..cfa.width {
                pattern[row * cfa.width + col] = cfa.color_at(row, col).min(2) as f32;
            }
        }
        let wb = if raw.wb_coeffs[0].is_nan() || raw.wb_coeffs[1] <= 0.0 {
            [1.0; 3]
        } else {
            let g = raw.wb_coeffs[1];
            [raw.wb_coeffs[0] / g, 1.0, raw.wb_coeffs[2] / g]
        };
        let (_, matrix) = raw.color_matrix_find_first([
            rawler::imgop::xyz::Illuminant::D65,
            rawler::imgop::xyz::Illuminant::D55,
            rawler::imgop::xyz::Illuminant::D50,
            rawler::imgop::xyz::Illuminant::Daylight,
            rawler::imgop::xyz::Illuminant::A,
        ])?;
        let matrix = camera_to_working(&matrix)?;
        let area = raw.crop_area.or(raw.active_area);
        let crop = match area {
            Some(rect) => [rect.p.x, rect.p.y, rect.d.w, rect.d.h].map(|v| v as u32),
            None => [0, 0, raw.width as u32, raw.height as u32],
        };
        if crop[2] == 0
            || crop[3] == 0
            || (crop[0] + crop[2]) as usize > raw.width
            || (crop[1] + crop[3]) as usize > raw.height
        {
            return None;
        }
        let orient = match orientation {
            2 => [false, true, false],
            3 => [false, true, true],
            4 => [false, false, true],
            5 => [true, false, false],
            6 => [true, false, true],
            7 => [true, true, true],
            8 => [true, true, false],
            _ => [false, false, false],
        };
        let (width, height) = if orient[0] {
            (crop[3], crop[2])
        } else {
            (crop[2], crop[3])
        };
        let mosaic = RawMosaic {
            black: raw.blacklevel.as_bayer_array(),
            white: raw.whitelevel.as_bayer_array(),
            wb,
            cfa: (cfa.width as u32, cfa.height as u32, pattern),
            crop,
            orient,
            exif_orientation: orientation,
            color: RawColor { matrix, clip: wb },
            raw,
        };
        Some(Self::new(width, height, Pixels::Raw(mosaic)))
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Whether this came from a RAW file's sensor data.
    pub fn is_raw(&self) -> bool {
        match &*self.pixels {
            Pixels::Raw(_) => true,
            Pixels::Half { raw, .. } => raw.is_some(),
            Pixels::Image(_) => false,
        }
    }

    /// The photo as a working-space image the CPU pipeline can use.
    /// Converts (and for RAW files, develops on the CPU) when needed.
    pub(crate) fn cpu_image(&self) -> Cow<'_, DynamicImage> {
        match &*self.pixels {
            Pixels::Image(image) => Cow::Borrowed(image),
            Pixels::Half { data, raw } => {
                let mut out = Vec::with_capacity(self.width as usize * self.height as usize * 3);
                for px in data.chunks_exact(2) {
                    let c = [
                        f16::from_bits(px[0] as u16).to_f32(),
                        f16::from_bits((px[0] >> 16) as u16).to_f32(),
                        f16::from_bits(px[1] as u16).to_f32(),
                    ];
                    let c = match raw {
                        Some(color) => color.develop(c),
                        None => c,
                    };
                    out.extend(c.map(|v| (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16));
                }
                Cow::Owned(DynamicImage::ImageRgb16(
                    ImageBuffer::from_raw(self.width, self.height, out).expect("sized above"),
                ))
            }
            Pixels::Raw(mosaic) => Cow::Owned(develop_on_cpu(mosaic, self.width, self.height)),
        }
    }

    /// 8-bit working-space pixels, e.g. for picking a spot's source.
    pub fn to_rgb8(&self) -> RgbImage {
        self.cpu_image().to_rgb8()
    }
}

/// rawler's own (CPU) development, for when there's no GPU.
fn develop_on_cpu(mosaic: &RawMosaic, width: u32, height: u32) -> DynamicImage {
    let developed = rawler::imgop::develop::RawDevelop::default()
        .develop_intermediate(&mosaic.raw)
        .ok()
        .and_then(|i| i.to_dynamic_image());
    let Some(mut image) = developed else {
        return DynamicImage::ImageRgb16(ImageBuffer::new(width, height));
    };
    if let Some(orientation) = image::metadata::Orientation::from_exif(mosaic.exif_orientation) {
        image.apply_orientation(orientation);
    }
    cms::to_working(&image, None)
}

/// Camera RGB -> linear working RGB from the camera's XYZ -> camera
/// matrix, normalised (as rawler and dcraw do) so white-balanced neutral
/// maps to neutral.
fn camera_to_working(xyz_to_cam: &[f32]) -> Option<[f32; 9]> {
    if xyz_to_cam.len() < 9 {
        return None;
    }
    let w = &cms::WORKING_TO_XYZ;
    let mut rgb_to_cam = [[0.0f32; 3]; 3];
    for (i, row) in rgb_to_cam.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..3).map(|k| xyz_to_cam[i * 3 + k] * w[k][j]).sum();
        }
        let sum: f32 = row.iter().sum();
        if sum.abs() < 1e-6 {
            return None;
        }
        for v in row.iter_mut() {
            *v /= sum;
        }
    }
    let inverse = invert(rgb_to_cam)?;
    Some([
        inverse[0][0], inverse[0][1], inverse[0][2],
        inverse[1][0], inverse[1][1], inverse[1][2],
        inverse[2][0], inverse[2][1], inverse[2][2],
    ])
}

fn invert(m: [[f32; 3]; 3]) -> Option<[[f32; 3]; 3]> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    Some([
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inv,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inv,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inv,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv,
        ],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_camera_rgb_stays_neutral() {
        // A made-up camera matrix: any normalised one must map white
        // (white-balanced 1, 1, 1) to white.
        let xyz_to_cam = [0.9, -0.3, -0.1, -0.4, 1.3, 0.1, -0.05, 0.2, 0.6];
        let m = camera_to_working(&xyz_to_cam).unwrap();
        for row in m.chunks(3) {
            assert!((row.iter().sum::<f32>() - 1.0).abs() < 1e-4, "{m:?}");
        }
    }

    /// The linear scene the synthetic RAW file records: smooth, so the
    /// demosaic can reproduce it closely.
    fn scene(x: u32, y: u32) -> [f32; 3] {
        let (fx, fy) = (x as f32 / 96.0, y as f32 / 64.0);
        [0.05 + 0.6 * fx, 0.1 + 0.5 * fy, 0.4 - 0.3 * fx * fy]
    }

    /// A 96x64 RGGB sensor image of `scene`, 14-bit with a black level of
    /// 512 and as-shot white balance (2.0, 1.0, 1.5), whose colour matrix
    /// makes camera RGB the working space's linear RGB.
    pub(crate) fn synthetic_raw() -> RawImage {
        use rawler::{
            cfa::{PlaneColor, CFA},
            decoders::Camera,
            imgop::xyz::Illuminant,
            pixarray::PixU16,
            rawimage::{BlackLevel, CFAConfig, WhiteLevel},
        };
        let (w, h) = (96u32, 64u32);
        let wb = [2.0f32, 1.0, 1.5];
        let (black, white) = (512.0f32, 16383.0f32);
        let data: Vec<u16> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| {
                let c = match (x % 2, y % 2) {
                    (0, 0) => 0,
                    (1, 1) => 2,
                    _ => 1,
                };
                let v = scene(x, y)[c] / wb[c];
                (black + v * (white - black)).round() as u16
            })
            .collect();
        let cfa = CFA::new("RGGB");
        let mut raw = RawImage::new(
            Camera::default(),
            PixU16::new_with(data, w as usize, h as usize),
            1,
            [wb[0], wb[1], wb[2], f32::NAN],
            RawPhotometricInterpretation::Cfa(CFAConfig::new(&cfa, &PlaneColor::new("RGB"))),
            Some(BlackLevel::new(&[512u16], 1, 1, 1)),
            Some(WhiteLevel::new(vec![16383])),
            false,
        );
        let to_xyz = cms::WORKING_TO_XYZ;
        let m = [to_xyz[0], to_xyz[1], to_xyz[2]];
        let inverse = invert(m).unwrap();
        raw.color_matrix
            .insert(Illuminant::D65, inverse.iter().flatten().copied().collect());
        raw
    }

    #[test]
    fn synthetic_raw_develops_to_its_scene() {
        if crate::gpu_adapter_name().is_none() {
            return;
        }
        let source = DevelopSource::from_raw(synthetic_raw(), 1).expect("supported layout");
        assert_eq!((source.width(), source.height()), (96, 64));
        let rendered = crate::apply(&source, &rustyedit_core::DevelopSettings::default());
        // `apply` writes sRGB; compare in the working space instead.
        let working = crate::render_export(
            &source,
            &rustyedit_core::DevelopSettings::default(),
            None,
            None,
            &crate::OutputSpace::DisplayP3,
        );
        assert_eq!(rendered.dimensions(), working.dimensions());
        let mut worst = 0;
        for y in 2..62 {
            for x in 2..94 {
                let expected = scene(x, y).map(|v| (cms::encode_srgb(v) * 255.0).round() as i32);
                let got = working.get_pixel(x, y).0;
                for c in 0..3 {
                    worst = worst.max((got[c] as i32 - expected[c]).abs());
                }
            }
        }
        eprintln!("synthetic RAW: worst channel error {worst} levels");
        assert!(worst <= 3, "worst channel error {worst}");
    }

    #[test]
    fn raw_exif_orientation_turns_the_picture() {
        if crate::gpu_adapter_name().is_none() {
            return;
        }
        let upright = DevelopSource::from_raw(synthetic_raw(), 1).unwrap();
        let turned = DevelopSource::from_raw(synthetic_raw(), 6).unwrap();
        assert_eq!((turned.width(), turned.height()), (64, 96));
        let settings = rustyedit_core::DevelopSettings::default();
        let space = crate::OutputSpace::DisplayP3;
        let a = crate::render_export(&upright, &settings, None, None, &space);
        let b = crate::render_export(&turned, &settings, None, None, &space);
        // Orientation 6: turned a quarter clockwise, so the upright
        // picture's top-left ends up top-right.
        for (x, y) in [(10u32, 20u32), (50, 5), (80, 40)] {
            let pa = a.get_pixel(x, y);
            let pb = b.get_pixel(63 - y, x);
            for c in 0..3 {
                assert!(pa[c].abs_diff(pb[c]) <= 1, "({x}, {y}): {pa:?} vs {pb:?}");
            }
        }
    }
}
