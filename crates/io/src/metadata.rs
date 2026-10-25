//! Reads photo metadata (EXIF and file facts) into display-ready groups.
//! Everything is already formatted as text, so the UI just lays it out.

use std::{fs::File, io::BufReader, path::Path};

use exif::{Exif, In, Tag, Value};

#[derive(Debug, Clone, Default)]
pub struct Metadata {
    /// The key exposure facts as `(label, value)`, e.g. `("Aperture", "ƒ/1.8")`.
    pub headline: Vec<(&'static str, String)>,
    /// Smaller facts to show as tags beside them: exposure compensation,
    /// program, metering, white balance, flash.
    pub chips: Vec<(&'static str, String)>,
    /// The camera ("FUJIFILM X-T5"), the lens, and when it was taken
    /// ("5 Apr 2017 \u{00B7} 02:44"), ready to show; empty when unknown.
    pub camera: String,
    pub lens: String,
    pub taken: String,
    pub groups: Vec<MetaGroup>,
}

/// The facts both readers (the EXIF reader and the RAW decoder) can give,
/// before they are put into words.
#[derive(Debug, Default)]
struct Facts {
    camera: String,
    lens: String,
    aperture: Option<f64>,
    shutter: Option<f64>,
    iso: Option<u32>,
    focal: Option<f64>,
    compensation: Option<f64>,
    program: Option<u32>,
    metering: Option<u32>,
    white_balance: Option<u32>,
    flash: Option<u32>,
    /// "2017:04:05 02:44:20".
    taken: Option<String>,
}

impl Facts {
    fn apply_to(self, meta: &mut Metadata) {
        meta.headline = [
            ("Aperture", self.aperture.map(|v| format!("\u{0192}/{}", trim_float(v)))),
            ("Shutter", self.shutter.map(format_shutter)),
            ("ISO", self.iso.map(|v| v.to_string())),
            ("Focal", self.focal.map(|v| format!("{} mm", trim_float(v)))),
        ]
        .into_iter()
        .filter_map(|(label, value)| value.map(|v| (label, v)))
        .collect();
        meta.chips = [
            ("Exposure", self.compensation.map(format_ev)),
            ("Mode", self.program.and_then(program_name).map(str::to_string)),
            ("Metering", self.metering.and_then(metering_name).map(str::to_string)),
            ("White balance", self.white_balance.and_then(white_balance_name).map(str::to_string)),
            ("Flash", self.flash.map(|f| flash_text(f).to_string())),
        ]
        .into_iter()
        .filter_map(|(label, value)| value.map(|v| (label, v)))
        .collect();
        meta.camera = self.camera;
        meta.lens = self.lens;
        meta.taken = self.taken.as_deref().and_then(taken_text).unwrap_or_default();
    }
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
    let mut meta = read_basic_metadata(path, dimensions);
    // A Fujifilm also records its film simulation and other settings.
    if meta.camera.to_lowercase().contains("fuji") {
        if let Some(recipe) = crate::fuji::read_fuji_recipe(path) {
            add_fuji_settings(&mut meta, &recipe);
        }
    }
    meta
}

/// The Fujifilm settings as a tag beside the exposure facts and a group of
/// their own.
fn add_fuji_settings(meta: &mut Metadata, r: &rustyedit_core::FujiRecipe) {
    use rustyedit_core::{MonoFilter, WhiteBalance};
    let mut sim = r.film_simulation.label().to_string();
    if r.film_simulation.is_monochrome() && r.mono_filter != MonoFilter::Off {
        sim.push_str(&format!(" + {}", r.mono_filter.label()));
    }
    meta.chips.insert(0, ("Film", sim.clone()));
    let tone = |v: f32| if v == v.trunc() { format!("{:+}", v as i32) } else { format!("{v:+.1}") };
    let wb = if r.white_balance == WhiteBalance::Kelvin {
        format!("{}K", r.kelvin)
    } else {
        r.white_balance.label().to_string()
    };
    let mut rows = vec![
        ("Film simulation", sim),
        ("Dynamic range", r.dynamic_range.label().to_string()),
        ("Grain", r.grain.label().to_string()),
        ("Colour Chrome", r.color_chrome.label().to_string()),
        ("Chrome FX Blue", r.color_chrome_blue.label().to_string()),
        ("White balance", format!("{wb}, {:+} R / {:+} B", r.wb_red, r.wb_blue)),
        ("Highlight tone", tone(r.highlight)),
        ("Shadow tone", tone(r.shadow)),
    ];
    // A black and white simulation has no colour of its own to describe.
    if r.film_simulation.is_monochrome() {
        rows.retain(|(l, _)| *l != "Colour Chrome" && *l != "Chrome FX Blue");
    }
    meta.groups.insert(
        meta.groups.iter().position(|g| g.title != "File").unwrap_or(meta.groups.len()),
        MetaGroup { title: "Fujifilm", rows },
    );
}

fn read_basic_metadata(path: &Path, dimensions: Option<(u32, u32)>) -> Metadata {
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
        // Containers the EXIF reader can't open (Canon CR3, Fujifilm RAF,
        // ...): the RAW decoder reads the same facts.
        if crate::decode::is_raw(path) {
            if let Some(raw) = crate::decode::raw_metadata(path) {
                raw_facts(&raw, &mut meta);
            }
        }
        return meta;
    };

    // The exposure triangle and the rest of the headline facts.
    let make = display(&exif, Tag::Make).unwrap_or_default();
    let model = display(&exif, Tag::Model).unwrap_or_default();
    let signed = |tag: Tag| match &exif.get_field(tag, In::PRIMARY)?.value {
        Value::SRational(v) => v.first().filter(|r| r.denom != 0).map(|r| r.to_f64()),
        _ => None,
    };
    Facts {
        camera: camera_name(&make, &model),
        lens: display(&exif, Tag::LensModel).unwrap_or_default(),
        aperture: rational(&exif, Tag::FNumber),
        shutter: rational(&exif, Tag::ExposureTime),
        iso: uint(&exif, Tag::PhotographicSensitivity),
        focal: rational(&exif, Tag::FocalLength),
        compensation: signed(Tag::ExposureBiasValue),
        program: uint(&exif, Tag::ExposureProgram),
        metering: uint(&exif, Tag::MeteringMode),
        white_balance: uint(&exif, Tag::WhiteBalance),
        flash: uint(&exif, Tag::Flash),
        taken: display(&exif, Tag::DateTimeOriginal),
    }
    .apply_to(&mut meta);

    for (title, fields) in EXIF_GROUPS {
        let rows: Vec<_> = fields
            .iter()
            .filter_map(|(label, tag)| {
                let text = display(&exif, *tag)?;
                // Some of the reader's wording is long-winded; say it plainly.
                let text = match *tag {
                    Tag::Flash => uint(&exif, Tag::Flash)
                        .map(|f| flash_text(f).to_string())
                        .unwrap_or(text),
                    Tag::SensitivityType => uint(&exif, Tag::SensitivityType)
                        .map(|c| sensitivity_type_name(c).to_string())
                        .unwrap_or(text),
                    Tag::Orientation if text.starts_with("row 0 at top and column 0 at left") => {
                        "Normal".to_string()
                    }
                    Tag::DateTimeOriginal | Tag::DateTimeDigitized | Tag::DateTime => {
                        datetime_text(&text)
                    }
                    _ => text,
                };
                Some((*label, text))
            })
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

/// Make and model as one name, without repeating the make.
fn camera_name(make: &str, model: &str) -> String {
    let (make, model) = (make.trim(), model.trim());
    if model.to_lowercase().starts_with(&make.to_lowercase()) {
        model.to_string()
    } else {
        format!("{make} {model}").trim().to_string()
    }
}

/// What a RAW decoder read (for files the EXIF reader can't open), put
/// into `meta`: the same headline, tags and groups.
fn raw_facts(raw: &rawler::decoders::RawMetadata, meta: &mut Metadata) {
    let e = &raw.exif;
    let ratio = |r: &rawler::formats::tiff::Rational| (r.d != 0).then(|| r.n as f64 / r.d as f64);
    let signed = |r: &rawler::formats::tiff::SRational| (r.d != 0).then(|| r.n as f64 / r.d as f64);
    let lens = raw
        .lens
        .as_ref()
        .map(|lens| format!("{} {}", lens.lens_make, lens.lens_model).trim().to_string())
        .or_else(|| e.lens_model.clone())
        .unwrap_or_default();
    let iso = e
        .iso_speed_ratings
        .map(u32::from)
        .or(e.iso_speed)
        .or(e.recommended_exposure_index);
    let taken = e.date_time_original.clone().or_else(|| e.create_date.clone());
    Facts {
        camera: camera_name(&raw.make, &raw.model),
        lens: lens.clone(),
        aperture: e.fnumber.as_ref().and_then(ratio),
        shutter: e.exposure_time.as_ref().and_then(ratio),
        iso,
        focal: e.focal_length.as_ref().and_then(ratio),
        compensation: e.exposure_bias.as_ref().and_then(signed),
        program: e.exposure_program.map(u32::from),
        metering: e.metering_mode.map(u32::from),
        white_balance: e.white_balance.map(u32::from),
        flash: e.flash.map(u32::from),
        taken: taken.clone(),
    }
    .apply_to(meta);

    let mut camera = vec![("Make", raw.make.trim().to_string()), ("Model", raw.model.trim().to_string())];
    if let Some(v) = &e.serial_number {
        camera.push(("Serial", v.trim().to_string()));
    }
    if let Some(v) = &e.owner_name {
        camera.push(("Owner", v.trim().to_string()));
    }
    let mut lens_rows = Vec::new();
    if !lens.is_empty() {
        lens_rows.push(("Lens", lens));
    }
    if let Some(v) = &e.lens_make {
        lens_rows.push(("Make", v.trim().to_string()));
    }
    if let Some(v) = &e.lens_serial_number {
        lens_rows.push(("Serial", v.trim().to_string()));
    }
    if let Some(v) = e.max_aperture_value.as_ref().and_then(ratio) {
        // The EXIF value is in APEX stops: f-number = 2^(v / 2).
        lens_rows.push(("Max aperture", format!("\u{0192}/{}", trim_float(2f64.powf(v / 2.0)))));
    }
    let mut exposure = Vec::new();
    if let Some(v) = e.exposure_program.and_then(|p| program_name(p.into())) {
        exposure.push(("Program", v.to_string()));
    }
    if let Some(v) = e.metering_mode.and_then(|p| metering_name(p.into())) {
        exposure.push(("Metering", v.to_string()));
    }
    if let Some(v) = e.exposure_bias.as_ref().and_then(signed) {
        exposure.push(("Exposure comp.", format_ev(v)));
    }
    if let Some(v) = e.sensitivity_type {
        exposure.push(("ISO type", sensitivity_type_name(v.into()).to_string()));
    }
    if let Some(v) = e.exposure_mode.and_then(|m| match m {
        0 => Some("Auto"),
        1 => Some("Manual"),
        2 => Some("Auto bracket"),
        _ => None,
    }) {
        exposure.push(("Mode", v.to_string()));
    }
    let mut optics = Vec::new();
    if let Some(v) = e.flash {
        optics.push(("Flash", flash_text(v.into()).to_string()));
    }
    if let Some(v) = e.white_balance.and_then(|w| white_balance_name(w.into())) {
        optics.push(("White balance", v.to_string()));
    }
    if let Some(v) = e.subject_distance.as_ref().and_then(ratio).filter(|d| *d > 0.0 && *d < 1e6) {
        optics.push(("Subject distance", format!("{} m", trim_float(v))));
    }
    if let Some(v) = e.scene_capture_type.and_then(|s| match s {
        0 => Some("Standard"),
        1 => Some("Landscape"),
        2 => Some("Portrait"),
        3 => Some("Night"),
        _ => None,
    }) {
        optics.push(("Scene type", v.to_string()));
    }
    let mut dates = Vec::new();
    for (label, value) in [
        ("Taken", &e.date_time_original),
        ("Created", &e.create_date),
        ("Modified", &e.modify_date),
    ] {
        if let Some(v) = value {
            dates.push((label, datetime_text(v)));
        }
    }
    if let Some(v) = e.image_number {
        // Many cameras count their shots here.
        optics.push(("Image number", v.to_string()));
    }
    let mut rights = Vec::new();
    if let Some(v) = &e.artist {
        rights.push(("Artist", v.trim().to_string()));
    }
    if let Some(v) = &e.copyright {
        rights.push(("Copyright", v.trim().to_string()));
    }
    for (title, rows) in [
        ("Camera", camera),
        ("Lens", lens_rows),
        ("Exposure", exposure),
        ("Optics", optics),
        ("Dates", dates),
        ("Rights", rights),
    ] {
        let rows: Vec<_> = rows.into_iter().filter(|(_, v)| !v.is_empty()).collect();
        if !rows.is_empty() {
            meta.groups.push(MetaGroup { title, rows });
        }
    }
}

fn format_ev(v: f64) -> String {
    if v.abs() < 0.05 {
        "\u{00B1}0 EV".to_string()
    } else {
        format!("{}{} EV", if v > 0.0 { "+" } else { "\u{2212}" }, trim_float(v.abs()))
    }
}

/// EXIF ExposureProgram.
fn program_name(code: u32) -> Option<&'static str> {
    Some(match code {
        1 => "Manual",
        2 => "Program",
        3 => "Aperture priority",
        4 => "Shutter priority",
        5 => "Creative",
        6 => "Action",
        7 => "Portrait",
        8 => "Landscape",
        _ => return None,
    })
}

/// EXIF MeteringMode.
fn metering_name(code: u32) -> Option<&'static str> {
    Some(match code {
        1 => "Average",
        2 => "Centre-weighted",
        3 => "Spot",
        4 => "Multi-spot",
        5 => "Pattern",
        6 => "Partial",
        _ => return None,
    })
}

/// EXIF WhiteBalance (auto or manual).
fn white_balance_name(code: u32) -> Option<&'static str> {
    match code {
        0 => Some("Auto"),
        1 => Some("Manual"),
        _ => None,
    }
}

