//! One-click starting points: Auto Tone and the white-balance eyedropper.
//!
//! Both work by rendering a small copy of the photo through the real
//! pipeline and adjusting sliders until the result measures right, rather
//! than by modelling the pipeline's maths. That keeps them correct for RAW
//! files (where exposure and white balance are applied as channel gains
//! before development) and ordinary images alike, and whatever the display
//! profile.

use rustyedit_core::{DevelopSection, DevelopSettings};

use crate::{prepare_preview, render_preview, ClipWarnings, DevelopSource};

/// Long edge of the copy Auto Tone measures.
const TONE_EDGE: u32 = 512;
/// Long edge of the copy the eyedropper measures; larger, so a small grey
/// patch is still several pixels across.
const WB_EDGE: u32 = 1024;

/// Where Auto Tone puts the middle of the picture (0-255), before taking
/// the photo's own key into account.
const MID_GREY: f32 = 118.0;
/// How far Auto Tone moves a dark photo's midtones towards [`MID_GREY`],
/// and how far a bright one's: bright scenes (snow, a white page) are
/// usually meant to be bright, so they're only toned down a little.
const LIFT_CORRECTION: f32 = 0.65;
const DARKEN_CORRECTION: f32 = 0.3;
/// Where the brightest and darkest tones should land (0-255).
const WHITE_TARGET: f32 = 247.0;
const BLACK_TARGET: f32 = 8.0;
/// Brightening stops before this share of the photo blows out.
const BRIGHTEN_LIMIT: (f32, f32) = (0.99, 250.0);
/// More than this share at pure white (or black) is a deliberate white
/// background or black surround (a document, a screenshot, a studio
/// backdrop), not something to recover.
const SOLID_SHARE: f32 = 0.2;
/// The furthest Whites and Blacks are moved.
const RANGE_LIMIT: f32 = 35.0;

/// `settings` with Exposure, Contrast, Highlights, Shadows, Whites and
/// Blacks chosen for `source`; everything else is left as it was.
pub fn auto_tone(source: &DevelopSource, settings: &DevelopSettings) -> DevelopSettings {
    let small = prepare_preview(source, TONE_EDGE);
    let mut s = *settings;
    s.exposure = 0.0;
    s.contrast = 0.0;
    s.highlights = 0.0;
    s.shadows = 0.0;
    s.whites = 0.0;
    s.blacks = 0.0;
    let measure = |s: &DevelopSettings| -> [u32; 256] {
        render_preview(&small, s, ClipWarnings::default()).histogram.luma
    };
    let with_exposure = |e: f32| {
        let mut t = s;
        t.exposure = e;
        measure(&t)
    };

    let start = measure(&s);
    if start.iter().all(|&n| n == 0) {
        return *settings;
    }
    let solid_white = share_from(&start, 250) > SOLID_SHARE;
    let solid_black = share_below(&start, 6) > SOLID_SHARE;

    // 1. Exposure: move the median towards mid grey, less so for bright
    // photos, and never darken a white background.
    let median = percentile(&start, 0.5);
    let correction = if median > MID_GREY {
        DARKEN_CORRECTION
    } else {
        LIFT_CORRECTION
    };
    let mut target = MID_GREY + (1.0 - correction) * (median - MID_GREY);
    if solid_white {
        target = target.max(median);
    }
    // Exposure is ±2 stops over ±100; with a display gamma near 2.2 a
    // stop moves an 8-bit level by about 2^(1/2.2).
    let guess = 50.0 * 2.2 * (target / median.max(1.0)).log2();
    let mut exposure = solve_increasing(
        |e| percentile(&with_exposure(e), 0.5) - target,
        (-50.0, 75.0),
        (0.0, median - target),
        guess,
        2.0,
        5,
    );
    // Brightening mustn't blow out highlights that weren't: Shadows lifts
    // the rest instead.
    let (q, limit) = BRIGHTEN_LIMIT;
    if exposure > 0.0 && percentile(&start, q) < limit {
        let top = percentile(&with_exposure(exposure), q);
        if top > limit {
            exposure = solve_increasing(
                |e| percentile(&with_exposure(e), q) - limit,
                (0.0, exposure),
                (0.0, percentile(&start, q) - limit),
                exposure / 2.0,
                2.0,
                4,
            );
        }
    }
    s.exposure = exposure.round();

    // 2. Highlights, Shadows and Contrast from the exposed picture.
    let exposed = measure(&s);
    let blown = share_from(&exposed, 250);
    if !solid_white && blown > 0.002 {
        s.highlights = -(blown * 500.0).clamp(10.0, 60.0).round();
    }
    let deep = share_below(&exposed, 30);
    if !solid_black && deep > 0.04 {
        s.shadows = (deep * 120.0).clamp(5.0, 45.0).round();
    }
    let spread = std_dev(&exposed);
    s.contrast = ((52.0 - spread) / 52.0 * 70.0).clamp(-20.0, 35.0).round();
    // Text on a white page is wide-spread by nature, not too contrasty.
    if solid_white || solid_black {
        s.contrast = s.contrast.max(0.0);
    }

    // 3. Whites and Blacks: stretch the range out to the targets, but
    // never squeeze a range that already reaches them (that's Highlights'
    // and Shadows' job).
    let shaped = measure(&s);
    let top = percentile(&shaped, 0.997);
    if !solid_white && top < WHITE_TARGET {
        s.whites = solve_for_level(&s, |t, v| t.whites = v, 0.997, WHITE_TARGET, top, &measure);
    }
    let shaped = measure(&s);
    let bottom = percentile(&shaped, 0.003);
    if !solid_black && bottom > BLACK_TARGET {
        s.blacks = solve_for_level(&s, |t, v| t.blacks = v, 0.003, BLACK_TARGET, bottom, &measure);
    }
    s
}

