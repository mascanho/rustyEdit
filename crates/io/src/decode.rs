//! Image decoding that understands camera RAW (DNG, Sony ARW, …) in
//! addition to the formats the `image` crate handles natively.

use std::path::Path;

use image::DynamicImage;

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
    if !is_raw(path) {
        return Ok(image::image_dimensions(path)?);
    }
    let raw = rawler::decode_file(path)
        .map_err(|e| anyhow::anyhow!("RAW decode failed for {}: {e}", path.display()))?;
    Ok((raw.width as u32, raw.height as u32))
}
