//! User preferences, plus a declarative registry describing how each one is
//! presented in the Settings window.
//!
//! Adding a new option is two steps:
//! 1. add a field (with a default) to one of the settings structs below;
//! 2. add a `SettingItem` describing it to a section in [`SETTINGS_PAGES`].
//!
//! The Settings window renders itself entirely from the registry, so no UI
//! code needs to change. Every struct is `#[serde(default)]`, so settings
//! files written by older versions keep loading after fields are added.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub library: LibrarySettings,
    pub layout: LayoutSettings,
    pub folders: FolderTreeSettings,
    pub filmstrip: FilmstripSettings,
    pub develop: DevelopPreferences,
    pub grid: GridSettings,
    pub appearance: AppearanceSettings,
}

// --- Appearance -------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceSettings {
    pub theme: AppTheme,
    pub accent: AccentColor,
    /// How strongly panels, controls and text stand apart, 0-100.
    /// 50 is the standard look.
    pub contrast: u8,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: AppTheme::Dark,
            accent: AccentColor::Blue,
            contrast: 50,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AppTheme {
    #[default]
    Dark,
    Midnight,
    Graphite,
    Light,
}

impl AppTheme {
    pub const ALL: [AppTheme; 4] = [
        AppTheme::Dark,
        AppTheme::Midnight,
        AppTheme::Graphite,
        AppTheme::Light,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            AppTheme::Dark => "Dark",
            AppTheme::Midnight => "Midnight",
            AppTheme::Graphite => "Graphite",
            AppTheme::Light => "Light",
        }
    }

    pub fn index(&self) -> usize {
        Self::ALL.iter().position(|t| t == self).unwrap_or(0)
    }

    pub fn from_index(ix: usize) -> Self {
        Self::ALL.get(ix).copied().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AccentColor {
    #[default]
    Blue,
    Teal,
    Green,
    Orange,
    Pink,
    Purple,
}

impl AccentColor {
    pub const ALL: [AccentColor; 6] = [
        AccentColor::Blue,
        AccentColor::Teal,
        AccentColor::Green,
        AccentColor::Orange,
        AccentColor::Pink,
        AccentColor::Purple,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            AccentColor::Blue => "Blue",
            AccentColor::Teal => "Teal",
            AccentColor::Green => "Green",
            AccentColor::Orange => "Orange",
            AccentColor::Pink => "Pink",
            AccentColor::Purple => "Purple",
        }
    }

    pub fn index(&self) -> usize {
        Self::ALL.iter().position(|a| a == self).unwrap_or(0)
    }

    pub fn from_index(ix: usize) -> Self {
        Self::ALL.get(ix).copied().unwrap_or_default()
    }

    /// Hue in degrees.
    pub fn hue(&self) -> f32 {
        match self {
            AccentColor::Blue => 205.0,
            AccentColor::Teal => 175.0,
            AccentColor::Green => 142.0,
            AccentColor::Orange => 28.0,
            AccentColor::Pink => 330.0,
            AccentColor::Purple => 265.0,
        }
    }
}

// --- Grid -----------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GridSettings {
    pub style: GridStyle,
    pub aspect: GridAspect,
    /// Fill the thumbnail area (cropping the photo) instead of fitting it.
    pub crop_to_fill: bool,
    pub spacing: GridSpacing,
    /// Pick / reject / rotate buttons over a hovered card.
    pub hover_controls: bool,
    /// Date, camera and size under the file name (Detailed style).
    pub show_details: bool,
    /// Colour bar along the bottom edge of labelled cards.
    pub show_label_bar: bool,
    pub dim_rejected: bool,
    pub tooltips: bool,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            style: GridStyle::Detailed,
            aspect: GridAspect::Landscape,
            crop_to_fill: false,
            spacing: GridSpacing::Normal,
            hover_controls: true,
            show_details: true,
            show_label_bar: true,
            dim_rejected: true,
            tooltips: true,
        }
    }
}

/// How much each grid card shows around its thumbnail, from everything
/// down to the bare photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GridStyle {
    /// Rating strip on top, name and details underneath.
    #[default]
    Detailed,
    /// Just the rating strip on top of the thumbnail.
    Compact,
    /// The thumbnail with small flag / label badges over it.
    Minimal,
    /// Only the photo; the selection outline is the sole chrome.
    Bare,
}

