//! Domain model for RustyEdit: assets, catalogs, flags, ratings and the
//! develop (edit) settings. No GPUI or IO dependency lives here so the
//! model can be reasoned about and tested in isolation.

mod asset;
mod catalog;
mod color_label;
mod develop;
mod flag;
mod histogram;
mod rating;
pub mod settings;

pub use asset::Asset;
pub use catalog::{Catalog, CatalogCounts, FilterMode};
pub use color_label::ColorLabel;
pub use develop::{Adjustment, DevelopSettings};
pub use flag::Flag;
pub use histogram::Histogram;
pub use rating::Rating;
pub use settings::Settings;
