//! Looks taken from photos: "Match look" and "New preset from photo".
//!
//! A reference photo is boiled down to [`LookStats`]: how its tones are
//! spread, the tint of its near-grey areas in the shadows, midtones and
//! highlights (where a grade or colour cast shows, whatever the scene), how
//! colourful it is, per-colour shifts, darkened corners, and whether it's
//! black and white.
//!
//! [`match_look`] then fits a photo to those statistics, one setting at a
//! time, by rendering it through the real pipeline and measuring the result
//! (the same approach as Auto Tone): Exposure for overall brightness, White
//! Balance for the overall cast, Saturation, a tone curve for the exact
//! tonal spread (faded blacks, soft highlights), Color Grading per tonal
//! range, the Color Mixer, then Vignette.
//!
//! [`preset_from_look`] fits a synthetic "typical neutral photo" instead,
//! so the result is a preset that can go on any photo.
//!
//! It matches statistics, not content: it can't tell skin from sky.

use image::{imageops::FilterType, DynamicImage, Rgb, RgbImage};
use rustyedit_core::{band_weight, ColorBand, DevelopSection, DevelopSettings, ToneCurve, ToneZone};

use crate::{
    auto::{newton2, percentile, solve_increasing},
    color::{hsl_to_rgb, rgb_to_hsl},
    prepare_preview, render_export, DevelopSource, OutputSpace,
};

/// Long edge the reference and the photo are measured at.
const ANALYSIS_EDGE: u32 = 384;
/// Pixels with less chroma than this count as near-grey: where a colour
/// cast or grade shows regardless of what the scene is.
const NEUTRAL_CHROMA: f32 = 0.06;
/// A tonal range needs this share of near-grey pixels to judge its tint.
const MIN_ZONE_SHARE: f32 = 0.01;
/// A colour range needs this share of the picture (in both photos) before
/// the Color Mixer copies it.
const MIN_BAND_SHARE: f32 = 0.03;
/// How much of a per-colour difference the Color Mixer copies: scenes
/// differ, so it only leans towards the reference.
const MIXER_DAMPING: f32 = 0.6;
/// Tonal positions the tone curve is fitted at.
const CURVE_QUANTILES: [f32; 5] = [0.02, 0.2, 0.5, 0.8, 0.98];
/// Furthest a curve point moves from the straight line. Exposure already
/// matched the overall brightness; the curve only shapes it, and images
/// with gaps in their tones (graphics, screenshots) would otherwise give
/// wild, kinked curves.
const CURVE_REACH: f32 = 0.25;
/// Most the black end is lifted (a faded look) and the white end lowered.
const CURVE_END_REACH: f32 = 0.2;
/// Steepest a curve segment may be, relative to the straight line, so no
/// range of tones is stretched harshly.
const CURVE_MAX_SLOPE: f32 = 2.5;

/// What a colour range looks like in a photo.
#[derive(Debug, Clone, Copy, PartialEq)]
struct BandStats {
    /// Share of the picture in this range (weighted by saturation).
    share: f32,
    /// Average hue, in degrees from the range's centre.
    hue_offset: f32,
    saturation: f32,
    lightness: f32,
}

/// The measurable look of a photo (see the module docs).
#[derive(Debug, Clone, PartialEq)]
pub struct LookStats {
    /// Luminance histogram, 0-255.
    luma: [u32; 256],
    /// Average (Cb, Cr) of near-grey pixels in the shadows, midtones and
    /// highlights ([`ToneZone::ALL`] order), where there are enough.
    tints: [Option<[f32; 2]>; 3],
    /// The same over all tones.
    tint: Option<[f32; 2]>,
    /// Average chroma of every pixel: how colourful the picture is.
    chroma: f32,
    /// Black and white.
    mono: bool,
    bands: [Option<BandStats>; 8],
    /// Brightness of the corners relative to the centre.
    vignette: f32,
}

/// One pixel, measured.
struct Pixel {
    /// Luminance, 0-1.
    y: f32,
    chroma: [f32; 2],
    hue: f32,
    saturation: f32,
    lightness: f32,
    /// Distance from the centre, 1 at the corners.
    radius: f32,
}

impl Pixel {
    fn is_neutral(&self) -> bool {
        self.chroma[0].hypot(self.chroma[1]) < NEUTRAL_CHROMA
    }
}

