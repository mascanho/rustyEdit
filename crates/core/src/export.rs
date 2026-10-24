//! What the Export module will write: which photos, in what format and
//! size, and where. Pure data; the app runs the export.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{asset::Asset, flag::Flag};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportScope {
    /// Only photos flagged as picks.
    Picked,
    /// Every photo in the catalog.
    All,
    /// Just the photo(s) currently selected.
    Selected,
    /// Photos rated three stars or more.
    TopRated,
    /// Photos with develop adjustments applied.
    Edited,
}

impl ExportScope {
    pub const ALL: [ExportScope; 5] = [
        ExportScope::Picked,
        ExportScope::Selected,
        ExportScope::TopRated,
        ExportScope::Edited,
        ExportScope::All,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ExportScope::Picked => "Picked",
            ExportScope::All => "All photos",
            ExportScope::Selected => "Selected",
            ExportScope::TopRated => "3\u{2605}+",
            ExportScope::Edited => "Edited",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    Jpeg,
    Png,
    Tiff,
    Bmp,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 4] = [
        ExportFormat::Jpeg,
        ExportFormat::Png,
        ExportFormat::Tiff,
        ExportFormat::Bmp,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ExportFormat::Jpeg => "JPEG",
            ExportFormat::Png => "PNG",
            ExportFormat::Tiff => "TIFF",
            ExportFormat::Bmp => "BMP",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Jpeg => "jpg",
            ExportFormat::Png => "png",
            ExportFormat::Tiff => "tif",
            ExportFormat::Bmp => "bmp",
        }
    }

    /// Whether the quality setting applies.
    pub fn is_lossy(&self) -> bool {
        matches!(self, ExportFormat::Jpeg)
    }
}

/// Longest-edge limit for exported images; `None` keeps the original size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportSize(pub Option<u32>);

impl ExportSize {
    pub const PRESETS: [ExportSize; 7] = [
        ExportSize(None),
        ExportSize(Some(6000)),
        ExportSize(Some(4096)),
        ExportSize(Some(2560)),
        ExportSize(Some(2048)),
        ExportSize(Some(1600)),
        ExportSize(Some(1080)),
    ];

    pub fn label(&self) -> String {
        match self.0 {
            None => "Original".into(),
            Some(px) => format!("{px} px"),
        }
    }

    /// Output dimensions for a source of `width` x `height`; never enlarges.
    pub fn fit(&self, width: u32, height: u32) -> (u32, u32) {
        match self.0 {
            Some(edge) if width.max(height) > edge && width > 0 && height > 0 => {
                let scale = edge as f64 / width.max(height) as f64;
                (
                    ((width as f64 * scale).round() as u32).max(1),
                    ((height as f64 * scale).round() as u32).max(1),
                )
            }
            _ => (width, height),
        }
    }
}

/// How exported files are named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportNaming {
    /// `IMG_0001`
    Original,
    /// `IMG_0001-edit`
    Edited,
    /// `IMG_0001-web`
    Web,
    /// `photo-001`, in export order.
    Sequence,
    /// `2024-05-17_IMG_0001`, from the capture date.
    DatePrefix,
}

impl ExportNaming {
    pub const ALL: [ExportNaming; 5] = [
        ExportNaming::Original,
        ExportNaming::Edited,
        ExportNaming::Web,
        ExportNaming::Sequence,
        ExportNaming::DatePrefix,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ExportNaming::Original => "Original",
            ExportNaming::Edited => "Name-edit",
            ExportNaming::Web => "Name-web",
            ExportNaming::Sequence => "photo-001",
            ExportNaming::DatePrefix => "Date_Name",
        }
    }

    /// File stem for `asset`, the `index`th (0-based) photo of the export.
    pub fn stem(&self, asset: &Asset, index: usize) -> String {
        let mut original = asset
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "photo".into());
        // A virtual copy is a different picture of the same file, so it
        // gets its own name rather than colliding with the original's.
        if let Some(copy) = &asset.copy_name {
            original = format!("{original}-{}", copy.to_lowercase().replace(' ', "-"));
        }
        match self {
            ExportNaming::Original => original,
            ExportNaming::Edited => format!("{original}-edit"),
            ExportNaming::Web => format!("{original}-web"),
            ExportNaming::Sequence => format!("photo-{:03}", index + 1),
            ExportNaming::DatePrefix => match civil_date(asset.meta.captured) {
                Some((y, m, d)) => format!("{y:04}-{m:02}-{d:02}_{original}"),
                None => original,
            },
        }
    }
}

/// What to do when the target file already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportConflict {
    /// Add `-1`, `-2`, ... to the name.
    Rename,
    /// Leave the existing file and move on.
    Skip,
    /// Replace the existing file.
    Overwrite,
}

impl ExportConflict {
    pub const ALL: [ExportConflict; 3] = [
        ExportConflict::Rename,
        ExportConflict::Skip,
        ExportConflict::Overwrite,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ExportConflict::Rename => "Rename",
            ExportConflict::Skip => "Skip",
            ExportConflict::Overwrite => "Overwrite",
        }
    }
}

