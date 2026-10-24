//! Soft-proofing and the out-of-gamut warning.
//!
//! A proof shows how a photo would look in another colour space: a
//! printer's CMYK profile, or a smaller RGB space like sRGB on a wide-gamut
//! display. The live preview frame (already in the display's space) is sent
//! through the proof profile and back, which clips or compresses whatever
//! the proof space can't hold. The gamut warning marks the pixels that
//! changed noticeably on the way, i.e. the colours that would be lost.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use image::RgbaImage;
use moxcms::{DataColorSpace, Layout, RenderingIntent, Transform8BitExecutor, TransformOptions};

use crate::cms::{display_space, OutputSpace};

/// How colours outside the proof space are brought inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ProofIntent {
    /// Keeps in-gamut colours exact and clips the rest (what most print
    /// workflows use with black point compensation).
    #[default]
    Relative,
    /// Compresses the whole range to keep relationships between colours.
    Perceptual,
}

impl ProofIntent {
    pub const ALL: [ProofIntent; 2] = [ProofIntent::Relative, ProofIntent::Perceptual];

    pub fn label(&self) -> &'static str {
        match self {
            ProofIntent::Relative => "Relative",
            ProofIntent::Perceptual => "Perceptual",
        }
    }

    fn moxcms(self) -> RenderingIntent {
        match self {
            ProofIntent::Relative => RenderingIntent::RelativeColorimetric,
            ProofIntent::Perceptual => RenderingIntent::Perceptual,
        }
    }
}

/// What to proof against, and what to show of it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProofSettings {
    /// The space being simulated: a built-in one, or a profile (RGB, grey
    /// or CMYK) from a file.
    pub target: OutputSpace,
    pub intent: ProofIntent,
    /// Show the photo as the proof space would reproduce it.
    pub simulate: bool,
    /// Paint the colours the proof space can't hold.
    pub warn: bool,
}

/// How far (in 8-bit levels) a pixel must move on the round trip to count
/// as out of gamut. Lookup-table profiles never round-trip perfectly, so a
/// few levels of drift is normal.
const GAMUT_TOLERANCE: i32 = 10;
/// The out-of-gamut warning colour and how strongly it covers the pixel.
const WARNING: [u8; 3] = [255, 0, 255];
const WARNING_STRENGTH: f32 = 0.7;

struct Pipeline {
    /// Display -> proof space (the proof's own layout: 4 or 5 samples).
    forward: Arc<Transform8BitExecutor>,
    /// Proof space -> display RGBA.
    back: Arc<Transform8BitExecutor>,
    /// Samples per pixel in the proof space.
    channels: usize,
}

type Key = (OutputSpace, OutputSpace, ProofIntent);

fn pipeline(display: &OutputSpace, proof: &ProofSettings) -> Option<Arc<Pipeline>> {
    static CACHE: OnceLock<Mutex<HashMap<Key, Option<Arc<Pipeline>>>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    cache
        .entry((display.clone(), proof.target.clone(), proof.intent))
        .or_insert_with(|| build(display, proof).map(Arc::new))
        .clone()
}

fn build(display: &OutputSpace, proof: &ProofSettings) -> Option<Pipeline> {
    let display = display.profile()?;
    let target = proof.target.profile()?;
    let (layout, channels) = match target.color_space {
        DataColorSpace::Cmyk => (Layout::Cmyka, 4),
        DataColorSpace::Rgb => (Layout::Rgba, 4),
        _ => return None,
    };
    let forward = display
        .create_transform_8bit(
            Layout::Rgba,
            &target,
            layout,
            TransformOptions {
                rendering_intent: proof.intent.moxcms(),
                ..Default::default()
            },
        )
        .ok()?;
    let back = target
        .create_transform_8bit(
            layout,
            &display,
            Layout::Rgba,
            TransformOptions {
                rendering_intent: RenderingIntent::RelativeColorimetric,
                ..Default::default()
            },
        )
        .ok()?;
    Some(Pipeline {
        forward,
        back,
        channels,
    })
}

// ---- CMYK (printer) profiles ------------------------------------------------
//
// moxcms reads a CMYK profile (ink -> colour) but cannot convert *to* ink, so
// a printer's gamut is mapped out by sampling it instead: a grid of ink
// mixes is converted to CIE Lab, and for every lightness and hue the most
// saturated colour the inks reach is kept. A pixel beyond that boundary is
// out of gamut, and the proof pulls it back onto it (keeping lightness and
// hue, like a relative-colorimetric conversion with black point
// compensation).