fn pixels(img: &RgbImage) -> Vec<Pixel> {
    let (w, h) = (img.width() as f32, img.height() as f32);
    img.enumerate_pixels()
        .map(|(x, yy, p)| {
            let [r, g, b] = p.0.map(|c| c as f32 / 255.0);
            let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let chroma = [(b - y) / 1.8556, (r - y) / 1.5748];
            let (hue, saturation, lightness) = rgb_to_hsl(r, g, b);
            let dx = (x as f32 + 0.5) / w * 2.0 - 1.0;
            let dy = (yy as f32 + 0.5) / h * 2.0 - 1.0;
            Pixel {
                y,
                chroma,
                hue,
                saturation,
                lightness,
                radius: (dx * dx + dy * dy).sqrt() / std::f32::consts::SQRT_2,
            }
        })
        .collect()
}

/// How much a luminance belongs to the shadows, midtones and highlights.
fn zone_weights(y: f32) -> [f32; 3] {
    [
        ((0.35 - y) / 0.35).clamp(0.0, 1.0),
        (1.0 - (y - 0.5).abs() / 0.3).clamp(0.0, 1.0),
        ((y - 0.65) / 0.35).clamp(0.0, 1.0),
    ]
}

/// The average tint of the pixels `use_pixel` picks, per tonal range and
/// overall. Near-black and blown pixels are left out.
fn tints(px: &[Pixel], use_pixel: impl Fn(usize) -> bool) -> ([Option<[f32; 2]>; 3], Option<[f32; 2]>) {
    let mut sums = [[0.0f32; 3]; 3];
    let mut all = [0.0f32; 3];
    for (i, p) in px.iter().enumerate() {
        if !use_pixel(i) || p.y < 0.03 || p.y > 0.97 {
            continue;
        }
        for (zone, w) in zone_weights(p.y).into_iter().enumerate() {
            sums[zone][0] += w * p.chroma[0];
            sums[zone][1] += w * p.chroma[1];
            sums[zone][2] += w;
        }
        all[0] += p.chroma[0];
        all[1] += p.chroma[1];
        all[2] += 1.0;
    }
    let n = px.len().max(1) as f32;
    let average = |[a, b, w]: [f32; 3]| (w / n >= MIN_ZONE_SHARE).then(|| [a / w, b / w]);
    (sums.map(average), average(all))
}

/// The near-grey pixels of a render, fixed while a setting is fitted (the
/// setting itself moves pixels in and out of "grey").
fn neutral_mask(px: &[Pixel]) -> Vec<bool> {
    px.iter().map(Pixel::is_neutral).collect()
}

impl LookStats {
    /// Whether the picture is black and white (no colour to speak of).
    pub fn is_mono(&self) -> bool {
        self.mono
    }

    /// The look of a reference photo.
    pub fn of_image(image: &DynamicImage) -> Self {
        let small = if image.width().max(image.height()) > ANALYSIS_EDGE {
            image.resize(ANALYSIS_EDGE, ANALYSIS_EDGE, FilterType::Triangle)
        } else {
            image.clone()
        };
        Self::of_rgb8(&small.to_rgb8())
    }

    fn of_rgb8(img: &RgbImage) -> Self {
        let px = pixels(img);
        let mut luma = [0u32; 256];
        for p in &px {
            luma[(p.y * 255.0).round().clamp(0.0, 255.0) as usize] += 1;
        }
        let (zone_tints, tint) = tints(&px, |i| px[i].is_neutral());
        let n = px.len().max(1) as f32;
        let magnitudes: Vec<f32> = px.iter().map(|p| p.chroma[0].hypot(p.chroma[1])).collect();
        let chroma = magnitudes.iter().sum::<f32>() / n;
        let colourful = magnitudes.iter().filter(|&&m| m > 0.04).count() as f32 / n;
        let mono = chroma < 0.01 && colourful < 0.01;

        let mut bands = [None; 8];
        for band in ColorBand::ALL {
            let (mut w_sum, mut hue, mut sat, mut light) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for p in &px {
                if p.saturation < 0.15 || !(0.1..=0.9).contains(&p.lightness) {
                    continue;
                }
                let w = band_weight(band, p.hue) * p.saturation;
                if w <= 0.0 {
                    continue;
                }
                let offset = ((p.hue - band.hue() + 540.0) % 360.0) - 180.0;
                w_sum += w;
                hue += w * offset;
                sat += w * p.saturation;
                light += w * p.lightness;
            }
            if w_sum / n >= MIN_BAND_SHARE {
                bands[band.index()] = Some(BandStats {
                    share: w_sum / n,
                    hue_offset: hue / w_sum,
                    saturation: sat / w_sum,
                    lightness: light / w_sum,
                });
            }
        }

        let mean = |pick: &dyn Fn(&Pixel) -> bool| {
            let (sum, count) = px
                .iter()
                .filter(|p| pick(p))
                .fold((0.0f32, 0.0f32), |(s, c), p| (s + p.y, c + 1.0));
            if count > 0.0 {
                sum / count
            } else {
                0.0
            }
        };
        let centre = mean(&|p| p.radius < 0.35);
        let corners = mean(&|p| p.radius > 0.85);
        let vignette = if centre > 0.02 { corners / centre } else { 1.0 };

        Self {
            luma,
            tints: zone_tints,
            tint,
            chroma,
            mono,
            bands,
            vignette,
        }
    }

