//! Filesystem-facing concerns: scanning folders for images, generating and
//! caching thumbnails, and persisting catalog state (flags/ratings/edits) so
//! it survives between app launches. Kept free of any GPUI dependency so it
//! can run on background threads.

mod browse;
mod catalog_store;
mod preview;
mod scan;
mod settings_store;
mod thumbnails;

pub use browse::{browse_roots, list_directory, FsEntry};
pub use catalog_store::{load as load_catalog, merge_saved_state, save as save_catalog};
pub use preview::{cleanup_live_preview, next_live_preview_path};
pub use scan::scan_folder;
pub use settings_store::{load as load_settings, save as save_settings, settings_path};
pub use thumbnails::{generate_thumbnails, ThumbnailCache, THUMBNAIL_LONG_EDGE};