/// The value (within ±[`RANGE_LIMIT`]) of the slider `set` changes that puts the `q`
/// percentile at `target`, found by measuring which way the slider moves
/// it first. `now` is the percentile at 0.
fn solve_for_level(
    s: &DevelopSettings,
    set: impl Fn(&mut DevelopSettings, f32),
    q: f32,
    target: f32,
    now: f32,
    measure: &impl Fn(&DevelopSettings) -> [u32; 256],
) -> f32 {
    const PROBE: f32 = 20.0;
    let at = |v: f32| {
        let mut t = *s;
        set(&mut t, v);
        percentile(&measure(&t), q)
    };
    let probe = at(PROBE);
    let slope = (probe - now) / PROBE;
    if slope.abs() < 0.05 {
        return 0.0;
    }
    // Make the function increasing whichever way the slider works.
    let sign = slope.signum();
    let x = solve_increasing(
        |v| sign * (at(sign * v) - target),
        (-RANGE_LIMIT, RANGE_LIMIT),
        (0.0, sign * (now - target)),
        sign * (target - now) / slope,
        1.5,
        4,
    );
    (sign * x).round()
}

/// Temperature and tint (slider units) that make the area around `at`
/// (fractions across and down the rendered frame) neutral grey.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NeutralBalance {
    pub temperature: f32,
    pub tint: f32,
}

/// Why the eyedropper couldn't balance a spot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BalanceError {
    /// Too dark to tell its colour reliably.
    TooDark,
    /// A channel is blown out, so its true colour is unknown.
    Clipped,
    /// Off the picture.
    Outside,
    /// Too strongly coloured to be made neutral within the sliders' range.
    NotNeutral,
}

impl BalanceError {
    pub fn message(&self) -> &'static str {
        match self {
            BalanceError::TooDark => "That spot is too dark to judge. Pick a mid grey or white.",
            BalanceError::Clipped => "That spot is blown out. Pick a grey or white with detail.",
            BalanceError::Outside => "Click on the photo to set white balance.",
            BalanceError::NotNeutral => {
                "That spot is too colourful to be neutral. Pick something grey or white."
            }
        }
    }
}

