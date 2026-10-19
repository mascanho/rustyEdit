use serde::{Deserialize, Serialize};

/// A Lightroom-style colour label, used as a free-form visual tag on top of
/// flags and ratings (e.g. "red = needs retouching").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ColorLabel {
    #[default]
    None,
    Red,
    Yellow,
    Green,
    Blue,
    Purple,
}

impl ColorLabel {
    /// Every real label, in the order they're offered in the UI.
    pub const ALL: [ColorLabel; 5] = [
        ColorLabel::Red,
        ColorLabel::Yellow,
        ColorLabel::Green,
        ColorLabel::Blue,
        ColorLabel::Purple,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ColorLabel::None => "None",
            ColorLabel::Red => "Red",
            ColorLabel::Yellow => "Yellow",
            ColorLabel::Green => "Green",
            ColorLabel::Blue => "Blue",
            ColorLabel::Purple => "Purple",
        }
    }
}
