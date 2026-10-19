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
            ("Lens", Tag::LensModel),
            ("Serial", Tag::BodySerialNumber),
            ("Software", Tag::Software),
        ],
    ),
    (
        "Exposure",
        &[
            ("Program", Tag::ExposureProgram),
            ("Metering", Tag::MeteringMode),
            ("Exposure comp.", Tag::ExposureBiasValue),
            ("Flash", Tag::Flash),
            ("White balance", Tag::WhiteBalance),
            ("Focal length (35mm)", Tag::FocalLengthIn35mmFilm),
        ],
    ),
    (
        "Image",
        &[
            ("Taken", Tag::DateTimeOriginal),
            ("Colour space", Tag::ColorSpace),
            ("Orientation", Tag::Orientation),
            ("Artist", Tag::Artist),
            ("Copyright", Tag::Copyright),
        ],
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
        meta.groups.push(MetaGroup {
            title: "Location",
            rows: vec![("GPS", gps)],
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
