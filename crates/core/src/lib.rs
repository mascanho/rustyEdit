//! Domain model for RustyEdit: assets, catalogs, flags, ratings and the
//! develop (edit) settings. No GPUI or IO dependency lives here so the
//! model can be reasoned about and tested in isolation.

mod albums;
mod asset;
mod catalog;
mod catalog_def;
mod collections;
mod color_label;
mod crop;
mod curves;
mod develop;
mod duplicates;
mod export;
mod fuji_recipe;
mod filters;
mod frame;
mod flag;
mod gradients;
mod grading;
mod histogram;
mod history;
mod mixer;
mod lens;
mod lens_blur;
mod film_stocks;
mod parametric;
mod presets;
mod rail;
mod rating;
mod retouch;
mod sections;
pub mod culling;
pub mod settings;
mod snapshot;

pub use albums::{Album, Albums};
pub use asset::{is_raw_path, AppliedPreset, Asset, AssetState, PhotoMeta, RAW_EXTENSIONS};
pub use culling::{CullInfo, CullMark};
pub use catalog::{Catalog, CatalogCounts, FilterMode, PairView, RatingFilter, SortKey, SortOrder};
pub use catalog_def::CatalogDef;
pub use collections::{default_keyword_sets, KeywordSet, SmartCollection, SmartCollections};
pub use color_label::ColorLabel;
pub use crop::{drag_crop_rect, rotate_about_centre, AspectLock, Crop, CropHandle, CropView, MAX_PERSPECTIVE, MAX_STRAIGHTEN};
pub use curves::{CurveChannel, CurveLook, CurvePreset, ToneCurve, ToneCurves, MAX_CURVE_POINTS};
pub use develop::{Adjustment, DevelopSettings};
pub use duplicates::{
    find_duplicates, find_duplicates_in, hamming, DuplicateGroup, DuplicateKind, PhotoHash,
    SIMILAR_HAMMING,
};
pub use export::{
    builtin_export_presets, ExportBitDepth, ExportColorSpace, ExportConflict, ExportFormat, ExportNaming,
    ExportOptions, ExportPngCompression, ExportPreset, ExportScope, ExportSharpen, ExportSize,
    ExportSubfolder, Watermark, WatermarkPosition,
};
pub use filters::{FilterSet, FocalBand, IsoBand};
pub use flag::Flag;
pub use frame::{FrameSettings, FrameStyle, FRAME_GROUPS};
pub use gradients::{Dab, PackedDab, Gradient, GradientKind, DAB_SPACING, MAX_DABS, Gradients, LocalAdjust, LocalProp, MAX_GRADIENTS};
pub use grading::{ColorGrading, Grade, GradeProp, ToneZone};
pub use histogram::{
    luma8, Clipping, Histogram, HistogramChannel, ScopeMode, Scopes, TonalZone, VECTOR_SIZE,
    WAVE_COLS, WAVE_ROWS,
};
pub use history::HistoryEntry;
pub use mixer::{band_weight, BandAdjust, ColorBand, ColorMixer, MixerProp};
pub use fuji_recipe::{
    DynamicRange, FilmSimulation, FujiRecipe, GrainEffect, MonoFilter, Strength, WhiteBalance,
};
pub use lens::{lens_profile, LensCorrection, LensProfile};
pub use lens_blur::{BokehShape, LensBlur, MAX_BLUR as MAX_LENS_BLUR};
pub use parametric::ParametricCurve;
pub use presets::{builtin_presets, DevelopPreset, Presets, PRESET_CATEGORIES};
pub use rail::RailTool;
pub use rating::Rating;
pub use retouch::{to_display, to_source, Spot, SpotKind, Spots, MAX_SPOTS};
pub use sections::DevelopSection;
pub use settings::Settings;
pub use snapshot::{next_snapshot_name, Snapshot};