    fn is_empty(&self) -> bool {
        self.luma.iter().all(|&n| n == 0)
    }
}

/// How [`match_look_with`] fits.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MatchOptions {
    /// Fit Vignette to darkened corners.
    pub(crate) vignette: bool,
    /// How much of each colour range's difference the Color Mixer copies:
    /// partly for a different scene, fully for the same one.
    pub(crate) mixer_strength: f32,
}

impl Default for MatchOptions {
    fn default() -> Self {
        Self {
            vignette: true,
            mixer_strength: MIXER_DAMPING,
        }
    }
}

/// `settings` with its look (tone, colour, grading, effects) fitted so the
/// photo in `source` measures like `reference`. Detail, lens corrections,
/// crop, retouching and masks are left as they were.
pub fn match_look(
    source: &DevelopSource,
    settings: &DevelopSettings,
    reference: &LookStats,
) -> DevelopSettings {
    match_look_with(source, settings, reference, MatchOptions::default())
}

pub(crate) fn match_look_with(
    source: &DevelopSource,
    settings: &DevelopSettings,
    reference: &LookStats,
    options: MatchOptions,
) -> DevelopSettings {
    if reference.is_empty() {
        return *settings;
    }
    let small = prepare_preview(source, ANALYSIS_EDGE);
    let render = |s: &DevelopSettings| render_export(&small, s, None, None, &OutputSpace::Srgb);
    let stats = |s: &DevelopSettings| LookStats::of_rgb8(&render(s));

    let mut s = *settings;
    for section in [
        DevelopSection::Basic,
        DevelopSection::ToneCurve,
        DevelopSection::Color,
        DevelopSection::ColorMixer,
        DevelopSection::ColorGrading,
        DevelopSection::Effects,
    ] {
        section.reset(&mut s);
        s.set_section_enabled(section, true);
    }

    // 1. Exposure: overall brightness (the median).
    let start = stats(&s);
    if start.is_empty() {
        return *settings;
    }
    let want = percentile(&reference.luma, 0.5);
    let median = percentile(&start.luma, 0.5);
    let guess = 50.0 * 2.2 * (want.max(1.0) / median.max(1.0)).log2();
    s.exposure = solve_increasing(
        |e| {
            let mut t = s;
            t.exposure = e;
            percentile(&stats(&t).luma, 0.5) - want
        },
        (-75.0, 75.0),
        (0.0, median - want),
        guess,
        2.0,
        5,
    )
    .round();

    // 2. White balance: the overall tint of the near-greys.
    if let Some(want) = reference.tint {
        let px = pixels(&render(&s));
        let mask = neutral_mask(&px);
        if tints(&px, |i| mask[i]).1.is_some() {
            let [temperature, tint] = newton2(
                |[temperature, tint]| {
                    let mut t = s;
                    t.temperature = temperature;
                    t.tint = tint;
                    let got = tints(&pixels(&render(&t)), |i| mask[i]).1.unwrap_or(want);
                    [255.0 * (got[0] - want[0]), 255.0 * (got[1] - want[1])]
                },
                [0.0, 0.0],
                (-100.0, 100.0),
                10.0,
                0.5,
                5,
            );
            s.temperature = temperature.round();
            s.tint = tint.round();
        }
    }

    // 3. Saturation: how colourful it is overall (or black and white).
    if reference.mono {
        s.saturation = -100.0;
    } else {
        let now = stats(&s).chroma;
        let guess = if now > 1e-4 {
            (reference.chroma / now - 1.0) * 100.0
        } else {
            0.0
        };
        s.saturation = solve_increasing(
            |v| {
                let mut t = s;
                t.saturation = v;
                255.0 * (stats(&t).chroma - reference.chroma)
            },
            (-80.0, 60.0),
            (0.0, 255.0 * (now - reference.chroma)),
            guess,
            1.0,
            4,
        )
        .round();
    }

    // 4. Tone curve: the exact spread of tones.
    if let Some(curve) = fit_curve(&s, reference, &render) {
        s.curves.master = curve;
    }

    // 5. Color Grading: the tint of each tonal range, beyond the overall
    // cast white balance took care of.
    let px = pixels(&render(&s));
    let mask = neutral_mask(&px);
    let (now, _) = tints(&px, |i| mask[i]);
    for zone in [ToneZone::Shadows, ToneZone::Highlights, ToneZone::Midtones] {
        let z = ToneZone::ALL.iter().position(|t| *t == zone).unwrap_or(0);
        let (Some(want), Some(have)) = (reference.tints[z], now[z]) else {
            continue;
        };
        if 255.0 * (want[0] - have[0]).hypot(want[1] - have[1]) < 2.0 {
            continue;
        }
        let set = |t: &mut DevelopSettings, [a, b]: [f32; 2]| {
            let grade = t.grading.zone_mut(zone);
            grade.hue = b.atan2(a).to_degrees().rem_euclid(360.0);
            grade.saturation = a.hypot(b).min(100.0);
        };
        let x = newton2(
            |x| {
                let mut t = s;
                set(&mut t, x);
                let got = tints(&pixels(&render(&t)), |i| mask[i]).0[z].unwrap_or(want);
                [255.0 * (got[0] - want[0]), 255.0 * (got[1] - want[1])]
            },
            [0.0, 0.0],
            (-100.0, 100.0),
            15.0,
            0.75,
            5,
        );
        set(&mut s, x);
        let grade = s.grading.zone_mut(zone);
        grade.hue = grade.hue.round();
        grade.saturation = grade.saturation.round();
        if grade.saturation < 1.0 {
            *grade = Default::default();
        }
    }

    // 6. Color Mixer: lean each colour range towards the reference's.
    if !reference.mono {
        let now = stats(&s);
        for band in ColorBand::ALL {
            let i = band.index();
            let (Some(want), Some(have)) = (reference.bands[i], now.bands[i]) else {
                continue;
            };
            let adjust = &mut s.mixer[i];
            // Hue: ±100 shifts by up to 30 degrees.
            let shift = ((want.hue_offset - have.hue_offset + 540.0) % 360.0) - 180.0;
            adjust.hue = (shift / 30.0 * 100.0 * options.mixer_strength).clamp(-60.0, 60.0).round();
            // Saturation scales by (1 + value / 100).
            if have.saturation > 0.05 {
                adjust.saturation = ((want.saturation / have.saturation - 1.0)
                    * 100.0
                    * options.mixer_strength)
                    .clamp(-60.0, 60.0)
                    .round();
            }
            // Luminance moves lightness by value / 100 × 0.25 × saturation.
            if have.saturation > 0.05 {
                adjust.luminance = ((want.lightness - have.lightness)
                    / (0.25 * have.saturation)
                    * 100.0
                    * options.mixer_strength)
                    .clamp(-50.0, 50.0)
                    .round();
            }
        }
    }

    // 7. Vignette: darker corners, if the reference has them.
    let now = stats(&s).vignette;
    if options.vignette && reference.vignette < now - 0.05 {
        s.vignette = solve_increasing(
            |v| {
                let mut t = s;
                t.vignette = v;
                stats(&t).vignette - reference.vignette
            },
            (-80.0, 0.0),
            (0.0, now - reference.vignette),
            -30.0,
            0.01,
            4,
        )
        .round();
    }

    s
}

