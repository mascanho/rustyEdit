//! Filesystem-facing concerns: scanning folders for images, generating and
//! caching thumbnails, and persisting catalog state (flags/ratings/edits) so
//! it survives between app launches. Kept free of any GPUI dependency so it
//! can run on background threads.

mod album_store;
mod browse;
mod catalog_registry;
mod catalog_store;
mod decode;
mod maintenance;
mod metadata;
mod preset_store;
mod scan;
mod settings_store;
mod thumbnails;
pub mod xmp;

pub use album_store::{load as load_albums, save as save_albums};
pub use browse::{browse_roots, list_directory, FsEntry};
pub mod registry {
    pub use crate::catalog_registry::*;
}
pub use catalog_store::{
    load as load_catalog, merge_saved_state, missing_assets, save as save_catalog,
};
pub use decode::{exif_orientation, image_dimensions, open_for_develop, open_image};
pub use maintenance::{
    build_backup, move_to_trash, optimize_catalog, read_backup, restore_backup, write_backup,
    BackupSidecar, CatalogBackup, OptimizeReport,
};
pub use preset_store::{
    export_to as export_presets, import_from as import_presets, load as load_presets,
    save as save_presets,
};
pub use metadata::{read_metadata, read_photo_meta, MetaGroup, Metadata};
pub use scan::scan_folder;
pub use settings_store::{load as load_settings, save as save_settings, settings_path};
pub use thumbnails::{
    cache_size, clear_cache, generate_thumbnails, ThumbInfo, ThumbnailCache, LOUPE_LONG_EDGE,
    THUMBNAIL_LONG_EDGE,
};