/// The white balance that makes the spot at `at` neutral, measured on the
/// rendered photo with the colour sections that come after white balance
/// (curves, Color Mixer, Color Grading, effects) left out, as they'd skew
/// what "neutral" means.
pub fn neutral_white_balance(
    source: &DevelopSource,
    settings: &DevelopSettings,
    at: (f32, f32),
) -> Result<NeutralBalance, BalanceError> {
    if !(0.0..=1.0).contains(&at.0) || !(0.0..=1.0).contains(&at.1) {
        return Err(BalanceError::Outside);
    }
    let small = prepare_preview(source, WB_EDGE);
    let mut base = *settings;
    for section in [
        DevelopSection::ToneCurve,
        DevelopSection::ColorMixer,
        DevelopSection::ColorGrading,
        DevelopSection::Effects,
        DevelopSection::Gradients,
    ] {
        section.reset(&mut base);
    }
    base.saturation = 0.0;
    base.vibrance = 0.0;

    let sample = |temperature: f32, tint: f32| -> [f32; 3] {
        let mut s = base;
        s.temperature = temperature;
        s.tint = tint;
        let frame = render_preview(&small, &s, ClipWarnings::default());
        sample_bgra(&frame.bgra, at, 2)
    };
    // Red against blue, and green against their average: both zero when
    // the spot is neutral.
    let error = |[r, g, b]: [f32; 3]| [r - b, g - (r + b) / 2.0];

    let mut x = [settings.temperature, settings.tint];
    let first = sample(x[0], x[1]);
    let luma = 0.2126 * first[0] + 0.7152 * first[1] + 0.0722 * first[2];
    if first.iter().any(|&c| c >= 252.0) {
        return Err(BalanceError::Clipped);
    }
    if luma < 18.0 {
        return Err(BalanceError::TooDark);
    }
    let mut e = error(first);

    // Newton's method with the Jacobian measured once and then refined
    // from each step (Broyden), so each step costs one render.
    const DELTA: f32 = 10.0;
    let step_from = |x: [f32; 2], i: usize| {
        let mut probe = x;
        probe[i] += if probe[i] + DELTA > 100.0 { -DELTA } else { DELTA };
        probe
    };
    let mut jacobian = [[0.0f32; 2]; 2];
    for i in 0..2 {
        let probe = step_from(x, i);
        let pe = error(sample(probe[0], probe[1]));
        let d = probe[i] - x[i];
        jacobian[0][i] = (pe[0] - e[0]) / d;
        jacobian[1][i] = (pe[1] - e[1]) / d;
    }

    let mut best = (x, norm(e));
    for _ in 0..5 {
        if norm(e) < 0.75 {
            break;
        }
        let Some(dx) = solve2(jacobian, [-e[0], -e[1]]) else {
            break;
        };
        let next = [
            (x[0] + dx[0]).clamp(-100.0, 100.0),
            (x[1] + dx[1]).clamp(-100.0, 100.0),
        ];
        let step = [next[0] - x[0], next[1] - x[1]];
        if step[0].abs() + step[1].abs() < 0.05 {
            break;
        }
        let ne = error(sample(next[0], next[1]));
        // Broyden update: J += (Δe − J·Δx) Δxᵀ / |Δx|².
        let de = [ne[0] - e[0], ne[1] - e[1]];
        let jdx = [
            jacobian[0][0] * step[0] + jacobian[0][1] * step[1],
            jacobian[1][0] * step[0] + jacobian[1][1] * step[1],
        ];
        let len2 = step[0] * step[0] + step[1] * step[1];
        for r in 0..2 {
            for c in 0..2 {
                jacobian[r][c] += (de[r] - jdx[r]) * step[c] / len2;
            }
        }
        x = next;
        e = ne;
        if norm(e) < best.1 {
            best = (x, norm(e));
        }
    }

    // Out of the sliders' reach: it was never meant to be grey.
    if best.1 > 6.0 {
        return Err(BalanceError::NotNeutral);
    }
    let [temperature, tint] = best.0;
    Ok(NeutralBalance {
        temperature: temperature.round(),
        tint: tint.round(),
    })
}

