use image::{imageops, RgbImage};
use rustyedit_core::DevelopSettings;

/// Clarity is modeled as a wide-radius, low-amount unsharp mask (local
/// contrast), while sharpening is a narrow-radius, higher-amount one. Both
/// are no-ops when their slider is at zero so untouched images never pay
/// the cost of a convolution pass.
pub fn apply_detail(image: &mut RgbImage, settings: &DevelopSettings) {
    if settings.clarity.abs() > 0.5 {
        let amount = (settings.clarity / 100.0) * 0.6;
        *image = imageops::unsharpen(image, 12.0, (amount * 40.0) as i32);
    }

    if settings.sharpening.abs() > 0.5 {
        let amount = (settings.sharpening / 100.0).max(0.0) * 0.8;
        *image = imageops::unsharpen(image, 1.0, (amount * 40.0) as i32);
    }
}
