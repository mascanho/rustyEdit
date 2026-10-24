//! Image decoding that understands camera RAW files from every maker the
//! RAW decoder (rawler) supports (Canon, Nikon, Sony, Fujifilm, Olympus /
//! OM System, Panasonic, Leica, Pentax, Samsung, Hasselblad, Phase One,
//! Sigma, DNG, ...) in addition to the formats the `image` crate handles
//! natively.

use std::{path::Path, sync::OnceLock};

use image::{metadata::Orientation, DynamicImage};

/// RAW extensions rawler decodes that aren't photos.
const NOT_PHOTOS: &[&str] = &["crm"];
/// Older Sony formats the ARW decoder reads but rawler doesn't list.
const EXTRA_RAW: &[&str] = &["srf", "sr2"];

/// Every camera RAW extension, lower-case.
pub(crate) fn raw_extensions() -> &'static [&'static str] {
    static EXTENSIONS: OnceLock<Vec<&'static str>> = OnceLock::new();
    EXTENSIONS.get_or_init(|| {
        let mut all: Vec<&'static str> = rawler::decoders::supported_extensions()
            .iter()
            .map(|ext| &*Box::leak(ext.to_ascii_lowercase().into_boxed_str()))
            .filter(|ext| !NOT_PHOTOS.contains(ext))
            .collect();
        all.extend(EXTRA_RAW.iter().filter(|e| !all.contains(e)).collect::<Vec<_>>());
        all
    })
}

/// Whether `path` is a camera RAW file.
pub fn is_raw_file(path: &Path) -> bool {
    is_raw(path)
}

pub(crate) fn is_raw(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| raw_extensions().contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// A RAW file's own metadata, read by rawler without decoding the image:
/// for containers the EXIF reader can't open (Canon CR3, Fujifilm RAF, ...).
pub(crate) fn raw_metadata(path: &Path) -> Option<rawler::decoders::RawMetadata> {
    let source = rawler::rawsource::RawSource::new(path).ok()?;
    let decoder = rawler::get_decoder(&source).ok()?;
    decoder
        .raw_metadata(&source, &rawler::decoders::RawDecodeParams::default())
        .ok()
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

/// EXIF orientation of `path` (1 = upright), or 1 when unknown. RAW files
/// the EXIF reader can't open are asked through rawler.
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
        .or_else(|| {
            is_raw(path)
                .then(|| raw_metadata(path)?.exif.orientation.map(u32::from))
                .flatten()
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

/// The camera's own JPEG preview inside a RAW file, upright, or `None` for
/// other files or when there isn't one. It shows the photo with the
/// camera's colour look and (on most cameras) its lens corrections.
pub fn embedded_preview(path: &Path) -> Option<DynamicImage> {
    if !is_raw(path) {
        return None;
    }
    let params = rawler::decoders::RawDecodeParams::default();
    let preview = rawler::analyze::extract_preview_pixels(path, &params).ok()?;
    let mut image = DynamicImage::ImageRgb8(preview.to_rgb8());
    if let Some(orientation) = Orientation::from_exif(exif_orientation(path)) {
        image.apply_orientation(orientation);
    }
    Some(image)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_major_cameras_raw_files_are_recognised() {
        for name in [
            "a.CR3", "a.cr2", "a.NEF", "a.nrw", "a.arw", "a.RAF", "a.ORF", "a.rw2", "a.rwl",
            "a.pef", "a.srw", "a.dng", "a.3fr", "a.iiq", "a.x3f", "a.srf", "a.sr2",
        ] {
            assert!(is_raw(Path::new(name)), "{name}");
        }
        // Canon's RAW video and ordinary images aren't RAW photos.
        assert!(!is_raw(Path::new("clip.crm")));
        assert!(!is_raw(Path::new("photo.jpg")));
    }
}