/// Finds where the two-variable function `f` is (near) zero, starting from
/// `x0`, each variable kept within `range`: Newton's method with the
/// Jacobian measured once (steps of `delta`) and then refined from each
/// step (Broyden), so each step costs one evaluation. Returns the best
/// point seen.
pub(crate) fn newton2(
    mut f: impl FnMut([f32; 2]) -> [f32; 2],
    x0: [f32; 2],
    range: (f32, f32),
    delta: f32,
    tolerance: f32,
    steps: usize,
) -> [f32; 2] {
    let mut x = x0.map(|v| v.clamp(range.0, range.1));
    let mut e = f(x);
    let mut best = (x, norm(e));
    if best.1 <= tolerance {
        return x;
    }
    let mut jacobian = [[0.0f32; 2]; 2];
    for i in 0..2 {
        let mut probe = x;
        probe[i] += if probe[i] + delta > range.1 { -delta } else { delta };
        let pe = f(probe);
        let d = probe[i] - x[i];
        jacobian[0][i] = (pe[0] - e[0]) / d;
        jacobian[1][i] = (pe[1] - e[1]) / d;
    }
    for _ in 0..steps {
        let Some(dx) = solve2(jacobian, [-e[0], -e[1]]) else {
            break;
        };
        let next = [
            (x[0] + dx[0]).clamp(range.0, range.1),
            (x[1] + dx[1]).clamp(range.0, range.1),
        ];
        let step = [next[0] - x[0], next[1] - x[1]];
        if step[0].abs() + step[1].abs() < 0.05 {
            break;
        }
        let ne = f(next);
        let de = [ne[0] - e[0], ne[1] - e[1]];
        let jdx = [
            jacobian[0][0] * step[0] + jacobian[0][1] * step[1],
            jacobian[1][0] * step[0] + jacobian[1][1] * step[1],
        ];
        let len2 = step[0] * step[0] + step[1] * step[1];
        for r in 0..2 {
            for c in 0..2 {
                jacobian[r][c] += (de[r] - jdx[r]) * step[c] / len2;
            }
        }
        x = next;
        e = ne;
        if norm(e) < best.1 {
            best = (x, norm(e));
        }
        if best.1 <= tolerance {
            break;
        }
    }
    best.0
}

fn norm(e: [f32; 2]) -> f32 {
    e[0].hypot(e[1])
}

/// Solves the 2x2 system `m · x = v`.
fn solve2(m: [[f32; 2]; 2], v: [f32; 2]) -> Option<[f32; 2]> {
    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    if det.abs() < 1e-6 {
        return None;
    }
    Some([
        (v[0] * m[1][1] - m[0][1] * v[1]) / det,
        (m[0][0] * v[1] - v[0] * m[1][0]) / det,
    ])
}

/// The average RGB (0-255) of the `(2r+1)²` pixels around `at` in a BGRA
/// frame.
fn sample_bgra(bgra: &image::RgbaImage, at: (f32, f32), r: i32) -> [f32; 3] {
    let (w, h) = (bgra.width() as i32, bgra.height() as i32);
    if w == 0 || h == 0 {
        return [0.0; 3];
    }
    let cx = ((at.0 * w as f32) as i32).clamp(0, w - 1);
    let cy = ((at.1 * h as f32) as i32).clamp(0, h - 1);
    let mut sum = [0.0f32; 3];
    let mut n = 0.0;
    for y in (cy - r).max(0)..=(cy + r).min(h - 1) {
        for x in (cx - r).max(0)..=(cx + r).min(w - 1) {
            let [b, g, r, _] = bgra.get_pixel(x as u32, y as u32).0;
            sum[0] += r as f32;
            sum[1] += g as f32;
            sum[2] += b as f32;
            n += 1.0;
        }
    }
    sum.map(|v| v / n)
}

