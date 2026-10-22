//! Image decoding that understands camera RAW (DNG, Sony ARW, …) in
//! addition to the formats the `image` crate handles natively.

use std::path::Path;

use image::{metadata::Orientation, DynamicImage};

const RAW_EXTENSIONS: &[&str] = &["dng", "arw", "srf", "sr2"];

pub(crate) fn is_raw(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| RAW_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

pub(crate) fn raw_extensions() -> &'static [&'static str] {
    RAW_EXTENSIONS
}

/// Opens any supported image. RAW files are developed with a default
/// pipeline into sRGB; if that fails (unsupported compression, say) the
/// camera's embedded full-size preview is used instead. Does real decode
/// work; call off the UI thread.
pub fn open_image(path: &Path) -> anyhow::Result<DynamicImage> {
    let mut image = decode_unoriented(path)?;
    // Cameras store portrait shots sideways plus an EXIF "rotate me" flag.
    if let Some(orientation) = Orientation::from_exif(exif_orientation(path)) {
        image.apply_orientation(orientation);
    }
    Ok(image)
}

/// Opens a photo for editing in Develop. RAW files keep their sensor data
/// (demosaiced and developed on the GPU, so white balance and exposure act
/// on the camera's own values); other images are converted from their
/// embedded colour profile into the editing colour space. Falls back to
/// [`open_image`] for RAW layouts the GPU path doesn't handle. Does real
/// decode work; call off the UI thread.
pub fn open_for_develop(path: &Path) -> anyhow::Result<rustyedit_processing::DevelopSource> {
    use rustyedit_processing::DevelopSource;
    if is_raw(path) {
        if let Ok(raw) = rawler::decode_file(path) {
            if let Some(source) = DevelopSource::from_raw(raw, exif_orientation(path)) {
                return Ok(source);
            }
        }
        return Ok(DevelopSource::from_image(&open_image(path)?, None));
    }
    let (mut image, icc) = decode_with_profile(path)?;
    if let Some(orientation) = Orientation::from_exif(exif_orientation(path)) {
        image.apply_orientation(orientation);
    }
    Ok(DevelopSource::from_image(&image, icc.as_deref()))
}

/// Decodes a non-RAW image along with its embedded ICC profile, if any.
fn decode_with_profile(path: &Path) -> anyhow::Result<(DynamicImage, Option<Vec<u8>>)> {
    use image::ImageDecoder;
    let mut decoder = image::ImageReader::open(path)?
        .with_guessed_format()?
        .into_decoder()?;
    let icc = decoder.icc_profile().ok().flatten();
    Ok((DynamicImage::from_decoder(decoder)?, icc))
}

/// EXIF orientation of `path` (1 = upright), or 1 when unknown.
pub fn exif_orientation(path: &Path) -> u8 {
    std::fs::File::open(path)
        .ok()
        .and_then(|f| {
            exif::Reader::new()
                .read_from_container(&mut std::io::BufReader::new(f))
                .ok()
        })
        .and_then(|exif| {
            exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?
                .value
                .get_uint(0)
        })
        .filter(|o| (1..=8).contains(o))
        .map_or(1, |o| o as u8)
}

fn decode_unoriented(path: &Path) -> anyhow::Result<DynamicImage> {
    if !is_raw(path) {
        return Ok(image::open(path)?);
    }
    let params = rawler::decoders::RawDecodeParams::default();
    let developed = rawler::analyze::raw_to_srgb(path, &params);
    let preview = || rawler::analyze::extract_preview_pixels(path, &params);
    let full = || rawler::analyze::extract_full_pixels(path, &params);
    let img = developed
        .or_else(|_| full())
        .or_else(|_| preview())
        .map_err(|e| anyhow::anyhow!("RAW decode failed for {}: {e}", path.display()))?;
    Ok(DynamicImage::ImageRgb8(img.to_rgb8()))
}

/// Full-size pixel dimensions without developing the image where possible.
pub fn image_dimensions(path: &Path) -> anyhow::Result<(u32, u32)> {
    let (w, h) = if !is_raw(path) {
        image::image_dimensions(path)?
    } else {
        let raw = rawler::decode_file(path)
            .map_err(|e| anyhow::anyhow!("RAW decode failed for {}: {e}", path.display()))?;
        (raw.width as u32, raw.height as u32)
    };
    // Sideways orientations (5-8) swap the displayed width and height.
    Ok(if exif_orientation(path) >= 5 {
        (h, w)
    } else {
        (w, h)
    })
}