const L_BINS: usize = 64;
const H_BINS: usize = 120;
/// Inks per axis of the sampling grid (13^4 = 28 561 mixes).
const INK_STEPS: usize = 13;
/// Lab units a colour may sit beyond the boundary before it counts as out.
const CHROMA_TOLERANCE: f32 = 3.0;
/// moxcms's Lab/XYZ profile carries XYZ halved (ICC's PCS encoding).
const PCS_SCALE: f32 = 1.999_969_5;
const D50: [f32; 3] = [0.9642, 1.0, 0.8249];

struct Gamut {
    /// Highest chroma per (lightness bin, hue bin); 0 where nothing is
    /// reachable.
    max_chroma: Vec<f32>,
    /// Darkest lightness the inks reach.
    l_min: f32,
}

fn lab_f(t: f32) -> f32 {
    if t > 0.008_856 {
        t.cbrt()
    } else {
        7.787 * t + 16.0 / 116.0
    }
}

fn lab_f_inv(t: f32) -> f32 {
    let cubed = t * t * t;
    if cubed > 0.008_856 {
        cubed
    } else {
        (t - 16.0 / 116.0) / 7.787
    }
}

/// CIE XYZ (D50) -> (L, a, b).
fn xyz_to_lab(xyz: [f32; 3]) -> [f32; 3] {
    let f = [
        lab_f(xyz[0] / D50[0]),
        lab_f(xyz[1] / D50[1]),
        lab_f(xyz[2] / D50[2]),
    ];
    [
        116.0 * f[1] - 16.0,
        500.0 * (f[0] - f[1]),
        200.0 * (f[1] - f[2]),
    ]
}

fn lab_to_xyz(lab: [f32; 3]) -> [f32; 3] {
    let fy = (lab[0] + 16.0) / 116.0;
    let fx = fy + lab[1] / 500.0;
    let fz = fy - lab[2] / 200.0;
    [
        lab_f_inv(fx) * D50[0],
        lab_f_inv(fy) * D50[1],
        lab_f_inv(fz) * D50[2],
    ]
}

impl Gamut {
    fn bins(l: f32, hue: f32) -> (usize, usize) {
        let lb = ((l / 100.0).clamp(0.0, 1.0) * (L_BINS - 1) as f32).round() as usize;
        let hb = (((hue + std::f32::consts::PI) / std::f32::consts::TAU) * H_BINS as f32) as usize;
        (lb.min(L_BINS - 1), hb.min(H_BINS - 1))
    }

    fn build(profile: &moxcms::ColorProfile) -> Option<Self> {
        // moxcms can't go from ink straight to Lab, so the inks are read
        // into ProPhoto RGB (wider than any printer) and on to XYZ.
        let wide = moxcms::ColorProfile::new_pro_photo_rgb();
        let lab = moxcms::ColorProfile::new_lab();
        let to_wide = profile
            .create_transform_f32(
                Layout::Cmyka,
                &wide,
                Layout::Rgb,
                TransformOptions {
                    rendering_intent: RenderingIntent::RelativeColorimetric,
                    ..Default::default()
                },
            )
            .ok()?;
        let wide_to_pcs = wide
            .create_transform_f32(Layout::Rgb, &lab, Layout::Rgb, TransformOptions::default())
            .ok()?;
        let n = INK_STEPS;
        let step = 1.0 / (n - 1) as f32;
        // Cmyka is four samples per pixel: c, m, y, k.
        let mut inks = Vec::with_capacity(n * n * n * n * 4);
        for c in 0..n {
            for m in 0..n {
                for y in 0..n {
                    for k in 0..n {
                        inks.extend_from_slice(&[
                            c as f32 * step,
                            m as f32 * step,
                            y as f32 * step,
                            k as f32 * step,
                        ]);
                    }
                }
            }
        }
        let mut rgb = vec![0f32; inks.len() / 4 * 3];
        to_wide.transform(&inks, &mut rgb).ok()?;
        let mut pcs = vec![0f32; rgb.len()];
        wide_to_pcs.transform(&rgb, &mut pcs).ok()?;

        let mut max_chroma = vec![0f32; L_BINS * H_BINS];
        let mut l_min = 100.0f32;
        for px in pcs.chunks_exact(3) {
            let [l, a, b] = xyz_to_lab([px[0] * PCS_SCALE, px[1] * PCS_SCALE, px[2] * PCS_SCALE]);
            if !l.is_finite() {
                continue;
            }
            l_min = l_min.min(l);
            let (lb, hb) = Self::bins(l, b.atan2(a));
            let slot = &mut max_chroma[lb * H_BINS + hb];
            *slot = slot.max(a.hypot(b));
        }
        // Fill the gaps the sampling leaves between neighbouring bins.
        for _ in 0..3 {
            let snapshot = max_chroma.clone();
            for lb in 0..L_BINS {
                for hb in 0..H_BINS {
                    if snapshot[lb * H_BINS + hb] > 0.0 {
                        continue;
                    }
                    let around = [
                        (lb.wrapping_sub(1), hb),
                        (lb + 1, hb),
                        (lb, (hb + H_BINS - 1) % H_BINS),
                        (lb, (hb + 1) % H_BINS),
                    ]
                    .into_iter()
                    .filter(|(l, _)| *l < L_BINS)
                    .map(|(l, h)| snapshot[l * H_BINS + h])
                    .fold(0.0f32, f32::max);
                    if around > 0.0 && (lb as f32 / (L_BINS - 1) as f32 * 100.0) >= l_min {
                        max_chroma[lb * H_BINS + hb] = around * 0.97;
                    }
                }
            }
        }
        Some(Self { max_chroma, l_min })
    }