impl GridStyle {
    pub const ALL: [GridStyle; 4] = [
        GridStyle::Detailed,
        GridStyle::Compact,
        GridStyle::Minimal,
        GridStyle::Bare,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            GridStyle::Detailed => "Detailed",
            GridStyle::Compact => "Compact",
            GridStyle::Minimal => "Minimal",
            GridStyle::Bare => "Bare",
        }
    }

    pub fn index(&self) -> usize {
        Self::ALL.iter().position(|s| s == self).unwrap_or(0)
    }

    pub fn from_index(ix: usize) -> Self {
        Self::ALL.get(ix).copied().unwrap_or_default()
    }

    pub fn next(&self) -> Self {
        Self::from_index((self.index() + 1) % Self::ALL.len())
    }
}

/// Card metadata for each style, in `GridStyle::ALL` order.
pub static GRID_STYLE_CARDS: [CardOption; 4] = [
    CardOption {
        title: "Detailed",
        description: "Rating, flag and label on top; name and details below.",
        preview: CardPreview::Grid(GridStyle::Detailed),
    },
    CardOption {
        title: "Compact",
        description: "A slim rating strip over each thumbnail.",
        preview: CardPreview::Grid(GridStyle::Compact),
    },
    CardOption {
        title: "Minimal",
        description: "Thumbnails with small flag and label badges.",
        preview: CardPreview::Grid(GridStyle::Minimal),
    },
    CardOption {
        title: "Bare",
        description: "Just the photos. Details live in the tooltip.",
        preview: CardPreview::Grid(GridStyle::Bare),
    },
];

/// Shape of the thumbnail area in grid cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GridAspect {
    /// 4:3
    #[default]
    Landscape,
    /// 3:2, like most camera sensors.
    Classic,
    Square,
}

impl GridAspect {
    /// Thumbnail height as a fraction of its width.
    pub fn height_ratio(&self) -> f32 {
        match self {
            GridAspect::Landscape => 0.75,
            GridAspect::Classic => 2.0 / 3.0,
            GridAspect::Square => 1.0,
        }
    }
}

/// Gap between grid cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GridSpacing {
    Tight,
    #[default]
    Normal,
    Roomy,
}

impl GridSpacing {
    pub fn gap(&self) -> f32 {
        match self {
            GridSpacing::Tight => 3.0,
            GridSpacing::Normal => 10.0,
            GridSpacing::Roomy => 18.0,
        }
    }
}

/// Bounds and step for the Grid card size, shared by Settings and the
/// grid's own +/- control.
pub const GRID_CARD_MIN: u32 = 90;
pub const GRID_CARD_MAX: u32 = 420;
pub const GRID_CARD_STEP: u32 = 20;
pub const GRID_CARD_DEFAULT: u32 = 180;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LibrarySettings {
    pub startup_view: StartupView,
    /// Width of a grid card in pixels; the column count follows from it.
    pub grid_card_size: u32,
    pub show_left_panel: bool,
    pub show_right_panel: bool,
    /// After flagging or rating a single photo, move on to the next one.
    pub auto_advance: bool,
    /// What the loupe shows the photo against.
    pub loupe_background: LoupeBackground,
    /// How photos are ordered.
    pub sort: crate::catalog::SortOrder,
}

impl Default for LibrarySettings {
    fn default() -> Self {
        Self {
            startup_view: StartupView::Loupe,
            grid_card_size: GRID_CARD_DEFAULT,
            show_left_panel: true,
            show_right_panel: true,
            auto_advance: false,
            loupe_background: LoupeBackground::Theme,
            sort: crate::catalog::SortOrder::default(),
        }
    }
}

/// What the loupe's backdrop looks like. Photographers judge tone, colour
/// and edges differently against black, 18% grey and white.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LoupeBackground {
    /// The app's own dark theme.
    #[default]
    Theme,
    Black,
    DarkGrey,
    /// 18% reflectance grey, the photographic mid-tone.
    MidGrey,
    LightGrey,
    White,
    /// Warm off-white, close to unbleached print paper.
    Paper,
}

