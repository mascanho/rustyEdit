//! Filesystem-facing concerns: scanning folders for images, generating and
//! caching thumbnails, and persisting catalog state (flags/ratings/edits) so
//! it survives between app launches. Kept free of any GPUI dependency so it
//! can run on background threads.

mod album_store;
mod browse;
mod catalog_registry;
mod catalog_store;
mod decode;
mod fuji;
mod maintenance;
mod metadata;
mod pdf;
mod recovery;
mod shortcuts_store;
pub mod zine;
pub mod zine_raster;
pub mod zine_web;
mod zip_store;
mod lens_db;
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
pub use fuji::read_fuji_recipe;
pub use decode::{
    embedded_preview, exif_orientation, image_dimensions, is_raw_file, open_for_develop,
    open_image,
};
pub use maintenance::{
    build_backup, move_all_to_trash, move_to_trash, optimize_catalog, read_backup, restore_backup, write_backup,
    BackupSidecar, CatalogBackup, OptimizeReport,
};
pub use recovery::{
    begin_session, end_session, had_unclean_exit, latest_auto_backup, list_auto_backups,
    read_auto_backup, relative_time, write_auto_backup, AutoBackup,
};
pub use shortcuts_store::{load_shortcut_overrides, save_shortcut_overrides};
pub use lens_db::{find_lens_profile, lens_database_size, lensfun_dirs, LensDb};
pub use preset_store::{
    export_to as export_presets, import_from as import_presets, load as load_presets,
    save as save_presets,
};
pub use metadata::{read_gps, read_metadata, read_photo_meta, MetaGroup, Metadata};
pub use scan::scan_folder;
pub use settings_store::{load as load_settings, save as save_settings, settings_path};
pub use thumbnails::{
    cache_size, clear_cache, generate_thumbnails, generate_thumbnails_in_batches, ThumbInfo, ThumbnailCache, LOUPE_LONG_EDGE,
    THUMBNAIL_LONG_EDGE,
};