    /// The most chroma reachable at lightness `l` and hue angle `hue`.
    fn boundary(&self, l: f32, hue: f32) -> f32 {
        let (lb, hb) = Self::bins(l, hue);
        // Neighbouring hue bins are close; the larger avoids stair-steps.
        let prev = (hb + H_BINS - 1) % H_BINS;
        let next = (hb + 1) % H_BINS;
        let at = |h: usize| self.max_chroma[lb * H_BINS + h];
        at(hb).max(at(prev).min(at(next))).max(at(hb) * 0.0)
    }
}

fn gamut(profile: &OutputSpace) -> Option<Arc<Gamut>> {
    static CACHE: OnceLock<Mutex<HashMap<OutputSpace, Option<Arc<Gamut>>>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    cache
        .entry(profile.clone())
        .or_insert_with(|| {
            profile
                .profile()
                .and_then(|p| Gamut::build(&p))
                .map(Arc::new)
        })
        .clone()
}

/// A CMYK proof of an RGBA frame (RGB bytes in `original`, display space).
/// Returns the proofed RGBA and which pixels are out of gamut.
fn proof_cmyk(
    original: &[u8],
    display: &OutputSpace,
    gamut: &Gamut,
    intent: ProofIntent,
) -> Option<(Vec<u8>, Vec<bool>)> {
    let display = display.profile()?;
    let lab = moxcms::ColorProfile::new_lab();
    let to_pcs = display
        .create_transform_f32(Layout::Rgb, &lab, Layout::Rgb, TransformOptions::default())
        .ok()?;
    let from_pcs = lab
        .create_transform_f32(
            Layout::Rgb,
            &display,
            Layout::Rgb,
            TransformOptions::default(),
        )
        .ok()?;

    let pixels = original.len() / 4;
    let rgb: Vec<f32> = original
        .chunks_exact(4)
        .flat_map(|p| {
            [
                p[0] as f32 / 255.0,
                p[1] as f32 / 255.0,
                p[2] as f32 / 255.0,
            ]
        })
        .collect();
    let mut pcs = vec![0f32; pixels * 3];
    let mut out_rgb = vec![0f32; pixels * 3];
    let mut outside = vec![false; pixels];

    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let band = pixels.div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        for (((rgb, pcs), out), outside) in rgb
            .chunks(band * 3)
            .zip(pcs.chunks_mut(band * 3))
            .zip(out_rgb.chunks_mut(band * 3))
            .zip(outside.chunks_mut(band))
        {
            let (to_pcs, from_pcs) = (&to_pcs, &from_pcs);
            scope.spawn(move || {
                if to_pcs.transform(rgb, pcs).is_err() {
                    out.copy_from_slice(rgb);
                    return;
                }
                for (i, px) in pcs.chunks_exact_mut(3).enumerate() {
                    let [l, a, b] =
                        xyz_to_lab([px[0] * PCS_SCALE, px[1] * PCS_SCALE, px[2] * PCS_SCALE]);
                    // Black point compensation: the screen's black becomes
                    // the ink's darkest.
                    let l2 = gamut.l_min + l.clamp(0.0, 100.0) * (100.0 - gamut.l_min) / 100.0;
                    let (hue, chroma) = (b.atan2(a), a.hypot(b));
                    let limit = gamut.boundary(l2, hue);
                    outside[i] = chroma > limit + CHROMA_TOLERANCE;
                    let kept = if chroma <= limit {
                        chroma
                    } else if intent == ProofIntent::Perceptual && limit > 1.0 {
                        // A soft knee instead of a hard clip.
                        let knee = limit * 0.8;
                        if chroma <= knee {
                            chroma
                        } else {
                            knee + (limit - knee) * ((chroma - knee) / (limit - knee)).tanh()
                        }
                    } else {
                        limit
                    };
                    let xyz = lab_to_xyz([l2, kept * hue.cos(), kept * hue.sin()]);
                    px[0] = xyz[0] / PCS_SCALE;
                    px[1] = xyz[1] / PCS_SCALE;
                    px[2] = xyz[2] / PCS_SCALE;
                }
                if from_pcs.transform(pcs, out).is_err() {
                    out.copy_from_slice(rgb);
                }
            });
        }
    });

    let mut proofed = original.to_vec();
    for (px, rgb) in proofed.chunks_exact_mut(4).zip(out_rgb.chunks_exact(3)) {
        for c in 0..3 {
            px[c] = (rgb[c].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }
    Some((proofed, outside))
}

/// Proofs a live-preview frame (BGRA, in the display's colour space) in
/// place. Does nothing when neither the simulation nor the warning is on, or
/// when the profile can't be used.
pub fn apply_proof(frame: &mut RgbaImage, proof: &ProofSettings) {
    apply_proof_for(frame, proof, &display_space());
}

pub(crate) fn apply_proof_for(frame: &mut RgbaImage, proof: &ProofSettings, display: &OutputSpace) {
    if !proof.simulate && !proof.warn {
        return;
    }
    let (width, height) = frame.dimensions();
    let pixels = width as usize * height as usize;
    if pixels == 0 {
        return;
    }

    // BGRA -> RGBA for the transforms.
    let mut original = frame.as_raw().clone();
    for px in original.chunks_exact_mut(4) {
        px.swap(0, 2);
    }

    // Printer (CMYK) profiles are mapped by their reachable colours.
    let is_cmyk = proof
        .target
        .profile()
        .is_some_and(|p| p.color_space == DataColorSpace::Cmyk);
    if is_cmyk {
        let Some(gamut) = gamut(&proof.target) else {
            return;
        };
        let Some((proofed, outside)) = proof_cmyk(&original, display, &gamut, proof.intent) else {
            return;
        };
        paint(
            frame.as_mut(),
            &original,
            &proofed,
            |i| proof.warn && outside[i],
            proof.simulate,
        );
        return;
    }

    let Some(pipeline) = pipeline(display, proof) else {
        return;
    };
    let mut device = vec![0u8; pixels * pipeline.channels];
    let mut round_trip = vec![0u8; pixels * 4];
    let rows_per_band = (height as usize).div_ceil(
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4),
    );
    let bands = rows_per_band * width as usize;
    std::thread::scope(|scope| {
        for ((src, dev), out) in original
            .chunks(bands * 4)
            .zip(device.chunks_mut(bands * pipeline.channels))
            .zip(round_trip.chunks_mut(bands * 4))
        {
            let pipeline = &pipeline;
            scope.spawn(move || {
                if pipeline.forward.transform(src, dev).is_ok() {
                    let _ = pipeline.back.transform(dev, out);
                }
            });
        }
    });

    paint(
        frame.as_mut(),
        &original,
        &round_trip,
        |i| {
            proof.warn && {
                let (o, p) = (&original[i * 4..i * 4 + 3], &round_trip[i * 4..i * 4 + 3]);
                (0..3).any(|c| (o[c] as i32 - p[c] as i32).abs() > GAMUT_TOLERANCE)
            }
        },
        proof.simulate,
    );
}