impl LoupeBackground {
    pub const ALL: [LoupeBackground; 7] = [
        LoupeBackground::Theme,
        LoupeBackground::Black,
        LoupeBackground::DarkGrey,
        LoupeBackground::MidGrey,
        LoupeBackground::LightGrey,
        LoupeBackground::White,
        LoupeBackground::Paper,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            LoupeBackground::Theme => "App theme",
            LoupeBackground::Black => "Black",
            LoupeBackground::DarkGrey => "Dark grey",
            LoupeBackground::MidGrey => "18% grey",
            LoupeBackground::LightGrey => "Light grey",
            LoupeBackground::White => "White",
            LoupeBackground::Paper => "Paper",
        }
    }

    /// `(hue 0-1, saturation, lightness)`, or `None` to use the theme.
    pub fn hsl(&self) -> Option<(f32, f32, f32)> {
        match self {
            LoupeBackground::Theme => None,
            LoupeBackground::Black => Some((0.0, 0.0, 0.0)),
            LoupeBackground::DarkGrey => Some((0.0, 0.0, 0.16)),
            LoupeBackground::MidGrey => Some((0.0, 0.0, 0.46)),
            LoupeBackground::LightGrey => Some((0.0, 0.0, 0.78)),
            LoupeBackground::White => Some((0.0, 0.0, 1.0)),
            LoupeBackground::Paper => Some((40.0 / 360.0, 0.30, 0.93)),
        }
    }

    pub fn next(&self) -> Self {
        let all = Self::ALL;
        let at = all.iter().position(|b| b == self).unwrap_or(0);
        all[(at + 1) % all.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StartupView {
    #[default]
    Loupe,
    Grid,
}

// --- Layout ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutSettings {
    pub preset: LayoutPreset,
    pub filmstrip_position: FilmstripPosition,
    pub navigator_width: u32,
    pub inspector_width: u32,
    /// Height of the filmstrip's picture area; the frames scale with it.
    pub filmstrip_height: u32,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            preset: LayoutPreset::Classic,
            filmstrip_position: FilmstripPosition::Bottom,
            navigator_width: 260,
            inspector_width: 260,
            filmstrip_height: 80,
        }
    }
}

/// The panels that can be docked beside the photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    /// Catalog overview and filters.
    Navigator,
    /// Histogram and metadata for the selected photo. In Develop, the
    /// editing panel takes this slot.
    Inspector,
}

impl PanelKind {
    pub fn label(&self) -> &'static str {
        match self {
            PanelKind::Navigator => "Navigator",
            PanelKind::Inspector => "Inspector",
        }
    }
}

/// Which panels sit in the left and right docks, top to bottom. A dock
/// holding two panels stacks them in one column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelArrangement {
    pub left: &'static [PanelKind],
    pub right: &'static [PanelKind],
}

impl PanelArrangement {
    /// Which side the Inspector (and so Develop's editing panel) lives on;
    /// `None` when the arrangement hides it.
    pub fn inspector_side(&self) -> Option<DockSide> {
        self.side_of(PanelKind::Inspector)
    }

    /// Which side the Navigator (and so Develop's tools column) lives on;
    /// `None` when the arrangement hides it.
    pub fn navigator_side(&self) -> Option<DockSide> {
        self.side_of(PanelKind::Navigator)
    }