/// How exported files are grouped into folders inside the destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportSubfolder {
    None,
    /// `2024-05`
    Month,
    /// `2024`
    Year,
    /// `5 stars`
    Rating,
    /// `Picked`, `Rejected`, `Unflagged`
    Flag,
    /// `Red`, `Blue`, ...
    ColorLabel,
}

impl ExportSubfolder {
    pub const ALL: [ExportSubfolder; 6] = [
        ExportSubfolder::None,
        ExportSubfolder::Year,
        ExportSubfolder::Month,
        ExportSubfolder::Rating,
        ExportSubfolder::Flag,
        ExportSubfolder::ColorLabel,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ExportSubfolder::None => "None",
            ExportSubfolder::Month => "Month",
            ExportSubfolder::Year => "Year",
            ExportSubfolder::Rating => "Rating",
            ExportSubfolder::Flag => "Flag",
            ExportSubfolder::ColorLabel => "Label",
        }
    }

    /// The folder `asset` goes into, if any.
    pub fn folder(&self, asset: &Asset) -> Option<String> {
        match self {
            ExportSubfolder::None => None,
            ExportSubfolder::Year => {
                civil_date(asset.meta.captured).map(|(y, _, _)| format!("{y:04}"))
            }
            ExportSubfolder::Month => {
                civil_date(asset.meta.captured).map(|(y, m, _)| format!("{y:04}-{m:02}"))
            }
            ExportSubfolder::Rating => Some(match asset.rating.value() {
                0 => "Unrated".to_string(),
                1 => "1 star".to_string(),
                n => format!("{n} stars"),
            }),
            ExportSubfolder::Flag => Some(
                match asset.flag {
                    Flag::Picked => "Picked",
                    Flag::Rejected => "Rejected",
                    Flag::Unflagged => "Unflagged",
                }
                .to_string(),
            ),
            ExportSubfolder::ColorLabel => Some(asset.color_label.label().to_string()),
        }
    }
}

/// Output sharpening applied after resizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportSharpen {
    Off,
    Low,
    High,
}

impl ExportSharpen {
    pub const ALL: [ExportSharpen; 3] =
        [ExportSharpen::Off, ExportSharpen::Low, ExportSharpen::High];

    pub fn label(&self) -> &'static str {
        match self {
            ExportSharpen::Off => "Off",
            ExportSharpen::Low => "Low",
            ExportSharpen::High => "High",
        }
    }

    /// `(sigma, threshold)` for an unsharp mask; `None` when off.
    pub fn params(&self) -> Option<(f32, i32)> {
        match self {
            ExportSharpen::Off => None,
            ExportSharpen::Low => Some((0.6, 1)),
            ExportSharpen::High => Some((1.0, 2)),
        }
    }
}

/// How many bits per channel PNG and TIFF files are written with. JPEG is
/// always 8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ExportBitDepth {
    #[default]
    Eight,
    /// Smoother gradients and room for wide colour spaces; the files are
    /// twice the size.
    Sixteen,
}

impl ExportBitDepth {
    pub const ALL: [ExportBitDepth; 2] = [ExportBitDepth::Eight, ExportBitDepth::Sixteen];

    pub fn label(&self) -> &'static str {
        match self {
            ExportBitDepth::Eight => "8-bit",
            ExportBitDepth::Sixteen => "16-bit",
        }
    }
}

/// The colour space exported pixels are written in; its ICC profile is
/// embedded in the file so other apps show the colours as intended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ExportColorSpace {
    /// Safe everywhere, and what the web expects.
    #[default]
    Srgb,
    /// Wider gamut, as on Apple displays and phones.
    DisplayP3,
    /// Wider gamut, common in print workflows.
    AdobeRgb,
    /// The widest of the set, for archives and high-end print. Best at 16
    /// bits: 8 bits spread over so wide a gamut can band.
    ProPhoto,
}

impl ExportColorSpace {
    pub const ALL: [ExportColorSpace; 4] = [
        ExportColorSpace::Srgb,
        ExportColorSpace::DisplayP3,
        ExportColorSpace::AdobeRgb,
        ExportColorSpace::ProPhoto,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ExportColorSpace::Srgb => "sRGB",
            ExportColorSpace::DisplayP3 => "Display P3",
            ExportColorSpace::AdobeRgb => "Adobe RGB",
            ExportColorSpace::ProPhoto => "ProPhoto RGB",
        }
    }
}

/// PNG encoder effort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportPngCompression {
    Fast,
    Balanced,
    Smallest,
}