/// Writes the result into the BGRA frame: the proofed colour (or the
/// original when only warning), with out-of-gamut pixels tinted.
fn paint(
    frame: &mut [u8],
    original: &[u8],
    proofed: &[u8],
    out_of_gamut: impl Fn(usize) -> bool,
    simulate: bool,
) {
    for (i, px) in frame.chunks_exact_mut(4).enumerate() {
        let shown = if simulate {
            &proofed[i * 4..i * 4 + 3]
        } else {
            &original[i * 4..i * 4 + 3]
        };
        let rgb: [u8; 3] = if out_of_gamut(i) {
            std::array::from_fn(|c| {
                (shown[c] as f32 * (1.0 - WARNING_STRENGTH) + WARNING[c] as f32 * WARNING_STRENGTH)
                    .round() as u8
            })
        } else {
            [shown[0], shown[1], shown[2]]
        };
        px[0] = rgb[2];
        px[1] = rgb[1];
        px[2] = rgb[0];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn frame(colours: &[[u8; 3]]) -> RgbaImage {
        // BGRA, as the preview delivers it.
        RgbaImage::from_fn(colours.len() as u32, 1, |x, _| {
            let [r, g, b] = colours[x as usize];
            Rgba([b, g, r, 255])
        })
    }

    fn rgb_at(frame: &RgbaImage, x: u32) -> [u8; 3] {
        let p = frame.get_pixel(x, 0).0;
        [p[2], p[1], p[0]]
    }

    fn settings(target: OutputSpace, simulate: bool, warn: bool) -> ProofSettings {
        ProofSettings {
            target,
            intent: ProofIntent::Relative,
            simulate,
            warn,
        }
    }

    #[test]
    fn proofing_to_the_display_space_changes_nothing() {
        let colours = [[255, 0, 0], [12, 200, 90], [128, 128, 128], [250, 240, 10]];
        let mut shown = frame(&colours);
        let before = shown.clone();
        apply_proof_for(
            &mut shown,
            &settings(OutputSpace::Srgb, true, true),
            &OutputSpace::Srgb,
        );
        for x in 0..colours.len() as u32 {
            let (a, b) = (rgb_at(&shown, x), rgb_at(&before, x));
            for c in 0..3 {
                assert!(
                    (a[c] as i32 - b[c] as i32).abs() <= 3,
                    "{x}: {a:?} vs {b:?}"
                );
            }
        }
    }

    #[test]
    fn colours_outside_a_smaller_space_are_warned_about_and_clipped() {
        // A saturated P3 green and red can't be shown in sRGB; grey can.
        let mut shown = frame(&[[0, 255, 0], [255, 0, 0], [128, 128, 128]]);
        apply_proof_for(
            &mut shown,
            &settings(OutputSpace::Srgb, false, true),
            &OutputSpace::DisplayP3,
        );
        let green = rgb_at(&shown, 0);
        assert!(
            green[0] > 150 && green[2] > 150,
            "green is painted: {green:?}"
        );
        assert!(
            rgb_at(&shown, 1)[2] > 150,
            "red is painted: {:?}",
            rgb_at(&shown, 1)
        );
        let grey = rgb_at(&shown, 2);
        assert!(
            (grey[0] as i32 - 128).abs() <= 3 && (grey[1] as i32 - 128).abs() <= 3,
            "{grey:?}"
        );

        // Simulating alone pulls the colour inside sRGB without marking it.
        let mut simulated = frame(&[[0, 255, 0]]);
        apply_proof_for(
            &mut simulated,
            &settings(OutputSpace::Srgb, true, false),
            &OutputSpace::DisplayP3,
        );
        // sRGB's green, seen on a P3 display, has some red and blue in it.
        let green = rgb_at(&simulated, 0);
        assert!(green[0] > 60 && green[2] > 30, "{green:?}");
    }

    /// A real printer-style profile (CMYK, lookup tables) when the system
    /// has one: saturated screen colours can't be printed, greys can.
    #[test]
    fn a_cmyk_profile_flags_unprintable_colours() {
        let Ok(bytes) =
            std::fs::read("/System/Library/ColorSync/Profiles/Generic CMYK Profile.icc")
        else {
            eprintln!("no system CMYK profile; skipping");
            return;
        };
        let mut shown = frame(&[[0, 255, 0], [0, 120, 255], [128, 128, 128]]);
        apply_proof_for(
            &mut shown,
            &settings(OutputSpace::Profile(bytes), true, true),
            &OutputSpace::Srgb,
        );
        // Pure screen green and a vivid blue are well outside CMYK.
        assert!(
            rgb_at(&shown, 0)[0] > 150 && rgb_at(&shown, 0)[2] > 150,
            "{:?}",
            rgb_at(&shown, 0)
        );
        assert!(rgb_at(&shown, 1)[0] > 150, "{:?}", rgb_at(&shown, 1));
        // A mid grey is reproduced, not warned about (and stays grey).
        let grey = rgb_at(&shown, 2);
        assert!(grey[0].abs_diff(grey[2]) < 20 && grey[1] < 200, "{grey:?}");
    }

    #[test]
    fn nothing_happens_with_both_switches_off() {
        let mut shown = frame(&[[0, 255, 0]]);
        let before = shown.clone();
        apply_proof_for(
            &mut shown,
            &settings(OutputSpace::Srgb, false, false),
            &OutputSpace::DisplayP3,
        );
        assert_eq!(shown, before);
    }
}