/// Finds where the increasing function `f` crosses zero within `range`,
/// to within `tolerance`, in at most `evals` evaluations. `known` is one
/// point already measured and `guess` where to look first. Uses the secant
/// method, falling back to the bracket's regula falsi when a step would
/// leave it; returns the best point seen when the target is out of reach.
pub(crate) fn solve_increasing(
    mut f: impl FnMut(f32) -> f32,
    range: (f32, f32),
    known: (f32, f32),
    guess: f32,
    tolerance: f32,
    evals: usize,
) -> f32 {
    let (mut lo, mut hi) = range;
    let (mut f_lo, mut f_hi): (Option<f32>, Option<f32>) = (None, None);
    let mut note = |x: f32, fx: f32, lo: &mut f32, hi: &mut f32| {
        if fx < 0.0 {
            *lo = x;
            f_lo = Some(fx);
        } else {
            *hi = x;
            f_hi = Some(fx);
        }
        (f_lo, f_hi)
    };
    note(known.0, known.1, &mut lo, &mut hi);
    if known.1.abs() <= tolerance {
        return known.0;
    }
    let mut best = known;
    let mut prev = known;
    let mut x = guess.clamp(range.0, range.1);
    for _ in 0..evals {
        let fx = f(x);
        if fx.abs() < best.1.abs() {
            best = (x, fx);
        }
        if fx.abs() <= tolerance {
            return x;
        }
        let (fl, fh) = note(x, fx, &mut lo, &mut hi);
        let secant = if (fx - prev.1).abs() > 1e-4 {
            x - fx * (x - prev.0) / (fx - prev.1)
        } else {
            f32::NAN
        };
        prev = (x, fx);
        let secant = secant.clamp(range.0, range.1);
        x = if secant > lo && secant < hi {
            secant
        } else if fh.is_none() && secant >= hi {
            // Everything so far is below the target and the trend points
            // past the end: try the end itself.
            range.1
        } else if fl.is_none() && secant <= lo {
            range.0
        } else {
            match (fl, fh) {
                (Some(a), Some(b)) => lo - a * (hi - lo) / (b - a),
                _ => (lo + hi) / 2.0,
            }
        };
        if hi - lo < 0.5 {
            break;
        }
    }
    best.0
}

/// The level (0-255, interpolated) below which a share `q` of the pixels
/// fall.
pub(crate) fn percentile(bins: &[u32; 256], q: f32) -> f32 {
    let total: u64 = bins.iter().map(|&n| n as u64).sum();
    if total == 0 {
        return 0.0;
    }
    let want = q.clamp(0.0, 1.0) as f64 * total as f64;
    let mut seen = 0.0f64;
    for (level, &n) in bins.iter().enumerate() {
        let next = seen + n as f64;
        if next >= want && n > 0 {
            let within = ((want - seen) / n as f64).clamp(0.0, 1.0);
            return level as f32 + within as f32 - 0.5;
        }
        seen = next;
    }
    255.0
}

/// The share of pixels at `level` or brighter.
fn share_from(bins: &[u32; 256], level: usize) -> f32 {
    let total: u64 = bins.iter().map(|&n| n as u64).sum();
    if total == 0 {
        return 0.0;
    }
    bins[level.min(255)..].iter().map(|&n| n as u64).sum::<u64>() as f32 / total as f32
}

/// The share of pixels darker than `level`.
fn share_below(bins: &[u32; 256], level: usize) -> f32 {
    1.0 - share_from(bins, level)
}

