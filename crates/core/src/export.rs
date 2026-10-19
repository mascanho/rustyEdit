//! What the Export module will write: which photos, in what format and
//! size, and where. Pure data; the app runs the export.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportScope {
    /// Only photos flagged as picks.
    Picked,
    /// Every photo in the catalog.
    All,
    /// Just the photo currently selected.
    Selected,
}

impl ExportScope {
    pub const ALL: [ExportScope; 3] = [ExportScope::Picked, ExportScope::All, ExportScope::Selected];

    pub fn label(&self) -> &'static str {
        match self {
            ExportScope::Picked => "Picked",
            ExportScope::All => "All photos",
            ExportScope::Selected => "Selected",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Jpeg,
    Png,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 2] = [ExportFormat::Jpeg, ExportFormat::Png];

    pub fn label(&self) -> &'static str {
        match self {
            ExportFormat::Jpeg => "JPEG",
            ExportFormat::Png => "PNG",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Jpeg => "jpg",
            ExportFormat::Png => "png",
        }
    }
}

/// Longest-edge limit for exported images; `None` keeps the original size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportSize(pub Option<u32>);

impl ExportSize {
    pub const PRESETS: [ExportSize; 4] = [
        ExportSize(None),
        ExportSize(Some(4096)),
        ExportSize(Some(2560)),
        ExportSize(Some(1600)),
    ];

    pub fn label(&self) -> String {
        match self.0 {
            None => "Original".into(),
            Some(px) => format!("{px} px"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOptions {
    pub scope: ExportScope,
    pub format: ExportFormat,
    /// JPEG quality, 1-100.
    pub quality: u8,
    pub size: ExportSize,
    pub destination: Option<PathBuf>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            scope: ExportScope::Picked,
            format: ExportFormat::Jpeg,
            quality: 90,
            size: ExportSize(None),
            destination: None,
        }
    }
}
