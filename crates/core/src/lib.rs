//! Domain model for RustyEdit: assets, catalogs, flags, ratings and the
//! develop (edit) settings. No GPUI or IO dependency lives here so the
//! model can be reasoned about and tested in isolation.

mod albums;
mod asset;
mod catalog;
mod catalog_def;
mod color_label;
mod develop;
mod export;
mod filters;
mod flag;
mod histogram;
mod history;
mod rail;
mod rating;
pub mod settings;

pub use albums::{Album, Albums};
pub use asset::{Asset, AssetState, PhotoMeta};
pub use catalog::{Catalog, CatalogCounts, FilterMode, RatingFilter, SortKey, SortOrder};
pub use catalog_def::CatalogDef;
pub use color_label::ColorLabel;
pub use develop::{Adjustment, DevelopSettings};
pub use export::{ExportFormat, ExportOptions, ExportScope, ExportSize};
pub use filters::{FilterSet, FocalBand, IsoBand};
pub use flag::Flag;
pub use histogram::Histogram;
pub use history::HistoryEntry;
pub use rail::RailTool;
pub use rating::Rating;
pub use settings::Settings;
