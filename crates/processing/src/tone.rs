use image::Rgb;
use rustyedit_core::DevelopSettings;

/// Smoothstep-style weight, 1.0 in the shadows and 0.0 in the highlights.
fn shadow_weight(luma: f32) -> f32 {
    (1.0 - luma).clamp(0.0, 1.0).powf(1.5)
}

/// Smoothstep-style weight, 1.0 in the highlights and 0.0 in the shadows.
fn highlight_weight(luma: f32) -> f32 {
    luma.clamp(0.0, 1.0).powf(1.5)
}

fn luma(pixel: [f32; 3]) -> f32 {
    0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2]
}

/// Applies exposure, contrast, highlights/shadows and white/black point
/// adjustments to a single linear-ish `[0, 1]` RGB pixel.
pub fn apply_tone(pixel: [f32; 3], settings: &DevelopSettings) -> [f32; 3] {
    let mut rgb = pixel;

    // Exposure: +/-100 maps to roughly +/-2 stops.
    let exposure_factor = 2f32.powf(settings.exposure / 50.0);
    for c in rgb.iter_mut() {
        *c *= exposure_factor;
    }

    let l = luma(rgb);
    let shadow_amount = settings.shadows / 100.0;
    let highlight_amount = settings.highlights / 100.0;
    let shadow_w = shadow_weight(l);
    let highlight_w = highlight_weight(l);

    for c in rgb.iter_mut() {
        // Shadows: positive lifts dark tones, negative deepens them.
        *c += shadow_amount * shadow_w * 0.5;
        // Highlights: positive brightens bright tones, negative recovers them.
        *c += highlight_amount * highlight_w * 0.35;
    }

    // Contrast: pivot around mid-gray.
    let contrast_factor = 1.0 + settings.contrast / 100.0;
    for c in rgb.iter_mut() {
        *c = (*c - 0.5) * contrast_factor + 0.5;
    }

    // Whites / blacks: rescale the output range.
    let black_point = (settings.blacks / 100.0).clamp(-1.0, 1.0) * 0.25;
    let white_point = 1.0 + (settings.whites / 100.0).clamp(-1.0, 1.0) * 0.25;
    let range = (white_point - black_point).max(0.05);
    for c in rgb.iter_mut() {
        *c = ((*c - black_point) / range).clamp(0.0, 1.0);
    }

    rgb
}

pub fn apply_tone_rgb8(pixel: Rgb<u8>, settings: &DevelopSettings) -> Rgb<u8> {
    let float = [
        pixel[0] as f32 / 255.0,
        pixel[1] as f32 / 255.0,
        pixel[2] as f32 / 255.0,
    ];
    let adjusted = apply_tone(float, settings);
    Rgb([
        (adjusted[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (adjusted[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (adjusted[2].clamp(0.0, 1.0) * 255.0).round() as u8,
    ])
}
