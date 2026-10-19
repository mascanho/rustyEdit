use image::Rgb;
use rustyedit_core::DevelopSettings;

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let delta = max - min;

    if delta < f32::EPSILON {
        return (0.0, 0.0, l);
    }

    let s = if l < 0.5 {
        delta / (max + min)
    } else {
        delta / (2.0 - max - min)
    };

    let h = if max == r {
        ((g - b) / delta + 6.0) % 6.0
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };

    (h * 60.0, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s < f32::EPSILON {
        return (l, l, l);
    }

    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h_prime as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    (r1 + m, g1 + m, b1 + m)
}

/// Applies white balance (temperature/tint) and saturation/vibrance to a
/// single `[0, 1]` RGB pixel.
pub fn apply_color(pixel: [f32; 3], settings: &DevelopSettings) -> [f32; 3] {
    let [mut r, mut g, mut b] = pixel;

    // Temperature: push toward orange (warm) or blue (cool).
    let temp = settings.temperature / 100.0;
    r += temp * 0.12;
    b -= temp * 0.12;

    // Tint: push toward green or magenta.
    let tint = settings.tint / 100.0;
    g += tint * 0.12;
    r -= tint * 0.06;
    b -= tint * 0.06;

    r = r.clamp(0.0, 1.0);
    g = g.clamp(0.0, 1.0);
    b = b.clamp(0.0, 1.0);

    let (h, s, l) = rgb_to_hsl(r, g, b);

    let saturation_factor = 1.0 + settings.saturation / 100.0;
    // Vibrance protects already-saturated colors and skin tones by scaling
    // its effect down as saturation increases.
    let vibrance_factor = 1.0 + (settings.vibrance / 100.0) * (1.0 - s);

    let new_s = (s * saturation_factor * vibrance_factor).clamp(0.0, 1.0);
    let (r, g, b) = hsl_to_rgb(h, new_s, l);

    [r, g, b]
}

pub fn apply_color_rgb8(pixel: Rgb<u8>, settings: &DevelopSettings) -> Rgb<u8> {
    let float = [
        pixel[0] as f32 / 255.0,
        pixel[1] as f32 / 255.0,
        pixel[2] as f32 / 255.0,
    ];
    let adjusted = apply_color(float, settings);
    Rgb([
        (adjusted[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (adjusted[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (adjusted[2].clamp(0.0, 1.0) * 255.0).round() as u8,
    ])
}