/// Standard deviation of the levels.
fn std_dev(bins: &[u32; 256]) -> f32 {
    let total: f64 = bins.iter().map(|&n| n as f64).sum();
    if total == 0.0 {
        return 0.0;
    }
    let mean: f64 = bins
        .iter()
        .enumerate()
        .map(|(l, &n)| l as f64 * n as f64)
        .sum::<f64>()
        / total;
    let var: f64 = bins
        .iter()
        .enumerate()
        .map(|(l, &n)| (l as f64 - mean).powi(2) * n as f64)
        .sum::<f64>()
        / total;
    var.sqrt() as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb, RgbImage};

    fn flat(levels: &[(usize, u32)]) -> [u32; 256] {
        let mut bins = [0u32; 256];
        for &(level, n) in levels {
            bins[level] = n;
        }
        bins
    }

    #[test]
    fn percentiles_and_shares() {
        let bins = flat(&[(10, 50), (200, 50)]);
        assert!((percentile(&bins, 0.25) - 10.0).abs() < 1.0);
        assert!((percentile(&bins, 0.75) - 200.0).abs() < 1.0);
        assert!((share_from(&bins, 200) - 0.5).abs() < 1e-6);
        assert!((share_below(&bins, 11) - 0.5).abs() < 1e-6);
        assert!((std_dev(&bins) - 95.0).abs() < 0.01);
    }

    #[test]
    fn solver_finds_roots_and_stops_at_the_edge() {
        let x = solve_increasing(|x| 2.0 * x - 30.0, (-50.0, 50.0), (0.0, -30.0), 5.0, 0.1, 6);
        assert!((x - 15.0).abs() < 0.2, "{x}");
        // Out of reach: the best it can do is the top of the range.
        let x = solve_increasing(|x| x - 500.0, (-50.0, 50.0), (0.0, -500.0), 10.0, 0.1, 6);
        assert!((x - 50.0).abs() < 1.0, "{x}");
    }

    #[test]
    fn solve2_inverts() {
        let x = solve2([[2.0, 1.0], [1.0, 3.0]], [5.0, 10.0]).unwrap();
        assert!((x[0] - 1.0).abs() < 1e-5 && (x[1] - 3.0).abs() < 1e-5);
    }

    /// A gradient scaled to `brightness`, so it has real tonal range.
    fn source(brightness: f32, tint: [f32; 3]) -> DevelopSource {
        let img = RgbImage::from_fn(160, 120, |x, y| {
            let v = (x as f32 / 159.0 * 0.7 + y as f32 / 119.0 * 0.3) * brightness;
            Rgb(tint.map(|t| (v * t * 255.0).clamp(0.0, 255.0) as u8))
        });
        DevelopSource::from_image(&DynamicImage::ImageRgb8(img), None)
    }

    #[test]
    fn auto_tone_brightens_dark_and_darkens_bright_photos() {
        let dark = auto_tone(&source(0.25, [1.0; 3]), &DevelopSettings::default());
        assert!(dark.exposure > 10.0, "dark: {}", dark.exposure);
        // Bright, with a little clipping (not a white background).
        let bright = auto_tone(&source(1.15, [1.0; 3]), &DevelopSettings::default());
        assert!(bright.exposure < 0.0, "bright: {}", bright.exposure);
        // Only the tone sliders change.
        let keeps = DevelopSettings {
            saturation: 33.0,
            ..Default::default()
        };
        assert_eq!(auto_tone(&source(0.25, [1.0; 3]), &keeps).saturation, 33.0);
    }

    #[test]
    fn eyedropper_neutralises_a_colour_cast() {
        // A warm, slightly green cast over the whole picture.
        let src = source(0.9, [1.0, 0.93, 0.8]);
        let balance =
            neutral_white_balance(&src, &DevelopSettings::default(), (0.6, 0.5)).unwrap();
        assert!(balance.temperature < -5.0, "{balance:?}");
        let settings = DevelopSettings {
            temperature: balance.temperature,
            tint: balance.tint,
            ..Default::default()
        };
        let frame = render_preview(&prepare_preview(&src, WB_EDGE), &settings, ClipWarnings::default());
        let [r, g, b] = sample_bgra(&frame.bgra, (0.6, 0.5), 2);
        assert!((r - b).abs() < 3.0 && (g - (r + b) / 2.0).abs() < 3.0, "{r} {g} {b}");
    }

    #[test]
    fn auto_tone_keeps_a_white_page_white() {
        // A document: mostly pure white, with darker text-like bands.
        let img = RgbImage::from_fn(160, 120, |_, y| {
            if y % 10 < 7 { Rgb([255; 3]) } else { Rgb([40; 3]) }
        });
        let src = DevelopSource::from_image(&DynamicImage::ImageRgb8(img), None);
        let tuned = auto_tone(&src, &DevelopSettings::default());
        assert!(tuned.exposure >= 0.0, "{}", tuned.exposure);
        assert_eq!(tuned.highlights, 0.0);
        assert_eq!(tuned.whites, 0.0);
    }

    #[test]
    fn eyedropper_refuses_a_strong_colour() {
        let red = source(0.8, [1.0, 0.1, 0.1]);
        assert_eq!(
            neutral_white_balance(&red, &DevelopSettings::default(), (0.7, 0.5)),
            Err(BalanceError::NotNeutral)
        );
    }

    #[test]
    fn eyedropper_refuses_black_and_blown_spots() {
        let dark = source(0.02, [1.0; 3]);
        assert_eq!(
            neutral_white_balance(&dark, &DevelopSettings::default(), (0.5, 0.5)),
            Err(BalanceError::TooDark)
        );
        let blown = source(4.0, [1.0; 3]);
        assert_eq!(
            neutral_white_balance(&blown, &DevelopSettings::default(), (0.9, 0.9)),
            Err(BalanceError::Clipped)
        );
    }
}
