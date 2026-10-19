use serde::{Deserialize, Serialize};

/// A Lightroom-style cull flag: an image is either a keeper, a reject, or
/// has not been triaged yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Flag {
    #[default]
    Unflagged,
    Picked,
    Rejected,
}

impl Flag {
    pub fn label(&self) -> &'static str {
        match self {
            Flag::Unflagged => "Unflagged",
            Flag::Picked => "Picked",
            Flag::Rejected => "Rejected",
        }
    }

    pub fn glyph(&self) -> &'static str {
        match self {
            Flag::Unflagged => "",
            Flag::Picked => "\u{2691}",   // ⚑
            Flag::Rejected => "\u{2715}", // ✕
        }
    }
}
