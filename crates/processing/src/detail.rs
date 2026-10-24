use image::{imageops, RgbImage};
use rustyedit_core::DevelopSettings;

/// How strongly the Clarity slider pushes (or, below zero, softens) local
/// contrast at its ends.
const CLARITY_STRENGTH: f32 = 0.9;

/// The blur behind Clarity: a wide one, in proportion to the photo, so the
/// look is the same on a preview as on the full-size export.
pub(crate) fn clarity_sigma(width: u32, height: u32) -> f32 {
    (width.max(height) as f32 / 2048.0 * 12.0).max(3.0)
}

/// How far Clarity pushes each pixel away from its blur (negative: towards).
pub(crate) fn clarity_amount(clarity: f32) -> f32 {
    clarity / 100.0 * CLARITY_STRENGTH
}

/// Clarity: local contrast, as the picture pushed away from a wide blur of
/// itself (and, with a negative slider, pulled towards it, which softens).
/// A no-op at zero, so untouched images never pay for the blur. Sharpening
/// is its own stage (`sharpen`): the two used to share an unsharp mask
/// whose only control was its threshold, so neither slider did its job.
pub fn apply_detail(image: &mut RgbImage, settings: &DevelopSettings) {
    if settings.clarity.abs() <= 0.5 || image.width() < 3 || image.height() < 3 {
        return;
    }
    let amount = clarity_amount(settings.clarity);
    let blurred = imageops::blur(image, clarity_sigma(image.width(), image.height()));
    let blurred = blurred.as_raw();
    let raw: &mut [u8] = &mut *image;
    crate::parallel::for_row_bands(raw, 3, |first, band| {
        for (i, px) in band.chunks_exact_mut(3).enumerate() {
            let at = (first + i) * 3;
            for c in 0..3 {
                let v = px[c] as f32;
                let detail = v - blurred[at + c] as f32;
                px[c] = (v + detail * amount).round().clamp(0.0, 255.0) as u8;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    /// A soft edge with fine noise on each side.
    fn scene() -> RgbImage {
        RgbImage::from_fn(120, 60, |x, y| {
            let base = if x < 60 { 90i32 } else { 160 };
            let noise = ((x * 37 + y * 19) % 9) as i32 - 4;
            let v = (base + noise).clamp(0, 255) as u8;
            Rgb([v, v, v])
        })
    }

    fn spread(img: &RgbImage, x0: u32, x1: u32) -> f32 {
        let vals: Vec<f32> = (10..50)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .map(|(x, y)| img.get_pixel(x, y)[0] as f32)
            .collect();
        let mean = vals.iter().sum::<f32>() / vals.len() as f32;
        (vals.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / vals.len() as f32).sqrt()
    }

    fn with_clarity(value: f32) -> RgbImage {
        let mut img = scene();
        let settings = DevelopSettings { clarity: value, ..Default::default() };
        apply_detail(&mut img, &settings);
        img
    }

    #[test]
    fn positive_clarity_adds_local_contrast_and_negative_softens() {
        let none = spread(&with_clarity(0.0), 5, 55);
        let more = spread(&with_clarity(60.0), 5, 55);
        let less = spread(&with_clarity(-60.0), 5, 55);
        assert!(more > none * 1.1, "{none} {more}");
        assert!(less < none * 0.7, "{none} {less}");
    }

    #[test]
    fn more_clarity_is_always_stronger() {
        let spreads: Vec<f32> = [20.0, 50.0, 90.0]
            .iter()
            .map(|&v| spread(&with_clarity(v), 5, 55))
            .collect();
        assert!(spreads[0] < spreads[1] && spreads[1] < spreads[2], "{spreads:?}");
    }

    #[test]
    fn zero_clarity_leaves_the_picture_alone() {
        assert_eq!(with_clarity(0.0), scene());
    }
}