    fn side_of(&self, panel: PanelKind) -> Option<DockSide> {
        if self.left.contains(&panel) {
            Some(DockSide::Left)
        } else if self.right.contains(&panel) {
            Some(DockSide::Right)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockSide {
    Left,
    Right,
}

/// Named arrangements of the side panels, picked in Settings → Layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LayoutPreset {
    #[default]
    Classic,
    Mirrored,
    StackedRight,
    StackedLeft,
    InspectorOnly,
    Focus,
}

impl LayoutPreset {
    pub const ALL: [LayoutPreset; 6] = [
        LayoutPreset::Classic,
        LayoutPreset::Mirrored,
        LayoutPreset::StackedRight,
        LayoutPreset::StackedLeft,
        LayoutPreset::InspectorOnly,
        LayoutPreset::Focus,
    ];

    pub fn arrangement(&self) -> PanelArrangement {
        use PanelKind::{Inspector, Navigator};
        let (left, right): (&'static [PanelKind], &'static [PanelKind]) = match self {
            LayoutPreset::Classic => (&[Navigator], &[Inspector]),
            LayoutPreset::Mirrored => (&[Inspector], &[Navigator]),
            LayoutPreset::StackedRight => (&[], &[Navigator, Inspector]),
            LayoutPreset::StackedLeft => (&[Navigator, Inspector], &[]),
            LayoutPreset::InspectorOnly => (&[], &[Inspector]),
            LayoutPreset::Focus => (&[], &[]),
        };
        PanelArrangement { left, right }
    }

    pub fn label(&self) -> &'static str {
        LAYOUT_CARDS[self.index()].title
    }

    pub fn index(&self) -> usize {
        Self::ALL.iter().position(|p| p == self).unwrap_or(0)
    }

    pub fn from_index(ix: usize) -> Self {
        Self::ALL.get(ix).copied().unwrap_or_default()
    }

    /// The next preset, wrapping around; used by the cycle-layout shortcut.
    pub fn next(&self) -> Self {
        Self::from_index((self.index() + 1) % Self::ALL.len())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FilmstripPosition {
    #[default]
    Bottom,
    Top,
}

/// Card metadata for each preset, in `LayoutPreset::ALL` order.
pub static LAYOUT_CARDS: [CardOption; 6] = [
    CardOption {
        title: "Classic",
        description: "Navigator left, Inspector right.",
        preview: CardPreview::Layout(LayoutPreset::Classic),
    },
    CardOption {
        title: "Mirrored",
        description: "Inspector left, Navigator right.",
        preview: CardPreview::Layout(LayoutPreset::Mirrored),
    },
    CardOption {
        title: "Stacked Right",
        description: "Both panels in one column on the right.",
        preview: CardPreview::Layout(LayoutPreset::StackedRight),
    },
    CardOption {
        title: "Stacked Left",
        description: "Both panels in one column on the left.",
        preview: CardPreview::Layout(LayoutPreset::StackedLeft),
    },
    CardOption {
        title: "Inspector Only",
        description: "Just the Inspector, for culling.",
        preview: CardPreview::Layout(LayoutPreset::InspectorOnly),
    },
    CardOption {
        title: "Focus",
        description: "No side panels — all photo.",
        preview: CardPreview::Layout(LayoutPreset::Focus),
    },
];

// --- Folder tree ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FolderTreeSettings {
    pub show_photos: bool,
    pub show_hidden: bool,
    pub tree_height: u32,
}

impl Default for FolderTreeSettings {
    fn default() -> Self {
        Self {
            show_photos: true,
            show_hidden: false,
            tree_height: 280,
        }
    }
}

// --- Filmstrip ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilmstripSettings {
    pub style: FilmstripStyle,
    pub show_on_startup: bool,
    pub show_ratings: bool,
    pub show_color_labels: bool,
    pub show_edge_print: bool,
}

impl Default for FilmstripSettings {
    fn default() -> Self {
        Self {
            style: FilmstripStyle::ColorNegative,
            show_on_startup: true,
            show_ratings: true,
            show_color_labels: true,
            show_edge_print: true,
        }
    }
}

/// The filmstrip's look and feel. The palettes and structure for each live
/// in the UI crate (`rustyedit_ui::FilmLook`); this is just the choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FilmstripStyle {
    #[default]
    ColorNegative,
    BlackAndWhite,
    Slide,
    Cinema,
    Instant,
    Minimal,
}

impl FilmstripStyle {
    pub const ALL: [FilmstripStyle; 6] = [
        FilmstripStyle::ColorNegative,
        FilmstripStyle::BlackAndWhite,
        FilmstripStyle::Slide,
        FilmstripStyle::Cinema,
        FilmstripStyle::Instant,
        FilmstripStyle::Minimal,
    ];

    pub fn index(&self) -> usize {
        Self::ALL.iter().position(|s| s == self).unwrap_or(0)
    }

    pub fn from_index(ix: usize) -> Self {
        Self::ALL.get(ix).copied().unwrap_or_default()
    }
}

/// Card metadata for each style, in `FilmstripStyle::ALL` order.
pub static FILMSTRIP_STYLE_CARDS: [CardOption; 6] = [
    CardOption {
        title: "Colour Negative",
        description: "Orange C-41 base with amber edge print.",
        preview: CardPreview::Filmstrip(FilmstripStyle::ColorNegative),
    },
    CardOption {
        title: "Black & White",
        description: "Silver-grey stock; previews in monochrome.",
        preview: CardPreview::Filmstrip(FilmstripStyle::BlackAndWhite),
    },
    CardOption {
        title: "Slide",
        description: "Inky E-6 base with green edge print.",
        preview: CardPreview::Filmstrip(FilmstripStyle::Slide),
    },
    CardOption {
        title: "Cinema",
        description: "35mm motion picture: 4-perf, frame lines, key codes.",
        preview: CardPreview::Filmstrip(FilmstripStyle::Cinema),
    },
    CardOption {
        title: "Instant",
        description: "Prints with white borders on a dark desk.",
        preview: CardPreview::Filmstrip(FilmstripStyle::Instant),
    },
    CardOption {
        title: "Minimal",
        description: "Clean rounded thumbnails that match the app.",
        preview: CardPreview::Filmstrip(FilmstripStyle::Minimal),
    },
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DevelopPreferences {
    pub preview_quality: PreviewQuality,
    /// Open Develop's tools column (Navigator, Presets, Snapshots, History)
    /// expanded instead of collapsed.
    pub tools_expanded_on_startup: bool,
    /// The editing panel's section order, by `DevelopSection::key`; empty
    /// for the default. See `DevelopSection::arrange`.
    pub section_order: Vec<String>,
}

impl Default for DevelopPreferences {
    fn default() -> Self {
        Self {
            preview_quality: PreviewQuality::default(),
            tools_expanded_on_startup: true,
            section_order: Vec::new(),
        }
    }
}

/// How large the decoded "smart preview" used while editing is. Bigger is
/// sharper but makes every slider drag slower.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PreviewQuality {
    Fast,
    #[default]
    Balanced,
    High,
}