impl ExportPngCompression {
    pub const ALL: [ExportPngCompression; 3] = [
        ExportPngCompression::Fast,
        ExportPngCompression::Balanced,
        ExportPngCompression::Smallest,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ExportPngCompression::Fast => "Fast",
            ExportPngCompression::Balanced => "Balanced",
            ExportPngCompression::Smallest => "Smallest",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportOptions {
    pub scope: ExportScope,
    pub format: ExportFormat,
    /// JPEG quality, 1-100.
    pub quality: u8,
    pub size: ExportSize,
    pub destination: Option<PathBuf>,
    pub naming: ExportNaming,
    pub conflict: ExportConflict,
    pub subfolder: ExportSubfolder,
    pub sharpen: ExportSharpen,
    pub png_compression: ExportPngCompression,
    pub color_space: ExportColorSpace,
    pub bit_depth: ExportBitDepth,
    /// Leave rejected photos out, whatever the scope.
    pub skip_rejected: bool,
    /// Reveal the destination in the file manager when finished.
    pub open_when_done: bool,
    /// Stamp the watermark designed in Settings on each photo (only while
    /// watermarks are switched on there).
    pub watermark: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            scope: ExportScope::Picked,
            format: ExportFormat::Jpeg,
            quality: 90,
            size: ExportSize(None),
            destination: None,
            naming: ExportNaming::Original,
            conflict: ExportConflict::Rename,
            subfolder: ExportSubfolder::None,
            sharpen: ExportSharpen::Off,
            png_compression: ExportPngCompression::Balanced,
            color_space: ExportColorSpace::Srgb,
            bit_depth: ExportBitDepth::Eight,
            skip_rejected: true,
            open_when_done: false,
            watermark: false,
        }
    }
}

/// A saved set of export options ("Web", "Client proofs", ...). Which
/// photos to export and where to put them stay as they are when one is
/// applied, unless the preset has its own destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportPreset {
    pub name: String,
    pub options: ExportOptions,
}

impl ExportPreset {
    /// `current` with this preset's options, keeping its photos and (when
    /// the preset has none) its destination.
    pub fn apply_to(&self, current: &ExportOptions) -> ExportOptions {
        ExportOptions {
            scope: current.scope,
            destination: self
                .options
                .destination
                .clone()
                .or_else(|| current.destination.clone()),
            ..self.options.clone()
        }
    }

    /// Whether `options` are this preset's (ignoring photos and folder).
    pub fn matches(&self, options: &ExportOptions) -> bool {
        self.apply_to(options) == *options
    }
}

/// The export presets that ship with the app.
pub fn builtin_export_presets() -> Vec<ExportPreset> {
    let preset = |name: &str, change: fn(&mut ExportOptions)| {
        let mut options = ExportOptions::default();
        change(&mut options);
        ExportPreset {
            name: name.to_string(),
            options,
        }
    };
    vec![
        preset("Full size JPEG", |o| {
            o.quality = 95;
        }),
        preset("Web (2048 px)", |o| {
            o.quality = 85;
            o.size = ExportSize(Some(2048));
            o.sharpen = ExportSharpen::Low;
        }),
        preset("Instagram (1080 px)", |o| {
            o.quality = 90;
            o.size = ExportSize(Some(1080));
            o.sharpen = ExportSharpen::Low;
        }),
        preset("Client proofs", |o| {
            o.quality = 80;
            o.size = ExportSize(Some(1600));
            o.watermark = true;
        }),
        preset("Archive TIFF", |o| {
            o.format = ExportFormat::Tiff;
            o.color_space = ExportColorSpace::AdobeRgb;
        }),
    ]
}

/// Where a watermark sits on the photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WatermarkPosition {
    #[default]
    BottomRight,
    BottomLeft,
    TopRight,
    TopLeft,
    Centre,
}

impl WatermarkPosition {
    pub const ALL: [WatermarkPosition; 5] = [
        WatermarkPosition::BottomRight,
        WatermarkPosition::BottomLeft,
        WatermarkPosition::TopRight,
        WatermarkPosition::TopLeft,
        WatermarkPosition::Centre,
    ];

    pub fn index(&self) -> usize {
        Self::ALL.iter().position(|p| p == self).unwrap_or(0)
    }

    pub fn from_index(ix: usize) -> Self {
        Self::ALL.get(ix).copied().unwrap_or_default()
    }
}

/// A text watermark, sized relative to the photo so it looks the same at
/// any export size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Watermark {
    pub text: String,
    pub position: WatermarkPosition,
    /// Text height as a percentage of the photo's long edge.
    pub size: u8,
    /// 0-100.
    pub opacity: u8,
    /// White text (otherwise black).
    pub white: bool,
}

impl Default for Watermark {
    fn default() -> Self {
        Self {
            text: "\u{00A9} Your Name".to_string(),
            position: WatermarkPosition::BottomRight,
            size: 3,
            opacity: 70,
            white: true,
        }
    }
}

/// Calendar `(year, month, day)` of a Unix timestamp; `None` when unknown.
fn civil_date(secs: i64) -> Option<(i32, u32, u32)> {
    if secs <= 0 {
        return None;
    }
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    Some((year, month, day))
}
