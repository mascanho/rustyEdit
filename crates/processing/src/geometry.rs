use image::DynamicImage;
use rustyedit_core::DevelopSettings;

/// Applies flips and 90-degree-snapped rotation. Arbitrary-angle rotation
/// (straightening) is intentionally left for a later crop/geometry pass, so
/// this stays a cheap, lossless transform for now.
pub fn apply_geometry(source: &DynamicImage, settings: &DevelopSettings) -> DynamicImage {
    let mut image = source.clone();

    if settings.flip_horizontal {
        image = image.fliph();
    }
    if settings.flip_vertical {
        image = image.flipv();
    }

    let snapped = ((settings.rotation_degrees / 90.0).round() as i32).rem_euclid(4);
    image = match snapped {
        1 => image.rotate90(),
        2 => image.rotate180(),
        3 => image.rotate270(),
        _ => image,
    };

    image
}
