//! The Film Stocks presets: looks inspired by classic and famous films
//! (Kodak Portra, Kodachrome, Fuji Velvia, Ilford HP5, Polaroid and more).
//! They're emulations built from the app's own sliders, curves, colour
//! mixer and grading, not measured profiles; each preset's notes say what
//! the stock is known for.

use crate::{
    presets::{plot, tint},
    ColorBand, DevelopPreset, DevelopSettings, ToneZone,
};

/// The category these presets are filed under.
pub const FILM_STOCKS: &str = "Film Stocks";

/// Shifts one colour range of the mixer.
fn mix(s: &mut DevelopSettings, band: ColorBand, hue: f32, saturation: f32, luminance: f32) {
    let adjust = &mut s.mixer[band.index()];
    adjust.hue = hue;
    adjust.saturation = saturation;
    adjust.luminance = luminance;
}

fn stock(name: &str, notes: &str, edit: fn(&mut DevelopSettings)) -> DevelopPreset {
    let mut settings = DevelopSettings::default();
    edit(&mut settings);
    DevelopPreset {
        name: name.to_string(),
        settings,
        category: FILM_STOCKS.to_string(),
        favorite: false,
        notes: notes.to_string(),
    }
}

/// Every film stock preset, colour stocks first, then black and white.
pub(crate) fn film_stock_presets() -> Vec<DevelopPreset> {
    vec![
        stock(
            "Kodak Portra 160",
            "Fine-grained, low-contrast portrait film: natural, gently warm skin.",
            |s| {
            s.contrast = -10.0;
            s.highlights = -15.0;
            s.shadows = 10.0;
            s.temperature = 4.0;
            s.vibrance = -5.0;
            s.saturation = -8.0;
            s.grain = 10.0;
            s.grain_size = 20.0;
            tint(s, ToneZone::Shadows, 200.0, 6.0);
            tint(s, ToneZone::Highlights, 40.0, 12.0);
            mix(s, ColorBand::Orange, 4.0, -5.0, 6.0);
            mix(s, ColorBand::Green, 15.0, -25.0, -5.0);
            mix(s, ColorBand::Blue, -5.0, -10.0, 0.0);
            s.curves.master.move_point(0, 0.0, 0.04);
            s.curves.master.move_point(1, 1.0, 0.96);
            },
        ),
        stock(
            "Kodak Portra 400",
            "The wedding favourite: warm peachy skin, soft contrast, muted olive greens.",
            |s| {
            s.contrast = -14.0;
            s.highlights = -20.0;
            s.shadows = 15.0;
            s.temperature = 7.0;
            s.tint = 3.0;
            s.vibrance = 5.0;
            s.saturation = -10.0;
            s.grain = 18.0;
            s.grain_size = 25.0;
            tint(s, ToneZone::Shadows, 190.0, 8.0);
            tint(s, ToneZone::Midtones, 30.0, 6.0);
            tint(s, ToneZone::Highlights, 35.0, 18.0);
            mix(s, ColorBand::Orange, 5.0, -4.0, 8.0);
            mix(s, ColorBand::Red, 5.0, -10.0, 0.0);
            mix(s, ColorBand::Green, 20.0, -30.0, -8.0);
            mix(s, ColorBand::Yellow, -8.0, -10.0, 5.0);
            mix(s, ColorBand::Blue, -8.0, -15.0, -5.0);
            s.curves.master.move_point(0, 0.0, 0.06);
            s.curves.master.move_point(1, 1.0, 0.95);
            },
        ),
        stock(
            "Kodak Portra 800",
            "Warmer and richer than 400, with visible grain.",
            |s| {
            s.contrast = -6.0;
            s.highlights = -15.0;
            s.shadows = 12.0;
            s.temperature = 10.0;
            s.tint = 4.0;
            s.vibrance = 10.0;
            s.grain = 30.0;
            s.grain_size = 35.0;
            tint(s, ToneZone::Shadows, 200.0, 10.0);
            tint(s, ToneZone::Highlights, 30.0, 22.0);
            mix(s, ColorBand::Green, 18.0, -20.0, -6.0);
            mix(s, ColorBand::Orange, 4.0, 0.0, 6.0);
            s.curves.master.move_point(0, 0.0, 0.05);
            s.curves.master.move_point(1, 1.0, 0.95);
            },
        ),
        stock(
            "Kodak Ektar 100",
            "The most saturated colour negative: punchy reds and blues, very fine grain.",
            |s| {
            s.contrast = 22.0;
            s.highlights = -10.0;
            s.blacks = -8.0;
            s.temperature = 3.0;
            s.vibrance = 25.0;
            s.saturation = 15.0;
            s.clarity = 8.0;
            s.grain = 6.0;
            s.grain_size = 15.0;
            tint(s, ToneZone::Highlights, 35.0, 8.0);
            mix(s, ColorBand::Red, 0.0, 20.0, -5.0);
            mix(s, ColorBand::Blue, -5.0, 25.0, -10.0);
            mix(s, ColorBand::Green, -5.0, 10.0, -5.0);
            mix(s, ColorBand::Orange, 0.0, 8.0, 0.0);
            },
        ),
        stock(
            "Kodak Gold 200",
            "Consumer classic: golden highlights, warm and nostalgic.",
            |s| {
            s.contrast = 8.0;
            s.highlights = -10.0;
            s.shadows = 8.0;
            s.temperature = 15.0;
            s.tint = 2.0;
            s.vibrance = 12.0;
            s.saturation = 5.0;
            s.grain = 22.0;
            s.grain_size = 30.0;
            tint(s, ToneZone::Shadows, 30.0, 8.0);
            tint(s, ToneZone::Highlights, 45.0, 25.0);
            mix(s, ColorBand::Yellow, -5.0, 15.0, 5.0);
            mix(s, ColorBand::Green, 15.0, -10.0, 0.0);
            mix(s, ColorBand::Blue, -5.0, -10.0, 0.0);
            s.curves.master.move_point(0, 0.0, 0.04);
            },
        ),
        stock(
            "Kodak UltraMax 400",
            "Warm, punchy everyday film with lively grain.",
            |s| {
            s.contrast = 14.0;
            s.temperature = 10.0;
            s.vibrance = 15.0;
            s.saturation = 10.0;
            s.grain = 28.0;
            s.grain_size = 35.0;
            tint(s, ToneZone::Highlights, 40.0, 18.0);
            mix(s, ColorBand::Red, 0.0, 12.0, 0.0);
            mix(s, ColorBand::Yellow, 0.0, 10.0, 0.0);
            mix(s, ColorBand::Green, 10.0, -5.0, 0.0);
            s.curves.master.move_point(0, 0.0, 0.03);
            },
        ),
        stock(
            "Kodak ColorPlus 200",
            "Budget film look: warm, slightly faded, yellow cast.",
            |s| {
            s.contrast = 4.0;
            s.temperature = 12.0;
            s.tint = 4.0;
            s.saturation = -5.0;
            s.grain = 26.0;
            s.grain_size = 32.0;
            tint(s, ToneZone::Shadows, 20.0, 10.0);
            tint(s, ToneZone::Highlights, 45.0, 22.0);
            mix(s, ColorBand::Green, 20.0, -25.0, 0.0);
            mix(s, ColorBand::Blue, 0.0, -15.0, 5.0);
            s.curves.master.move_point(0, 0.0, 0.07);
            s.curves.master.move_point(1, 1.0, 0.95);
            },
        ),
        stock(
            "Kodak Pro Image 100",
            "Warm, natural tropical-light film.",
            |s| {
            s.contrast = 5.0;
            s.shadows = 5.0;
            s.temperature = 8.0;
            s.vibrance = 8.0;
            s.grain = 10.0;
            s.grain_size = 20.0;
            tint(s, ToneZone::Highlights, 35.0, 12.0);
            mix(s, ColorBand::Orange, 3.0, 0.0, 5.0);
            mix(s, ColorBand::Green, 12.0, -10.0, 0.0);
            s.curves.master.move_point(0, 0.0, 0.03);
            },
        ),
        stock(
            "Kodak Max 800",
            "Fast consumer film: warm, contrasty, grainy.",
            |s| {
            s.contrast = 12.0;
            s.temperature = 12.0;
            s.saturation = 6.0;
            s.grain = 38.0;
            s.grain_size = 40.0;
            tint(s, ToneZone::Highlights, 40.0, 15.0);
            s.curves.master.move_point(0, 0.0, 0.04);
            },
        ),
        stock(
            "Kodak Elite Color 400",
            "Late-90s consumer film: bright, warm, clean.",
            |s| {
            s.contrast = 10.0;
            s.temperature = 6.0;
            s.vibrance = 10.0;
            s.grain = 22.0;
            s.grain_size = 28.0;
            mix(s, ColorBand::Green, 10.0, -8.0, 0.0);
            },
        ),
        stock(
            "Kodak Vericolor III",
            "Studio negative from the 80s: flat, cool, gentle.",
            |s| {
            s.contrast = -15.0;
            s.shadows = 10.0;
            s.temperature = -6.0;
            s.saturation = -10.0;
            s.grain = 14.0;
            s.grain_size = 25.0;
            tint(s, ToneZone::Shadows, 210.0, 10.0);
            s.curves.master.move_point(0, 0.0, 0.05);
            },
        ),
        stock(
            "Kodak Vision3 250D",
            "Daylight cinema negative: wide latitude, flat and filmic.",
            |s| {
            s.contrast = -18.0;
            s.highlights = -25.0;
            s.shadows = 20.0;
            s.temperature = 3.0;
            s.saturation = -12.0;
            s.grain = 12.0;
            s.grain_size = 22.0;
            tint(s, ToneZone::Shadows, 190.0, 14.0);
            tint(s, ToneZone::Highlights, 35.0, 12.0);
            mix(s, ColorBand::Orange, 3.0, -5.0, 5.0);
            mix(s, ColorBand::Blue, -5.0, -15.0, 0.0);
            s.curves.master.move_point(0, 0.0, 0.05);
            s.curves.master.move_point(1, 1.0, 0.94);
            },
        ),
        stock(
            "Kodak Vision3 500T",
            "Tungsten cinema negative: cool teal shadows, warm glowing highlights.",
            |s| {
            s.contrast = -10.0;
            s.highlights = -20.0;
            s.shadows = 15.0;
            s.temperature = -22.0;
            s.tint = 2.0;
            s.saturation = -6.0;
            s.grain = 24.0;
            s.grain_size = 30.0;
            tint(s, ToneZone::Shadows, 200.0, 22.0);
            tint(s, ToneZone::Highlights, 30.0, 18.0);
            mix(s, ColorBand::Orange, -5.0, 5.0, 8.0);
            mix(s, ColorBand::Blue, 0.0, 10.0, -5.0);
            s.curves.master.move_point(0, 0.0, 0.05);
            },
        ),
        stock(
            "Kodak Kodachrome 64",
            "Legendary slide film: rich reds, deep blues, dense shadows.",
            |s| {
            s.contrast = 25.0;
            s.highlights = -10.0;
            s.blacks = -15.0;
            s.temperature = 5.0;
            s.vibrance = 10.0;
            s.saturation = 12.0;
            s.grain = 8.0;
            s.grain_size = 18.0;
            tint(s, ToneZone::Shadows, 220.0, 8.0);
            tint(s, ToneZone::Highlights, 40.0, 10.0);
            mix(s, ColorBand::Red, -5.0, 20.0, -10.0);
            mix(s, ColorBand::Blue, -8.0, 15.0, -15.0);
            mix(s, ColorBand::Yellow, -5.0, 10.0, 0.0);
            mix(s, ColorBand::Green, -10.0, -5.0, -10.0);
            plot(&mut s.curves.master, &[(0.25, 0.2), (0.75, 0.8)]);
            },
        ),
        stock(
            "Kodak Kodachrome 25",
            "The finest Kodachrome: crisp, saturated, slightly cooler.",
            |s| {
            s.contrast = 20.0;
            s.blacks = -10.0;
            s.temperature = 2.0;
            s.vibrance = 8.0;
            s.saturation = 8.0;
            s.grain = 4.0;
            s.grain_size = 12.0;
            mix(s, ColorBand::Red, -5.0, 15.0, -8.0);
            mix(s, ColorBand::Blue, -5.0, 12.0, -10.0);
            plot(&mut s.curves.master, &[(0.25, 0.21), (0.75, 0.79)]);
            },
        ),
        stock(
            "Kodak Kodachrome 200",
            "Fast Kodachrome: warm, contrasty, grainy.",
            |s| {
            s.contrast = 22.0;
            s.temperature = 8.0;
            s.saturation = 6.0;
            s.grain = 22.0;
            s.grain_size = 30.0;
            mix(s, ColorBand::Red, -3.0, 15.0, -5.0);
            s.curves.master.move_point(0, 0.0, 0.02);
            },
        ),
        stock(
            "Kodak Ektachrome E100",
            "Modern slide film: clean, cool-neutral, vivid blues.",
            |s| {
            s.contrast = 18.0;
            s.blacks = -8.0;
            s.temperature = -6.0;
            s.vibrance = 18.0;
            s.saturation = 8.0;
            s.grain = 6.0;
            s.grain_size = 15.0;
            tint(s, ToneZone::Highlights, 200.0, 5.0);
            mix(s, ColorBand::Blue, -5.0, 20.0, -5.0);
            mix(s, ColorBand::Aqua, 0.0, 15.0, 0.0);
            mix(s, ColorBand::Green, -5.0, 8.0, 0.0);
            },
        ),
        stock(
            "Kodak Ektachrome 64 (1970s)",
            "Aged 70s slides: cyan cast, faded blacks.",
            |s| {
            s.contrast = 5.0;
            s.temperature = -14.0;
            s.tint = -4.0;
            s.saturation = -15.0;
            s.grain = 18.0;
            s.grain_size = 26.0;
            tint(s, ToneZone::Shadows, 190.0, 25.0);
            tint(s, ToneZone::Highlights, 180.0, 10.0);
            s.curves.master.move_point(0, 0.0, 0.08);
            s.curves.master.move_point(1, 1.0, 0.93);
            },
        ),
        stock(
            "Kodak Ektachrome 100VS",
            "Very Saturated slide film: bold, punchy colour.",
            |s| {
            s.contrast = 22.0;
            s.temperature = 4.0;
            s.vibrance = 30.0;
            s.saturation = 20.0;
            s.grain = 6.0;
            s.grain_size = 15.0;
            mix(s, ColorBand::Red, 0.0, 20.0, 0.0);
            mix(s, ColorBand::Blue, 0.0, 20.0, -5.0);
            },
        ),
        stock(
            "Kodak Elite Chrome 100",
            "Consumer slide film: vivid and clean.",
            |s| {
            s.contrast = 16.0;
            s.temperature = -2.0;
            s.vibrance = 20.0;
            s.saturation = 10.0;
            s.grain = 8.0;
            s.grain_size = 18.0;
            },
        ),
        stock(
            "Kodak Aerochrome III",
            "Infrared false colour: foliage turns red and magenta.",
            |s| {
            s.contrast = 15.0;
            s.tint = 40.0;
            s.saturation = 10.0;
            s.grain = 20.0;
            s.grain_size = 28.0;
            tint(s, ToneZone::Shadows, 330.0, 15.0);
            tint(s, ToneZone::Highlights, 340.0, 12.0);
            mix(s, ColorBand::Green, -100.0, 50.0, 5.0);
            mix(s, ColorBand::Yellow, -100.0, 30.0, 0.0);
            mix(s, ColorBand::Red, 0.0, 30.0, 0.0);
            },
        ),
        stock(
            "Fujifilm Pro 400H",
            "Airy wedding film: pastel, mint greens, cool lifted shadows.",
            |s| {
            s.contrast = -15.0;
            s.exposure = 5.0;
            s.highlights = -15.0;
            s.shadows = 15.0;
            s.temperature = -6.0;
            s.tint = -5.0;
            s.saturation = -10.0;
            s.grain = 16.0;
            s.grain_size = 24.0;
            tint(s, ToneZone::Shadows, 180.0, 12.0);
            tint(s, ToneZone::Highlights, 170.0, 8.0);
            mix(s, ColorBand::Green, -15.0, -10.0, 8.0);
            mix(s, ColorBand::Aqua, 0.0, 10.0, 5.0);
            mix(s, ColorBand::Orange, -3.0, -5.0, 8.0);
            mix(s, ColorBand::Blue, -5.0, -10.0, 5.0);
            s.curves.master.move_point(0, 0.0, 0.07);
            s.curves.master.move_point(1, 1.0, 0.95);
            },
        ),
        stock(
            "Fujifilm Pro 160NS",
            "Studio portrait negative: neutral, soft, cool skin.",
            |s| {
            s.contrast = -12.0;
            s.temperature = -4.0;
            s.tint = -3.0;
            s.saturation = -8.0;
            s.grain = 10.0;
            s.grain_size = 20.0;
            tint(s, ToneZone::Shadows, 190.0, 8.0);
            mix(s, ColorBand::Green, -8.0, -10.0, 5.0);
            mix(s, ColorBand::Orange, -2.0, -8.0, 6.0);
            s.curves.master.move_point(0, 0.0, 0.05);
            },
        ),
        stock(
            "Fujicolor Superia 400",
            "Everyday Fuji: green-tinted shadows, punchy, grainy.",
            |s| {
            s.contrast = 12.0;
            s.temperature = -4.0;
            s.tint = -8.0;
            s.vibrance = 8.0;
            s.saturation = 6.0;
            s.grain = 28.0;
            s.grain_size = 34.0;
            tint(s, ToneZone::Shadows, 150.0, 18.0);
            tint(s, ToneZone::Highlights, 60.0, 8.0);
            mix(s, ColorBand::Green, -10.0, 10.0, -5.0);
            mix(s, ColorBand::Red, 5.0, 8.0, 0.0);
            },
        ),
        stock(
            "Fujicolor Superia 200",
            "Cooler, greener consumer film.",
            |s| {
            s.contrast = 8.0;
            s.temperature = -3.0;
            s.tint = -6.0;
            s.grain = 20.0;
            s.grain_size = 28.0;
            tint(s, ToneZone::Shadows, 155.0, 14.0);
            mix(s, ColorBand::Green, -8.0, 8.0, 0.0);
            },
        ),
        stock(
            "Fujicolor C200",
            "Budget Fuji: cool, green-leaning, soft contrast.",
            |s| {
            s.contrast = -5.0;
            s.temperature = -8.0;
            s.tint = -8.0;
            s.saturation = -8.0;
            s.grain = 24.0;
            s.grain_size = 30.0;
            tint(s, ToneZone::Shadows, 160.0, 15.0);
            tint(s, ToneZone::Highlights, 180.0, 6.0);
            mix(s, ColorBand::Green, -10.0, 0.0, 5.0);
            mix(s, ColorBand::Blue, 0.0, -5.0, 5.0);
            s.curves.master.move_point(0, 0.0, 0.05);
            },
        ),
        stock(
            "Fujicolor Natura 1600",
            "Low-light cult film: soft, cool-green, heavy grain.",
            |s| {
            s.contrast = -8.0;
            s.shadows = 10.0;
            s.temperature = -5.0;
            s.tint = -6.0;
            s.saturation = -10.0;
            s.grain = 40.0;
            s.grain_size = 42.0;
            tint(s, ToneZone::Shadows, 165.0, 16.0);
            s.curves.master.move_point(0, 0.0, 0.06);
            },
        ),
        stock(
            "Fujicolor 100 Industrial",
            "Japanese drugstore film: neutral-warm with green shadows.",
            |s| {
            s.contrast = 6.0;
            s.temperature = 4.0;
            s.tint = -3.0;
            s.saturation = 4.0;
            s.grain = 12.0;
            s.grain_size = 22.0;
            tint(s, ToneZone::Shadows, 150.0, 8.0);
            },
        ),
        stock(
            "Fujifilm Eterna 500",
            "Cinema negative: low saturation, flat, teal shadows.",
            |s| {
            s.contrast = -22.0;
            s.highlights = -25.0;
            s.shadows = 20.0;
            s.temperature = -3.0;
            s.saturation = -25.0;
            s.grain = 18.0;
            s.grain_size = 25.0;
            tint(s, ToneZone::Shadows, 190.0, 14.0);
            tint(s, ToneZone::Highlights, 45.0, 6.0);
            s.curves.master.move_point(0, 0.0, 0.06);
            s.curves.master.move_point(1, 1.0, 0.93);
            },
        ),
        stock(
            "Fujichrome Velvia 50",
            "Landscape slide legend: intense saturation, deep greens and blues.",
            |s| {
            s.contrast = 28.0;
            s.blacks = -12.0;
            s.tint = 4.0;
            s.vibrance = 30.0;
            s.saturation = 22.0;
            s.clarity = 10.0;
            s.grain = 4.0;
            s.grain_size = 12.0;
            mix(s, ColorBand::Green, -10.0, 20.0, -12.0);
            mix(s, ColorBand::Blue, -5.0, 20.0, -12.0);
            mix(s, ColorBand::Red, 0.0, 15.0, -5.0);
            mix(s, ColorBand::Magenta, 0.0, 15.0, 0.0);
            mix(s, ColorBand::Purple, 0.0, 15.0, 0.0);
            plot(&mut s.curves.master, &[(0.25, 0.19), (0.75, 0.81)]);
            },
        ),
        stock(
            "Fujichrome Velvia 100",
            "Velvia with a little more restraint.",
            |s| {
            s.contrast = 22.0;
            s.tint = 3.0;
            s.vibrance = 25.0;
            s.saturation = 18.0;
            s.grain = 6.0;
            s.grain_size = 14.0;
            mix(s, ColorBand::Green, -8.0, 15.0, -8.0);
            mix(s, ColorBand::Blue, -5.0, 15.0, -8.0);
            },
        ),
        stock(
            "Fujichrome Provia 100F",
            "Neutral, accurate slide film with moderate contrast.",
            |s| {
            s.contrast = 12.0;
            s.temperature = -2.0;
            s.vibrance = 10.0;
            s.saturation = 4.0;
            s.grain = 6.0;
            s.grain_size = 15.0;
            },
        ),
        stock(
            "Fujichrome Astia 100F",
            "Soft-contrast slide film for skin.",
            |s| {
            s.contrast = -6.0;
            s.shadows = 8.0;
            s.vibrance = 4.0;
            s.saturation = -4.0;
            s.grain = 6.0;
            s.grain_size = 15.0;
            mix(s, ColorBand::Orange, 2.0, -5.0, 6.0);
            },
        ),
        stock(
            "Fujichrome Sensia 100",
            "Consumer slide film: warm and lively.",
            |s| {
            s.contrast = 10.0;
            s.temperature = 4.0;
            s.vibrance = 10.0;
            s.grain = 10.0;
            s.grain_size = 18.0;
            },
        ),
        stock(
            "Fujifilm FP-100C",
            "Peel-apart instant: cool, cyan shadows, lifted blacks.",
            |s| {
            s.contrast = 5.0;
            s.temperature = -10.0;
            s.tint = -4.0;
            s.saturation = -5.0;
            s.vignette = -10.0;
            s.grain = 8.0;
            s.grain_size = 22.0;
            tint(s, ToneZone::Shadows, 190.0, 20.0);
            tint(s, ToneZone::Highlights, 60.0, 10.0);
            s.curves.master.move_point(0, 0.0, 0.08);
            },
        ),
        stock(
            "Agfa Vista 200",
            "Warm, red-rich European consumer film.",
            |s| {
            s.contrast = 14.0;
            s.temperature = 8.0;
            s.tint = 4.0;
            s.saturation = 12.0;
            s.grain = 22.0;
            s.grain_size = 30.0;
            tint(s, ToneZone::Highlights, 30.0, 12.0);
            mix(s, ColorBand::Red, 0.0, 20.0, 0.0);
            mix(s, ColorBand::Orange, 0.0, 10.0, 0.0);
            mix(s, ColorBand::Blue, 0.0, -5.0, 0.0);
            },
        ),
        stock(
            "Agfa Ultra 100",
            "Agfa's ultra-saturated negative.",
            |s| {
            s.contrast = 18.0;
            s.temperature = 4.0;
            s.vibrance = 30.0;
            s.saturation = 25.0;
            s.grain = 10.0;
            s.grain_size = 18.0;
            },
        ),
        stock(
            "Agfa Optima 200",
            "Soft, warm professional negative.",
            |s| {
            s.contrast = -4.0;
            s.temperature = 6.0;
            s.saturation = -4.0;
            s.grain = 14.0;
            s.grain_size = 22.0;
            s.curves.master.move_point(0, 0.0, 0.04);
            },
        ),
        stock(
            "Agfa CT Precisa 100",
            "Cool slide film with cyan-blue skies.",
            |s| {
            s.contrast = 14.0;
            s.temperature = -12.0;
            s.tint = -2.0;
            s.vibrance = 12.0;
            s.grain = 8.0;
            s.grain_size = 16.0;
            tint(s, ToneZone::Shadows, 210.0, 14.0);
            mix(s, ColorBand::Blue, -5.0, 20.0, -5.0);
            mix(s, ColorBand::Aqua, 0.0, 20.0, 0.0);
            },
        ),
        stock(
            "Agfacolor (1950s)",
            "Vintage Agfacolor print: faded, magenta-warm, soft.",
            |s| {
            s.contrast = -10.0;
            s.temperature = 10.0;
            s.tint = 10.0;
            s.saturation = -30.0;
            s.vignette = -15.0;
            s.grain = 30.0;
            s.grain_size = 34.0;
            tint(s, ToneZone::Shadows, 330.0, 12.0);
            tint(s, ToneZone::Highlights, 20.0, 20.0);
            s.curves.master.move_point(0, 0.0, 0.1);
            s.curves.master.move_point(1, 1.0, 0.9);
            },
        ),
        stock(
            "CineStill 800T",
            "Tungsten cinema stock for stills: cool, red-glowing highlights.",
            |s| {
            s.contrast = 6.0;
            s.highlights = -10.0;
            s.shadows = 10.0;
            s.temperature = -25.0;
            s.tint = 4.0;
            s.saturation = 4.0;
            s.grain = 32.0;
            s.grain_size = 36.0;
            tint(s, ToneZone::Shadows, 200.0, 20.0);
            tint(s, ToneZone::Highlights, 10.0, 20.0);
            mix(s, ColorBand::Red, 0.0, 15.0, 5.0);
            mix(s, ColorBand::Orange, -5.0, 0.0, 5.0);
            s.curves.master.move_point(0, 0.0, 0.03);
            },
        ),
        stock(
            "CineStill 50D",
            "Daylight cinema stock: clean, saturated, slightly cool.",
            |s| {
            s.contrast = 10.0;
            s.temperature = -5.0;
            s.vibrance = 12.0;
            s.saturation = 6.0;
            s.grain = 8.0;
            s.grain_size = 16.0;
            tint(s, ToneZone::Shadows, 200.0, 8.0);
            },
        ),
        stock(
            "Technicolor 3-Strip",
            "Golden-age Hollywood colour: bold primaries.",
            |s| {
            s.contrast = 18.0;
            s.vibrance = 10.0;
            s.saturation = 25.0;
            s.grain = 10.0;
            s.grain_size = 20.0;
            tint(s, ToneZone::Shadows, 200.0, 10.0);
            tint(s, ToneZone::Highlights, 40.0, 10.0);
            mix(s, ColorBand::Red, 0.0, 25.0, 0.0);
            mix(s, ColorBand::Blue, 0.0, 20.0, -5.0);
            mix(s, ColorBand::Green, -5.0, 15.0, 0.0);
            },
        ),
        stock(
            "Konica Centuria 400",
            "Japanese consumer film: neutral with a magenta lean.",
            |s| {
            s.contrast = 8.0;
            s.temperature = -2.0;
            s.tint = 6.0;
            s.saturation = 5.0;
            s.grain = 26.0;
            s.grain_size = 32.0;
            tint(s, ToneZone::Shadows, 320.0, 8.0);
            },
        ),
        stock(
            "Ferrania Solaris 400",
            "Italian consumer film: warm and gently faded.",
            |s| {
            s.contrast = 6.0;
            s.temperature = 10.0;
            s.saturation = -6.0;
            s.grain = 26.0;
            s.grain_size = 32.0;
            tint(s, ToneZone::Highlights, 35.0, 15.0);
            mix(s, ColorBand::Green, 12.0, -15.0, 0.0);
            s.curves.master.move_point(0, 0.0, 0.05);
            },
        ),
        stock(
            "Lomography Color 800",
            "Lomo colour negative: saturated, warm, grainy, vignetted.",
            |s| {
            s.contrast = 18.0;
            s.temperature = 10.0;
            s.vibrance = 20.0;
            s.saturation = 15.0;
            s.vignette = -15.0;
            s.grain = 36.0;
            s.grain_size = 40.0;
            tint(s, ToneZone::Highlights, 40.0, 15.0);
            },
        ),
        stock(
            "LomoChrome Purple",
            "Experimental film: greens turn violet, skies shift.",
            |s| {
            s.contrast = 10.0;
            s.tint = 35.0;
            s.grain = 25.0;
            s.grain_size = 30.0;
            tint(s, ToneZone::Shadows, 280.0, 30.0);
            tint(s, ToneZone::Midtones, 290.0, 25.0);
            mix(s, ColorBand::Green, 0.0, -50.0, -10.0);
            mix(s, ColorBand::Yellow, -40.0, -10.0, 0.0);
            mix(s, ColorBand::Blue, 30.0, 10.0, 0.0);
            },
        ),
        stock(
            "LomoChrome Metropolis",
            "Muted, gritty urban look: desaturated and contrasty.",
            |s| {
            s.contrast = 25.0;
            s.blacks = -10.0;
            s.temperature = 4.0;
            s.vibrance = -10.0;
            s.saturation = -50.0;
            s.clarity = 15.0;
            s.grain = 22.0;
            s.grain_size = 28.0;
            mix(s, ColorBand::Red, 0.0, 10.0, 0.0);
            },
        ),
        stock(
            "LomoChrome Turquoise",
            "Experimental film: oranges go blue-green, skies go gold.",
            |s| {
            s.temperature = -40.0;
            s.tint = -10.0;
            s.grain = 25.0;
            s.grain_size = 30.0;
            tint(s, ToneZone::Shadows, 180.0, 30.0);
            tint(s, ToneZone::Midtones, 175.0, 20.0);
            tint(s, ToneZone::Highlights, 50.0, 15.0);
            mix(s, ColorBand::Orange, 0.0, -40.0, 0.0);
            mix(s, ColorBand::Blue, -30.0, -20.0, 10.0);
            },
        ),
        stock(
            "Polaroid 600",
            "Integral instant: faded, cool shadows, warm highlights, soft edges.",
            |s| {
            s.contrast = -10.0;
            s.exposure = 5.0;
            s.temperature = 6.0;
            s.saturation = -15.0;
            s.clarity = -10.0;
            s.vignette = -20.0;
            s.grain = 10.0;
            s.grain_size = 30.0;
            tint(s, ToneZone::Shadows, 200.0, 20.0);
            tint(s, ToneZone::Highlights, 45.0, 25.0);
            s.curves.master.move_point(0, 0.0, 0.1);
            s.curves.master.move_point(1, 1.0, 0.9);
            },
        ),
        stock(
            "Polaroid SX-70",
            "Dreamy instant: warm, yellowed, very soft.",
            |s| {
            s.contrast = -15.0;
            s.temperature = 15.0;
            s.tint = 4.0;
            s.saturation = -25.0;
            s.clarity = -15.0;
            s.vignette = -25.0;
            s.grain = 12.0;
            s.grain_size = 30.0;
            tint(s, ToneZone::Shadows, 30.0, 12.0);
            tint(s, ToneZone::Highlights, 45.0, 30.0);
            s.curves.master.move_point(0, 0.0, 0.12);
            s.curves.master.move_point(1, 1.0, 0.9);
            },
        ),
        stock(
            "Kodak Tri-X 400",
            "The photojournalist's B&W: punchy contrast and gritty grain.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 30.0;
            s.whites = 8.0;
            s.blacks = -12.0;
            s.clarity = 15.0;
            s.grain = 38.0;
            s.grain_size = 35.0;
            plot(&mut s.curves.master, &[(0.25, 0.2), (0.75, 0.82)]);
            },
        ),
        stock(
            "Kodak T-Max 100",
            "Modern tabular B&W: smooth tones, very fine grain.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 18.0;
            s.clarity = 8.0;
            s.grain = 8.0;
            s.grain_size = 14.0;
            },
        ),
        stock(
            "Kodak T-Max 3200",
            "Push-film B&W: deep blacks, heavy grain.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 22.0;
            s.blacks = -10.0;
            s.clarity = 10.0;
            s.grain = 60.0;
            s.grain_size = 50.0;
            },
        ),
        stock(
            "Kodak Plus-X 125",
            "Vintage mid-speed B&W: gentle and classic.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 15.0;
            s.grain = 14.0;
            s.grain_size = 20.0;
            s.curves.master.move_point(0, 0.0, 0.03);
            },
        ),
        stock(
            "Kodak Double-X 5222",
            "Cinema B&W negative: strong contrast, dark corners.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 32.0;
            s.highlights = -10.0;
            s.blacks = -15.0;
            s.vignette = -10.0;
            s.grain = 34.0;
            s.grain_size = 34.0;
            },
        ),
        stock(
            "Ilford HP5 Plus 400",
            "Versatile British B&W: mid contrast, honest grain.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 18.0;
            s.shadows = 6.0;
            s.grain = 32.0;
            s.grain_size = 32.0;
            },
        ),
        stock(
            "Ilford FP4 Plus 125",
            "Fine-grained classic B&W with smooth midtones.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 14.0;
            s.grain = 12.0;
            s.grain_size = 18.0;
            },
        ),
        stock(
            "Ilford Delta 3200",
            "Low-light B&W: soft, glowing, very grainy.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 10.0;
            s.clarity = -5.0;
            s.grain = 55.0;
            s.grain_size = 48.0;
            s.curves.master.move_point(0, 0.0, 0.05);
            },
        ),
        stock(
            "Ilford Pan F Plus 50",
            "Slow B&W: crisp contrast, almost no grain.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 30.0;
            s.blacks = -10.0;
            s.clarity = 12.0;
            s.grain = 4.0;
            s.grain_size = 10.0;
            },
        ),
        stock(
            "Ilford XP2 Super",
            "Chromogenic B&W: smooth, soft, wide latitude.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 6.0;
            s.highlights = -10.0;
            s.shadows = 10.0;
            s.grain = 10.0;
            s.grain_size = 20.0;
            },
        ),
        stock(
            "Ilford SFX 200",
            "Near-infrared B&W: dark skies, glowing foliage and skin.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 25.0;
            s.grain = 26.0;
            s.grain_size = 30.0;
            mix(s, ColorBand::Blue, 0.0, 0.0, -50.0);
            mix(s, ColorBand::Aqua, 0.0, 0.0, -30.0);
            mix(s, ColorBand::Green, 0.0, 0.0, 30.0);
            mix(s, ColorBand::Red, 0.0, 0.0, 25.0);
            },
        ),
        stock(
            "Kentmere 400",
            "Affordable everyday B&W.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 16.0;
            s.grain = 30.0;
            s.grain_size = 32.0;
            },
        ),
        stock(
            "Fujifilm Neopan Acros 100",
            "Ultra-sharp B&W: deep blacks, fine grain.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 22.0;
            s.blacks = -12.0;
            s.clarity = 10.0;
            s.grain = 6.0;
            s.grain_size = 12.0;
            },
        ),
        stock(
            "Fujifilm Neopan 1600",
            "Fast Japanese B&W: grainy and contrasty.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 20.0;
            s.grain = 46.0;
            s.grain_size = 42.0;
            },
        ),
        stock(
            "Agfa APX 100",
            "Classic German B&W with bright reds.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 18.0;
            s.grain = 14.0;
            s.grain_size = 20.0;
            mix(s, ColorBand::Red, 0.0, 0.0, 10.0);
            },
        ),
        stock(
            "Rollei Retro 80S",
            "Extended-red B&W: dramatic dark skies.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 32.0;
            s.grain = 12.0;
            s.grain_size = 18.0;
            mix(s, ColorBand::Blue, 0.0, 0.0, -40.0);
            mix(s, ColorBand::Red, 0.0, 0.0, 20.0);
            },
        ),
        stock(
            "Foma Fomapan 100",
            "Czech B&W: old-school tones, slightly soft.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 20.0;
            s.grain = 18.0;
            s.grain_size = 24.0;
            s.curves.master.move_point(0, 0.0, 0.03);
            },
        ),
        stock(
            "Lomography Lady Grey 400",
            "Soft, low-key B&W.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 12.0;
            s.grain = 30.0;
            s.grain_size = 32.0;
            s.curves.master.move_point(0, 0.0, 0.04);
            },
        ),
        stock(
            "Lomography Berlin Kino 400",
            "Cinema B&W: punchy blacks and grain.",
            |s| {
            s.saturation = -100.0;
            s.contrast = 26.0;
            s.blacks = -8.0;
            s.grain = 32.0;
            s.grain_size = 32.0;
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_plenty_and_each_has_notes() {
        let all = film_stock_presets();
        assert!(all.len() >= 60, "{}", all.len());
        assert!(all.iter().all(|p| !p.notes.is_empty()));
    }
}