/// A tone curve that gives the photo (as `s` renders it) the reference's
/// spread of tones, or `None` if it already has it.
///
/// The curve works on the pipeline's internal values, not on display
/// levels, so the mapping between the two is measured first: a flat curve
/// at height `v` renders the whole picture at one display level.
fn fit_curve(
    s: &DevelopSettings,
    reference: &LookStats,
    render: &impl Fn(&DevelopSettings) -> RgbImage,
) -> Option<ToneCurve> {
    let luma_hist = |img: &RgbImage| LookStats::of_rgb8(img).luma;
    let now = luma_hist(&render(s));
    let gap = CURVE_QUANTILES
        .iter()
        .map(|&q| (percentile(&reference.luma, q) - percentile(&now, q)).abs())
        .fold(0.0, f32::max);
    if gap < 3.0 {
        return None;
    }

    // Display level for each curve height.
    let mut levels: Vec<(f32, f32)> = (0..=10)
        .map(|i| {
            let v = i as f32 / 10.0;
            let mut t = *s;
            t.curves.master = ToneCurve::with_points(&[[0.0, v], [1.0, v]]);
            let hist = luma_hist(&render(&t));
            (v, percentile(&hist, 0.5))
        })
        .collect();
    for i in 1..levels.len() {
        levels[i].1 = levels[i].1.max(levels[i - 1].1);
    }
    if levels[10].1 - levels[0].1 < 50.0 {
        return None;
    }
    // Curve height that renders at display `level`.
    let height = |level: f32| -> f32 {
        if level <= levels[0].1 {
            return 0.0;
        }
        for pair in levels.windows(2) {
            let ((v0, l0), (v1, l1)) = (pair[0], pair[1]);
            if level <= l1 {
                return if l1 - l0 < 1e-3 {
                    v0
                } else {
                    v0 + (level - l0) / (l1 - l0) * (v1 - v0)
                };
            }
        }
        1.0
    };

    // Ends: lifted blacks and softened whites, if the reference has them.
    let (have_lo, want_lo) = (percentile(&now, 0.002), percentile(&reference.luma, 0.002));
    let (have_hi, want_hi) = (percentile(&now, 0.998), percentile(&reference.luma, 0.998));
    let black = if want_lo > have_lo + 2.0 {
        (height(want_lo) - height(have_lo)).clamp(0.0, CURVE_END_REACH)
    } else {
        0.0
    };
    let white = if want_hi < have_hi - 2.0 {
        1.0 - (height(have_hi) - height(want_hi)).clamp(0.0, CURVE_END_REACH)
    } else {
        1.0
    };

    // (input, output, quantile) for each fitted point.
    let mut points: Vec<(f32, f32, f32)> = CURVE_QUANTILES
        .iter()
        .map(|&q| {
            (
                height(percentile(&now, q)),
                height(percentile(&reference.luma, q)),
                q,
            )
        })
        .collect();
    let build = |points: &mut Vec<(f32, f32, f32)>| -> ToneCurve {
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut kept: Vec<(f32, f32, f32)> = Vec::new();
        for &p in points.iter() {
            if p.0 <= 0.03 || p.0 >= 0.97 {
                continue;
            }
            if kept.last().is_some_and(|last| p.0 - last.0 < 0.03) {
                continue;
            }
            kept.push(p);
        }
        // Near the straight line, rising, and between the ends.
        let mut floor = black;
        for p in kept.iter_mut() {
            p.1 = p.1.clamp(p.0 - CURVE_REACH, p.0 + CURVE_REACH).clamp(floor, white);
            floor = p.1;
        }
        let mut all = vec![[0.0, black]];
        all.extend(kept.iter().map(|p| [p.0, p.1]));
        all.push([1.0, white]);
        // No segment steeper than CURVE_MAX_SLOPE: cap each point from
        // below (rising from black) and from above (falling from white).
        for _ in 0..2 {
            for i in 1..all.len() - 1 {
                let dx = all[i][0] - all[i - 1][0];
                all[i][1] = all[i][1].min(all[i - 1][1] + CURVE_MAX_SLOPE * dx);
            }
            for i in (1..all.len() - 1).rev() {
                let dx = all[i + 1][0] - all[i][0];
                all[i][1] = all[i][1].max(all[i + 1][1] - CURVE_MAX_SLOPE * dx);
            }
        }
        for (p, fitted) in kept.iter_mut().zip(&all[1..]) {
            p.1 = fitted[1];
        }
        *points = kept;
        ToneCurve::with_points(&all)
    };
    let mut curve = build(&mut points);

    // One correction pass, measured with the curve in place.
    let mut t = *s;
    t.curves.master = curve;
    let after = luma_hist(&render(&t));
    for p in points.iter_mut() {
        p.1 += height(percentile(&reference.luma, p.2)) - height(percentile(&after, p.2));
    }
    curve = build(&mut points);
    (!curve.is_identity()).then_some(curve)
}

