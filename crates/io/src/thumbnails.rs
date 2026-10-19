use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

use image::imageops::FilterType;
use rustyedit_core::Asset;

/// Longest edge, in pixels, that cached library/filmstrip thumbnails are
/// generated at. Kept small so culling through thousands of images stays
/// fast: decoding + resizing is the dominant cost, not disk IO.
pub const THUMBNAIL_LONG_EDGE: u32 = 512;

#[derive(Debug, Clone)]
pub struct ThumbnailCache {
    dir: PathBuf,
}

impl ThumbnailCache {
    /// A cache rooted under the OS cache directory, namespaced per source
    /// folder so thumbnails for different shoots never collide.
    pub fn for_folder(folder: &Path) -> Self {
        let base = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
        let mut hasher = DefaultHasher::new();
        folder.hash(&mut hasher);
        let folder_key = format!("{:016x}", hasher.finish());

        let dir = base.join("rustyedit").join("thumbnails").join(folder_key);
        let _ = std::fs::create_dir_all(&dir);
        Self { dir }
    }

    fn cache_path_for(&self, path: &Path) -> Option<PathBuf> {
        let metadata = std::fs::metadata(path).ok()?;
        let modified = metadata.modified().ok()?;
        let modified_secs = modified
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        modified_secs.hash(&mut hasher);
        metadata.len().hash(&mut hasher);
        let key = format!("{:016x}.jpg", hasher.finish());

        Some(self.dir.join(key))
    }

    /// Returns a cached thumbnail path if one already exists on disk.
    pub fn cached(&self, source: &Path) -> Option<PathBuf> {
        let cache_path = self.cache_path_for(source)?;
        cache_path.exists().then_some(cache_path)
    }

    /// Decodes `source`, writes a resized thumbnail to the cache (unless one
    /// already exists), and returns the thumbnail path plus the full-size
    /// image dimensions. Does real decode work, so callers should run this
    /// off the UI thread.
    pub fn ensure(&self, source: &Path) -> anyhow::Result<(PathBuf, u32, u32)> {
        let cache_path = self
            .cache_path_for(source)
            .ok_or_else(|| anyhow::anyhow!("could not stat {}", source.display()))?;

        if cache_path.exists() {
            if let Ok((w, h)) = image::image_dimensions(source) {
                return Ok((cache_path, w, h));
            }
        }

        let image = image::open(source)?;
        let (width, height) = (image.width(), image.height());

        let thumbnail = image.resize(
            THUMBNAIL_LONG_EDGE,
            THUMBNAIL_LONG_EDGE,
            FilterType::Triangle,
        );
        thumbnail.to_rgb8().save(&cache_path)?;

        Ok((cache_path, width, height))
    }
}

/// Generates (or loads from cache) thumbnails for every asset in parallel
/// using a rayon thread pool, returning `(asset id, thumbnail path, width,
/// height)` for each asset that decoded successfully. Intended to be run
/// inside `cx.background_executor().spawn(..)` so the UI thread is never
/// blocked while a large folder is being culled.
pub fn generate_thumbnails(
    cache: &ThumbnailCache,
    assets: &[Asset],
) -> Vec<(u64, PathBuf, u32, u32)> {
    use rayon::prelude::*;

    assets
        .par_iter()
        .filter_map(|asset| {
            cache
                .ensure(&asset.path)
                .ok()
                .map(|(thumb_path, width, height)| (asset.id, thumb_path, width, height))
        })
        .collect()
}