/// EXIF Flash: bit 0 says whether it fired.
fn flash_text(code: u32) -> &'static str {
    if code & 1 == 1 {
        "Fired"
    } else {
        "Off"
    }
}

fn sensitivity_type_name(code: u32) -> &'static str {
    match code {
        1 => "Standard output",
        2 => "Recommended exposure index",
        3 => "ISO speed",
        4 => "Standard output + recommended index",
        5 => "Standard output + ISO speed",
        6 => "Recommended index + ISO speed",
        7 => "All three",
        _ => "Unknown",
    }
}

/// "2017:04:05 02:44:20" as "2017-04-05 02:44:20", the way file times read.
fn datetime_text(text: &str) -> String {
    let text = text.trim();
    let mut out = String::with_capacity(text.len());
    let mut colons = 0;
    for c in text.chars() {
        if c == ':' && colons < 2 && out.len() < 8 {
            colons += 1;
            out.push('-');
        } else {
            out.push(c);
        }
    }
    out
}

/// "2017:04:05 02:44:20" as "5 Apr 2017 \u{00B7} 02:44".
fn taken_text(text: &str) -> Option<String> {
    let digits: Vec<u32> = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect();
    let [y, mo, d, h, mi] = digits.get(..5)? else {
        return None;
    };
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = MONTHS.get((*mo as usize).checked_sub(1)?)?;
    (*y > 0).then(|| format!("{d} {month} {y} \u{00B7} {h:02}:{mi:02}"))
}