impl PreviewQuality {
    pub fn max_edge(&self) -> u32 {
        match self {
            PreviewQuality::Fast => 1024,
            PreviewQuality::Balanced => 2048,
            PreviewQuality::High => 4096,
        }
    }
}

// --- Registry -----------------------------------------------------------------

/// A top-level entry in the Settings window's sidebar.
pub struct SettingsPage {
    pub title: &'static str,
    pub sections: &'static [SettingsSection],
}

/// A titled group of related settings within a page.
pub struct SettingsSection {
    pub title: &'static str,
    pub items: &'static [SettingItem],
}

/// One user-facing option: what it's called, what it does, and which
/// control edits it.
pub struct SettingItem {
    pub label: &'static str,
    pub description: &'static str,
    pub control: SettingControl,
}

/// The kind of control used to edit a setting. Accessors are plain function
/// pointers so the whole registry can live in a `static`.
pub enum SettingControl {
    Toggle {
        get: fn(&Settings) -> bool,
        set: fn(&mut Settings, bool),
    },
    /// Pick one of a fixed set of options, addressed by index.
    Choice {
        options: &'static [&'static str],
        get: fn(&Settings) -> usize,
        set: fn(&mut Settings, usize),
    },
    /// An integer value edited with −/+ steppers, clamped to `min..=max`.
    Stepper {
        min: i64,
        max: i64,
        step: i64,
        /// Shown after the value, e.g. "px". Empty for none.
        unit: &'static str,
        get: fn(&Settings) -> i64,
        set: fn(&mut Settings, i64),
    },
    /// Pick one of several options shown as large cards with a visual
    /// preview. The row's label and description sit above the cards.
    Cards {
        options: &'static [CardOption],
        get: fn(&Settings) -> usize,
        set: fn(&mut Settings, usize),
    },
}

/// One option in a [`SettingControl::Cards`] picker.
pub struct CardOption {
    pub title: &'static str,
    pub description: &'static str,
    pub preview: CardPreview,
}

/// What a card's preview illustrates. The UI decides how to draw each kind,
/// keeping this crate free of any rendering code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardPreview {
    Layout(LayoutPreset),
    Filmstrip(FilmstripStyle),
    Grid(GridStyle),
}

