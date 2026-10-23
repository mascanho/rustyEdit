//! Reads photo metadata (EXIF and file facts) into display-ready groups.
//! Everything is already formatted as text, so the UI just lays it out.

use std::{fs::File, io::BufReader, path::Path};

use exif::{Exif, In, Tag, Value};

#[derive(Debug, Clone, Default)]
pub struct Metadata {
    /// The key exposure facts as `(label, value)`, e.g. `("Aperture", "ƒ/1.8")`.
    pub headline: Vec<(&'static str, String)>,
    pub groups: Vec<MetaGroup>,
}

#[derive(Debug, Clone)]
pub struct MetaGroup {
    pub title: &'static str,
    pub rows: Vec<(&'static str, String)>,
}

/// Curated EXIF fields per group, in display order. Adding a row to the
/// overlay is one line here.
const EXIF_GROUPS: &[(&str, &[(&str, Tag)])] = &[
    (
        "Camera",
        &[
            ("Make", Tag::Make),
            ("Model", Tag::Model),
            ("Serial", Tag::BodySerialNumber),
            ("Owner", Tag::CameraOwnerName),
            ("Software", Tag::Software),
        ],
    ),
    (
        "Lens",
        &[
            ("Lens", Tag::LensModel),
            ("Make", Tag::LensMake),
            ("Specification", Tag::LensSpecification),
            ("Serial", Tag::LensSerialNumber),
            ("Max aperture", Tag::MaxApertureValue),
        ],
    ),
    (
        "Exposure",
        &[
            ("Program", Tag::ExposureProgram),
            ("Mode", Tag::ExposureMode),
            ("Metering", Tag::MeteringMode),
            ("Exposure comp.", Tag::ExposureBiasValue),
            ("Shutter value", Tag::ShutterSpeedValue),
            ("Aperture value", Tag::ApertureValue),
            ("Brightness", Tag::BrightnessValue),
            ("ISO type", Tag::SensitivityType),
            ("Gain", Tag::GainControl),
        ],
    ),
    (
        "Optics",
        &[
            ("Focal length (35mm)", Tag::FocalLengthIn35mmFilm),
            ("Subject distance", Tag::SubjectDistance),
            ("Digital zoom", Tag::DigitalZoomRatio),
            ("Flash", Tag::Flash),
            ("Light source", Tag::LightSource),
            ("White balance", Tag::WhiteBalance),
            ("Scene type", Tag::SceneCaptureType),
            ("Sensing method", Tag::SensingMethod),
        ],
    ),
    (
        "Rendering",
        &[
            ("Contrast", Tag::Contrast),
            ("Saturation", Tag::Saturation),
            ("Sharpness", Tag::Sharpness),
            ("Colour space", Tag::ColorSpace),
            ("Orientation", Tag::Orientation),
        ],
    ),
    (
        "Dates",
        &[
            ("Taken", Tag::DateTimeOriginal),
            ("Digitised", Tag::DateTimeDigitized),
            ("Modified", Tag::DateTime),
        ],
    ),
    (
        "Image",
        &[
            ("Width", Tag::PixelXDimension),
            ("Height", Tag::PixelYDimension),
            ("Resolution X", Tag::XResolution),
            ("Resolution Y", Tag::YResolution),
            ("Resolution unit", Tag::ResolutionUnit),
            ("Description", Tag::ImageDescription),
        ],
    ),
    (
        "Rights",
        &[("Artist", Tag::Artist), ("Copyright", Tag::Copyright)],
    ),
];

pub fn read_metadata(path: &Path, dimensions: Option<(u32, u32)>) -> Metadata {
    let mut meta = Metadata::default();

    // File facts first: these exist even when the file has no EXIF.
    let mut file_rows = Vec::new();
    if let Some(name) = path.file_name() {
        file_rows.push(("Name", name.to_string_lossy().into_owned()));
    }
    if let Some(ext) = path.extension() {
        file_rows.push(("Type", ext.to_string_lossy().to_uppercase()));
    }
    if let Some((w, h)) = dimensions.filter(|&(w, h)| w > 0 && h > 0) {
        file_rows.push(("Dimensions", format!("{w} \u{00D7} {h}")));
        file_rows.push(("Megapixels", format!("{:.1} MP", w as f64 * h as f64 / 1e6)));
    }
    if let Ok(md) = std::fs::metadata(path) {
        file_rows.push(("Size", format_bytes(md.len())));
        for (label, time) in [("Created", md.created()), ("Modified", md.modified())] {
            if let Some(text) = time.ok().and_then(file_time_text) {
                file_rows.push((label, text));
            }
        }
    }
    if let Some(folder) = path.parent() {
        file_rows.push(("Folder", folder.display().to_string()));
    }
    meta.groups.push(MetaGroup {
        title: "File",
        rows: file_rows,
    });

    let Some(exif) = File::open(path).ok().and_then(|f| {
        exif::Reader::new()
            .read_from_container(&mut BufReader::new(f))
            .ok()
    }) else {
        return meta;
    };

    // Exposure triangle, formatted by hand: the generic display is terse.
    let aperture = rational(&exif, Tag::FNumber).map(|v| format!("\u{0192}/{}", trim_float(v)));
    let shutter = rational(&exif, Tag::ExposureTime).map(format_shutter);
    let iso = uint(&exif, Tag::PhotographicSensitivity).map(|v| v.to_string());
    let focal = rational(&exif, Tag::FocalLength).map(|v| format!("{} mm", trim_float(v)));
    meta.headline = [
        ("Aperture", aperture),
        ("Shutter", shutter),
        ("ISO", iso),
        ("Focal", focal),
    ]
    .into_iter()
    .filter_map(|(label, value)| value.map(|v| (label, v)))
    .collect();

    for (title, fields) in EXIF_GROUPS {
        let rows: Vec<_> = fields
            .iter()
            .filter_map(|(label, tag)| display(&exif, *tag).map(|v| (*label, v)))
            .collect();
        if !rows.is_empty() {
            meta.groups.push(MetaGroup { title, rows });
        }
    }

    if let Some(gps) = gps_text(&exif) {
        let mut rows = vec![("GPS", gps)];
        for (label, tag) in [
            ("Altitude", Tag::GPSAltitude),
            ("Direction", Tag::GPSImgDirection),
            ("Speed", Tag::GPSSpeed),
            ("Date", Tag::GPSDateStamp),
        ] {
            if let Some(value) = display(&exif, tag) {
                rows.push((label, value));
            }
        }
        meta.groups.push(MetaGroup {
            title: "Location",
            rows,
        });
    }
    meta
}

fn display(exif: &Exif, tag: Tag) -> Option<String> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    let text = field.display_value().with_unit(exif).to_string();
    let text = text.trim().trim_matches('"').trim().to_string();
    (!text.is_empty() && text != "unknown").then_some(text)
}

fn rational(exif: &Exif, tag: Tag) -> Option<f64> {
    match &exif.get_field(tag, In::PRIMARY)?.value {
        Value::Rational(v) => v.first().filter(|r| r.denom != 0).map(|r| r.to_f64()),
        _ => None,
    }
}

fn uint(exif: &Exif, tag: Tag) -> Option<u32> {
    exif.get_field(tag, In::PRIMARY)?.value.get_uint(0)
}

fn trim_float(v: f64) -> String {
    let s = format!("{v:.1}");
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

fn format_shutter(seconds: f64) -> String {
    if seconds <= 0.0 {
        "\u{2013}".into()
    } else if seconds < 0.5 {
        format!("1/{} s", (1.0 / seconds).round() as u32)
    } else {
        format!("{} s", trim_float(seconds))
    }
}

/// "2024-05-17 14:03" for a file time (UTC).
fn file_time_text(time: std::time::SystemTime) -> Option<String> {
    let secs = time.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let rem = secs.rem_euclid(86_400);
    Some(format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60
    ))
}

fn format_bytes(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    if bytes as f64 >= MB {
        format!("{:.1} MB", bytes as f64 / MB)
    } else {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    }
}

fn gps_text(exif: &Exif) -> Option<String> {
    let coordinate = |value_tag: Tag, ref_tag: Tag| -> Option<String> {
        let field = exif.get_field(value_tag, In::PRIMARY)?;
        let Value::Rational(parts) = &field.value else {
            return None;
        };
        let [d, m, s] = parts.get(..3)? else {
            return None;
        };
        let degrees = d.to_f64() + m.to_f64() / 60.0 + s.to_f64() / 3600.0;
        let hemisphere = display(exif, ref_tag).unwrap_or_default();
        Some(format!("{degrees:.5}\u{00B0} {hemisphere}"))
    };
    let lat = coordinate(Tag::GPSLatitude, Tag::GPSLatitudeRef)?;
    let lon = coordinate(Tag::GPSLongitude, Tag::GPSLongitudeRef)?;
    Some(format!("{lat}, {lon}"))
}

/// The EXIF facts the library sorts and filters on, read once at import and
/// kept in the catalog. Falls back to the file's modified time for the date.
pub fn read_photo_meta(path: &Path) -> rustyedit_core::PhotoMeta {
    let mut meta = rustyedit_core::PhotoMeta::default();

    if let Some(exif) = File::open(path).ok().and_then(|f| {
        exif::Reader::new()
            .read_from_container(&mut BufReader::new(f))
            .ok()
    }) {
        let make = display(&exif, Tag::Make).unwrap_or_default();
        let model = display(&exif, Tag::Model).unwrap_or_default();
        meta.camera = if model.to_lowercase().starts_with(&make.to_lowercase()) {
            model
        } else {
            format!("{make} {model}").trim().to_string()
        };
        meta.lens = display(&exif, Tag::LensModel).unwrap_or_default();
        meta.iso = uint(&exif, Tag::PhotographicSensitivity).unwrap_or(0);
        meta.focal_mm = rational(&exif, Tag::FocalLength).unwrap_or(0.0) as f32;
        if let Some(text) = display(&exif, Tag::DateTimeOriginal) {
            meta.captured = parse_exif_time(&text).unwrap_or(0);
        }
    }
    if meta.captured == 0 {
        meta.captured = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
    }
    meta
}

/// Parses "2017-04-05 02:44:20" (or with colons in the date) as UTC seconds.
fn parse_exif_time(text: &str) -> Option<i64> {
    let digits: Vec<i64> = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect();
    let [y, mo, d, h, mi, s] = digits.get(..6)? else {
        return None;
    };
    if !(1..=12).contains(mo) || !(1..=31).contains(d) {
        return None;
    }
    // Days from civil (Howard Hinnant's algorithm).
    let y = if *mo <= 2 { y - 1 } else { *y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if *mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + mi * 60 + s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exif_time() {
        assert_eq!(parse_exif_time("1970-01-02 00:00:01"), Some(86_401));
        assert_eq!(parse_exif_time("2017:04:05 03:24:20"), Some(1_491_362_660));
        assert_eq!(parse_exif_time("garbage"), None);
    }
}