fn display(exif: &Exif, tag: Tag) -> Option<String> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    // Text fields can hold several strings, mostly empty (Fujifilm's lens
    // name does); show the ones with something in them.
    if let Value::Ascii(parts) = &field.value {
        let text = parts
            .iter()
            .map(|p| String::from_utf8_lossy(p).trim_matches(|c: char| c == '\0' || c.is_whitespace()).to_string())
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        return (!text.is_empty() && text != "unknown").then_some(text);
    }
    let text = field.display_value().with_unit(exif).to_string();
    let text = text.trim().trim_matches('"').trim().to_string();
    // Stops from a lens that reports none come out as NaN.
    let junk = text.is_empty() || text == "unknown" || text.contains("NaN") || text.contains("inf");
    (!junk).then_some(text)
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
    // Containers the EXIF reader can't open (Canon CR3, Fujifilm RAF, ...):
    // the RAW decoder reads the same facts.
    if meta.camera.is_empty() && crate::decode::is_raw(path) {
        if let Some(raw) = crate::decode::raw_metadata(path) {
            let (make, model) = (raw.make.trim(), raw.model.trim());
            meta.camera = if model.to_lowercase().starts_with(&make.to_lowercase()) {
                model.to_string()
            } else {
                format!("{make} {model}").trim().to_string()
            };
            let exif = &raw.exif;
            meta.lens = raw
                .lens
                .as_ref()
                .map(|lens| format!("{} {}", lens.lens_make, lens.lens_model).trim().to_string())
                .or_else(|| exif.lens_model.clone())
                .unwrap_or_default();
            meta.iso = exif
                .iso_speed_ratings
                .map(u32::from)
                .or(exif.iso_speed)
                .or(exif.recommended_exposure_index)
                .unwrap_or(0);
            meta.focal_mm = exif
                .focal_length
                .filter(|r| r.d != 0)
                .map_or(0.0, |r| r.n as f32 / r.d as f32);
            if let Some(text) = exif.date_time_original.as_deref().or(exif.create_date.as_deref()) {
                meta.captured = parse_exif_time(text).unwrap_or(0);
            }
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

#[cfg(test)]
mod facts_tests {
    use super::*;

    #[test]
    fn facts_become_a_headline_and_tags() {
        let mut meta = Metadata::default();
        Facts {
            camera: "FUJIFILM X-T5".into(),
            lens: "XF35mmF1.4 R".into(),
            aperture: Some(1.4),
            shutter: Some(1.0 / 250.0),
            iso: Some(640),
            focal: Some(35.0),
            compensation: Some(-0.7),
            program: Some(3),
            metering: Some(5),
            white_balance: Some(0),
            flash: Some(16),
            taken: Some("2026:10:24 15:13:09".into()),
        }
        .apply_to(&mut meta);
        let headline: Vec<_> = meta.headline.iter().map(|(_, v)| v.as_str()).collect();
        assert_eq!(headline, ["\u{0192}/1.4", "1/250 s", "640", "35 mm"]);
        let chip = |label: &str| meta.chips.iter().find(|(l, _)| *l == label).map(|(_, v)| v.as_str());
        assert_eq!(chip("Exposure"), Some("\u{2212}0.7 EV"));
        assert_eq!(chip("Mode"), Some("Aperture priority"));
        assert_eq!(chip("Metering"), Some("Pattern"));
        assert_eq!(chip("White balance"), Some("Auto"));
        assert_eq!(chip("Flash"), Some("Off"));
        assert_eq!(meta.camera, "FUJIFILM X-T5");
        assert_eq!(meta.taken, "24 Oct 2026 \u{00B7} 15:13");
    }

    #[test]
    fn unknown_facts_are_left_out() {
        let mut meta = Metadata::default();
        Facts {
            aperture: Some(8.0),
            program: Some(0),
            metering: Some(255),
            ..Default::default()
        }
        .apply_to(&mut meta);
        assert_eq!(meta.headline.len(), 1);
        assert!(meta.chips.is_empty(), "{:?}", meta.chips);
        assert!(meta.taken.is_empty() && meta.camera.is_empty());
    }

    #[test]
    fn exposure_compensation_reads_naturally() {
        assert_eq!(format_ev(0.0), "\u{00B1}0 EV");
        assert_eq!(format_ev(1.0), "+1 EV");
        assert_eq!(format_ev(-1.3), "\u{2212}1.3 EV");
    }

    #[test]
    fn dates_read_like_file_times() {
        assert_eq!(datetime_text("2026:10:25 14:51:28"), "2026-10-25 14:51:28");
        assert_eq!(datetime_text("2026-10-25 14:51:28"), "2026-10-25 14:51:28");
    }

    #[test]
    fn the_make_is_not_repeated_in_the_camera_name() {
        assert_eq!(camera_name("SONY", "ILCE-7M4"), "SONY ILCE-7M4");
        assert_eq!(camera_name("FUJIFILM", "FUJIFILM X-T5"), "FUJIFILM X-T5");
        assert_eq!(taken_text("garbage"), None);
    }
}