pub static SETTINGS_PAGES: &[SettingsPage] = &[
    SettingsPage {
        title: "Appearance",
        sections: &[
            SettingsSection {
                title: "Theme",
                items: &[
                    SettingItem {
                        label: "Colour theme",
                        description: "The overall look of the app.",
                        control: SettingControl::Choice {
                            options: &["Dark", "Midnight", "Graphite", "Light"],
                            get: |s| s.appearance.theme.index(),
                            set: |s, ix| s.appearance.theme = AppTheme::from_index(ix),
                        },
                    },
                    SettingItem {
                        label: "Accent colour",
                        description: "Highlights, selections and active controls.",
                        control: SettingControl::Choice {
                            options: &["Blue", "Teal", "Green", "Orange", "Pink", "Purple"],
                            get: |s| s.appearance.accent.index(),
                            set: |s, ix| s.appearance.accent = AccentColor::from_index(ix),
                        },
                    },
                ],
            },
            SettingsSection {
                title: "Contrast",
                items: &[SettingItem {
                    label: "Interface contrast",
                    description: "How strongly panels, controls and text stand apart. \
                                  50 is the standard look.",
                    control: SettingControl::Stepper {
                        min: 0,
                        max: 100,
                        step: 5,
                        unit: "",
                        get: |s| s.appearance.contrast as i64,
                        set: |s, v| s.appearance.contrast = v.clamp(0, 100) as u8,
                    },
                }],
            },
        ],
    },
    SettingsPage {
        title: "Library",
        sections: &[SettingsSection {
            title: "View",
            items: &[
                SettingItem {
                    label: "Startup view",
                    description: "What the center of the Library shows when the app opens.",
                    control: SettingControl::Choice {
                        options: &["Loupe", "Grid"],
                        get: |s| s.library.startup_view as usize,
                        set: |s, ix| {
                            s.library.startup_view = if ix == 0 {
                                StartupView::Loupe
                            } else {
                                StartupView::Grid
                            }
                        },
                    },
                },
                SettingItem {
                    label: "Auto-advance",
                    description: "After flagging or rating a photo, jump to the next one.",
                    control: SettingControl::Toggle {
                        get: |s| s.library.auto_advance,
                        set: |s, v| s.library.auto_advance = v,
                    },
                },
            ],
        }],
    },
    SettingsPage {
        title: "Grid",
        sections: &[
            SettingsSection {
                title: "Cards",
                items: &[SettingItem {
                    label: "Card style",
                    description: "How much each card shows around its photo. \
                                  Cycle through these anytime with J.",
                    control: SettingControl::Cards {
                        options: &GRID_STYLE_CARDS,
                        get: |s| s.grid.style.index(),
                        set: |s, ix| s.grid.style = GridStyle::from_index(ix),
                    },
                }],
            },
            SettingsSection {
                title: "Thumbnails",
                items: &[
                    SettingItem {
                        label: "Thumbnail size",
                        description: "Width of each card; the number of columns follows.",
                        control: SettingControl::Stepper {
                            min: GRID_CARD_MIN as i64,
                            max: GRID_CARD_MAX as i64,
                            step: GRID_CARD_STEP as i64,
                            unit: " px",
                            get: |s| s.library.grid_card_size as i64,
                            set: |s, v| s.library.grid_card_size = v as u32,
                        },
                    },
                    SettingItem {
                        label: "Shape",
                        description: "Proportions of the thumbnail area.",
                        control: SettingControl::Choice {
                            options: &["4:3", "3:2", "Square"],
                            get: |s| s.grid.aspect as usize,
                            set: |s, ix| {
                                s.grid.aspect = match ix {
                                    1 => GridAspect::Classic,
                                    2 => GridAspect::Square,
                                    _ => GridAspect::Landscape,
                                }
                            },
                        },
                    },
                    SettingItem {
                        label: "Crop to fill",
                        description: "Fill the whole thumbnail area, trimming the photo's edges.",
                        control: SettingControl::Toggle {
                            get: |s| s.grid.crop_to_fill,
                            set: |s, v| s.grid.crop_to_fill = v,
                        },
                    },
                    SettingItem {
                        label: "Spacing",
                        description: "Gap between cards.",
                        control: SettingControl::Choice {
                            options: &["Tight", "Normal", "Roomy"],
                            get: |s| s.grid.spacing as usize,
                            set: |s, ix| {
                                s.grid.spacing = match ix {
                                    0 => GridSpacing::Tight,
                                    2 => GridSpacing::Roomy,
                                    _ => GridSpacing::Normal,
                                }
                            },
                        },
                    },
                ],
            },
            SettingsSection {
                title: "Details",
                items: &[
                    SettingItem {
                        label: "Hover controls",
                        description: "Show pick, reject and rotate buttons over a hovered card.",
                        control: SettingControl::Toggle {
                            get: |s| s.grid.hover_controls,
                            set: |s, v| s.grid.hover_controls = v,
                        },
                    },
                    SettingItem {
                        label: "Capture details",
                        description: "Show date, camera and size under the name (Detailed style).",
                        control: SettingControl::Toggle {
                            get: |s| s.grid.show_details,
                            set: |s, v| s.grid.show_details = v,
                        },
                    },
                    SettingItem {
                        label: "Colour label bar",
                        description:
                            "Run a bar in the label's colour along the card's bottom edge.",
                        control: SettingControl::Toggle {
                            get: |s| s.grid.show_label_bar,
                            set: |s, v| s.grid.show_label_bar = v,
                        },
                    },
                    SettingItem {
                        label: "Dim rejected photos",
                        description: "Fade cards that are flagged as rejected.",
                        control: SettingControl::Toggle {
                            get: |s| s.grid.dim_rejected,
                            set: |s, v| s.grid.dim_rejected = v,
                        },
                    },
                    SettingItem {
                        label: "Info button",
                        description:
                            "Show an \u{24D8} on each card; click it to see the photo's metadata.",
                        control: SettingControl::Toggle {
                            get: |s| s.grid.tooltips,
                            set: |s, v| s.grid.tooltips = v,
                        },
                    },
                ],
            },
        ],
    },
    SettingsPage {
        title: "Layout",
        sections: &[
            SettingsSection {
                title: "Panels",
                items: &[SettingItem {
                    label: "Arrangement",
                    description: "Where the Navigator and Inspector panels sit around the photo. \
                                  Cycle through these anytime with ⌘⇧L.",
                    control: SettingControl::Cards {
                        options: &LAYOUT_CARDS,
                        get: |s| s.layout.preset.index(),
                        set: |s, ix| s.layout.preset = LayoutPreset::from_index(ix),
                    },
                }],
            },
            SettingsSection {
                title: "Sizing",
                items: &[
                    SettingItem {
                        label: "Navigator width",
                        description: "Width of the Navigator panel when expanded.",
                        control: SettingControl::Stepper {
                            min: 200,
                            max: 420,
                            step: 20,
                            unit: "px",
                            get: |s| s.layout.navigator_width as i64,
                            set: |s, v| s.layout.navigator_width = v as u32,
                        },
                    },
                    SettingItem {
                        label: "Inspector width",
                        description: "Width of the Inspector and Develop panels when expanded.",
                        control: SettingControl::Stepper {
                            min: 220,
                            max: 440,
                            step: 20,
                            unit: "px",
                            get: |s| s.layout.inspector_width as i64,
                            set: |s, v| s.layout.inspector_width = v as u32,
                        },
                    },
                ],
            },
            SettingsSection {
                title: "Filmstrip",
                items: &[SettingItem {
                    label: "Filmstrip position",
                    description: "Run the filmstrip along the bottom or the top of the window.",
                    control: SettingControl::Choice {
                        options: &["Bottom", "Top"],
                        get: |s| s.layout.filmstrip_position as usize,
                        set: |s, ix| {
                            s.layout.filmstrip_position = if ix == 1 {
                                FilmstripPosition::Top
                            } else {
                                FilmstripPosition::Bottom
                            }
                        },
                    },
                }],
            },
            SettingsSection {
                title: "Startup",
                items: &[
                    SettingItem {
                        label: "Expand left dock on startup",
                        description: "Open the left-hand panels expanded instead of collapsed.",
                        control: SettingControl::Toggle {
                            get: |s| s.library.show_left_panel,
                            set: |s, v| s.library.show_left_panel = v,
                        },
                    },
                    SettingItem {
                        label: "Expand right dock on startup",
                        description: "Open the right-hand panels expanded instead of collapsed.",
                        control: SettingControl::Toggle {
                            get: |s| s.library.show_right_panel,
                            set: |s, v| s.library.show_right_panel = v,
                        },
                    },
                ],
            },
        ],
    },
    SettingsPage {
        title: "Folders",
        sections: &[SettingsSection {
            title: "Folder tree",
            items: &[
                SettingItem {
                    label: "Show photos",
                    description: "List photo files under each folder, not just subfolders.",
                    control: SettingControl::Toggle {
                        get: |s| s.folders.show_photos,
                        set: |s, v| s.folders.show_photos = v,
                    },
                },
                SettingItem {
                    label: "Show hidden items",
                    description: "Include files and folders whose names start with a dot.",
                    control: SettingControl::Toggle {
                        get: |s| s.folders.show_hidden,
                        set: |s, v| s.folders.show_hidden = v,
                    },
                },
                SettingItem {
                    label: "Tree height",
                    description: "Height of the folder tree in the Navigator.",
                    control: SettingControl::Stepper {
                        min: 160,
                        max: 640,
                        step: 40,
                        unit: "px",
                        get: |s| s.folders.tree_height as i64,
                        set: |s, v| s.folders.tree_height = v as u32,
                    },
                },
            ],
        }],
    },
    SettingsPage {
        title: "Filmstrip",
        sections: &[
            SettingsSection {
                title: "Look",
                items: &[SettingItem {
                    label: "Style",
                    description: "The filmstrip's look and feel.",
                    control: SettingControl::Cards {
                        options: &FILMSTRIP_STYLE_CARDS,
                        get: |s| s.filmstrip.style.index(),
                        set: |s, ix| s.filmstrip.style = FilmstripStyle::from_index(ix),
                    },
                }],
            },
            SettingsSection {
                title: "Appearance",
                items: &[
                    SettingItem {
                        label: "Show filmstrip on startup",
                        description: "Open the filmstrip expanded instead of collapsed.",
                        control: SettingControl::Toggle {
                            get: |s| s.filmstrip.show_on_startup,
                            set: |s, v| s.filmstrip.show_on_startup = v,
                        },
                    },
                    SettingItem {
                        label: "Star ratings",
                        description: "Print each photo's rating under its frame.",
                        control: SettingControl::Toggle {
                            get: |s| s.filmstrip.show_ratings,
                            set: |s, v| s.filmstrip.show_ratings = v,
                        },
                    },
                    SettingItem {
                        label: "Colour labels",
                        description: "Show the colour label bar and dot on each frame.",
                        control: SettingControl::Toggle {
                            get: |s| s.filmstrip.show_color_labels,
                            set: |s, v| s.filmstrip.show_color_labels = v,
                        },
                    },
                    SettingItem {
                        label: "Edge print",
                        description: "Show frame numbers and film stock along the film's edge.",
                        control: SettingControl::Toggle {
                            get: |s| s.filmstrip.show_edge_print,
                            set: |s, v| s.filmstrip.show_edge_print = v,
                        },
                    },
                ],
            },
        ],
    },
    SettingsPage {
        title: "Develop",
        sections: &[
            SettingsSection {
                title: "Tools column",
                items: &[SettingItem {
                    label: "Expand on startup",
                    description: "Open the Navigator, Presets, Snapshots and History column \
                                  expanded. It sits where the layout puts the Navigator.",
                    control: SettingControl::Toggle {
                        get: |s| s.develop.tools_expanded_on_startup,
                        set: |s, v| s.develop.tools_expanded_on_startup = v,
                    },
                }],
            },
            SettingsSection {
                title: "Performance",
                items: &[SettingItem {
                    label: "Preview quality",
                    description: "Size of the working preview while editing. Higher is sharper \
                              but makes sliders slower.",
                    control: SettingControl::Choice {
                        options: &["Fast (1024px)", "Balanced (2048px)", "High (4096px)"],
                        get: |s| s.develop.preview_quality as usize,
                        set: |s, ix| {
                            s.develop.preview_quality = match ix {
                                0 => PreviewQuality::Fast,
                                2 => PreviewQuality::High,
                                _ => PreviewQuality::Balanced,
                            }
                        },
                    },
                }],
            },
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_cards_match_styles_in_order() {
        for (ix, style) in GridStyle::ALL.iter().enumerate() {
            assert_eq!(style.index(), ix);
            assert_eq!(GridStyle::from_index(ix), *style);
            assert_eq!(GRID_STYLE_CARDS[ix].preview, CardPreview::Grid(*style));
        }
        assert_eq!(GridStyle::Bare.next(), GridStyle::Detailed);
    }

    #[test]
    fn layout_cards_match_presets_in_order() {
        for (ix, preset) in LayoutPreset::ALL.iter().enumerate() {
            assert_eq!(preset.index(), ix);
            assert_eq!(LayoutPreset::from_index(ix), *preset);
            assert_eq!(LAYOUT_CARDS[ix].preview, CardPreview::Layout(*preset));
        }
    }

    #[test]
    fn every_preset_places_each_panel_at_most_once() {
        for preset in LayoutPreset::ALL {
            let a = preset.arrangement();
            for kind in [PanelKind::Navigator, PanelKind::Inspector] {
                let n = a.left.iter().chain(a.right).filter(|k| **k == kind).count();
                assert!(n <= 1, "{preset:?} places {kind:?} {n} times");
            }
        }
    }

    #[test]
    fn filmstrip_style_cards_match_styles_in_order() {
        for (ix, style) in FilmstripStyle::ALL.iter().enumerate() {
            assert_eq!(style.index(), ix);
            assert_eq!(FilmstripStyle::from_index(ix), *style);
            assert_eq!(
                FILMSTRIP_STYLE_CARDS[ix].preview,
                CardPreview::Filmstrip(*style)
            );
        }
    }

    #[test]
    fn next_cycles_through_every_preset() {
        let mut seen = vec![LayoutPreset::default()];
        let mut p = LayoutPreset::default().next();
        while p != LayoutPreset::default() {
            seen.push(p);
            p = p.next();
        }
        assert_eq!(seen.len(), LayoutPreset::ALL.len());
    }
}