/// A stand-in for an ordinary, well exposed, neutral photo: every hue, a
/// typical spread of tones, and plenty of greys. Fitting it to a look gives
/// a preset that brings any such photo towards that look.
fn typical_photo() -> DevelopSource {
    let (w, h) = (180u32, 120u32);
    let img = RgbImage::from_fn(w, h, |x, y| {
        // Tones spread like a typical photo's: most in the middle.
        let u = (y as f32 + 0.5) / h as f32;
        let lightness = (0.45 + 0.13 * (u / (1.0 - u)).ln()).clamp(0.03, 0.97);
        let hue = x as f32 / w as f32 * 360.0;
        let saturation = if x % 4 == 0 { 0.0 } else { 0.45 };
        let (r, g, b) = hsl_to_rgb(hue, saturation, lightness);
        Rgb([r, g, b].map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8))
    });
    DevelopSource::from_image(&DynamicImage::ImageRgb8(img), None)
}

/// The look of `reference` as preset settings, for any photo.
pub fn preset_from_look(reference: &LookStats) -> DevelopSettings {
    match_look(&typical_photo(), &DevelopSettings::default(), reference)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A colourful gradient, scaled and tinted.
    fn photo(brightness: f32, tint: [f32; 3], lift: f32) -> RgbImage {
        RgbImage::from_fn(160, 120, |x, y| {
            let v = (x as f32 / 159.0 * 0.6 + y as f32 / 119.0 * 0.4) * brightness;
            let hue = y as f32 / 119.0 * 360.0;
            let (r, g, b) = hsl_to_rgb(hue, if x % 3 == 0 { 0.0 } else { 0.5 }, v.clamp(0.0, 1.0));
            Rgb([r, g, b]
                .iter()
                .zip(tint)
                .map(|(c, t)| ((lift + c * t * (1.0 - lift)) * 255.0).clamp(0.0, 255.0) as u8)
                .collect::<Vec<_>>()
                .try_into()
                .unwrap())
        })
    }

    fn source(img: &RgbImage) -> DevelopSource {
        DevelopSource::from_image(&DynamicImage::ImageRgb8(img.clone()), None)
    }

    fn rendered(src: &DevelopSource, s: &DevelopSettings) -> LookStats {
        LookStats::of_rgb8(&render_export(
            &prepare_preview(src, ANALYSIS_EDGE),
            s,
            None,
            None,
            &OutputSpace::Srgb,
        ))
    }

    #[test]
    fn matching_moves_brightness_and_cast_towards_the_reference() {
        let target = source(&photo(0.9, [1.0; 3], 0.0));
        let reference = LookStats::of_rgb8(&photo(0.6, [1.0, 0.92, 0.78], 0.0));
        let matched = match_look(&target, &DevelopSettings::default(), &reference);
        assert!(matched.exposure < 0.0, "exposure {}", matched.exposure);
        assert!(matched.temperature > 0.0, "temperature {}", matched.temperature);

        let before = rendered(&target, &DevelopSettings::default());
        let after = rendered(&target, &matched);
        let median = |s: &LookStats| percentile(&s.luma, 0.5);
        let want = median(&reference);
        assert!((median(&after) - want).abs() < (median(&before) - want).abs());
        let cast = |s: &LookStats| s.tint.unwrap();
        let dist = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
        assert!(dist(cast(&after), cast(&reference)) < dist(cast(&before), cast(&reference)));
    }

    #[test]
    fn matching_a_photo_to_itself_changes_little() {
        let img = photo(0.85, [1.0; 3], 0.0);
        let matched = match_look(&source(&img), &DevelopSettings::default(), &LookStats::of_rgb8(&img));
        assert!(matched.exposure.abs() <= 6.0, "{}", matched.exposure);
        assert!(matched.temperature.abs() <= 6.0, "{}", matched.temperature);
        assert!(matched.saturation.abs() <= 10.0, "{}", matched.saturation);
    }

    #[test]
    fn a_black_and_white_reference_gives_black_and_white() {
        let grey = DynamicImage::ImageRgb8(photo(0.8, [1.0; 3], 0.0)).grayscale().to_rgb8();
        let reference = LookStats::of_rgb8(&grey);
        assert!(reference.mono);
        let preset = preset_from_look(&reference);
        assert_eq!(preset.saturation, -100.0);
    }

    #[test]
    fn faded_blacks_become_a_lifted_curve() {
        let faded = LookStats::of_rgb8(&photo(0.9, [1.0; 3], 0.12));
        let preset = preset_from_look(&faded);
        let first = preset.curves.master.points()[0];
        assert!(first[1] > 0.02, "curve {:?}", preset.curves.master.points());
    }
}
