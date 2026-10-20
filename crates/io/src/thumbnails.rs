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

/// Longest edge of the loupe preview generated for RAW files.
pub const LOUPE_LONG_EDGE: u32 = 2560;

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
        // Bump when the cached pixels change meaning (v2: EXIF-oriented).
        2u8.hash(&mut hasher);
        path.hash(&mut hasher);
        modified_secs.hash(&mut hasher);
        metadata.len().hash(&mut hasher);
        let key = format!("{:016x}.jpg", hasher.finish());

        Some(self.dir.join(key))
    }

    fn loupe_path_for(&self, source: &Path) -> Option<PathBuf> {
        let thumb = self.cache_path_for(source)?;
        let stem = thumb.file_stem()?.to_string_lossy().into_owned();
        Some(self.dir.join(format!("{stem}-loupe.jpg")))
    }

    /// Whether the original needs a generated, larger preview to be shown
    /// in the loupe (formats GPUI can't render directly, i.e. camera RAW).
    pub fn needs_loupe_preview(source: &Path, orientation: u8) -> bool {
        crate::decode::is_raw(source) || orientation > 1
    }

    fn full_path_for(&self, source: &Path) -> Option<PathBuf> {
        let thumb = self.cache_path_for(source)?;
        let stem = thumb.file_stem()?.to_string_lossy().into_owned();
        Some(self.dir.join(format!("{stem}-full.jpg")))
    }

    /// Writes an upright, full-resolution JPEG for viewing at 100% and
    /// beyond, for originals the UI can't show directly. Heavy; run off the
    /// UI thread.
    pub fn ensure_full(&self, source: &Path) -> anyhow::Result<PathBuf> {
        let path = self
            .full_path_for(source)
            .ok_or_else(|| anyhow::anyhow!("could not stat {}", source.display()))?;
        if !path.exists() {
            crate::decode::open_image(source)?.to_rgb8().save(&path)?;
        }
        Ok(path)
    }

    /// A previously generated loupe-size preview, if one exists on disk.
    pub fn cached_loupe(&self, source: &Path) -> Option<PathBuf> {
        let path = self.loupe_path_for(source)?;
        path.exists().then_some(path)
    }

    /// Decodes `source` and writes a loupe-size JPEG (unless cached).
    /// Does real decode work; run off the UI thread.
    pub fn ensure_loupe(&self, source: &Path) -> anyhow::Result<PathBuf> {
        let path = self
            .loupe_path_for(source)
            .ok_or_else(|| anyhow::anyhow!("could not stat {}", source.display()))?;
        if path.exists() {
            return Ok(path);
        }
        let image = crate::decode::open_image(source)?;
        let image = if image.width().max(image.height()) > LOUPE_LONG_EDGE {
            image.resize(LOUPE_LONG_EDGE, LOUPE_LONG_EDGE, FilterType::Triangle)
        } else {
            image
        };
        image.to_rgb8().save(&path)?;
        Ok(path)
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
            if let Ok((w, h)) = crate::decode::image_dimensions(source) {
                return Ok((cache_path, w, h));
            }
        }

        let image = crate::decode::open_image(source)?;
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
    progress: &std::sync::atomic::AtomicUsize,
) -> Vec<ThumbInfo> {
    use rayon::prelude::*;
    use std::sync::atomic::Ordering;

    assets
        .par_iter()
        .filter_map(|asset| {
            // Already thumbnailed on a previous run and its size is known:
            // no need to decode the (possibly RAW) original again; only
            // fill in EXIF facts an older catalog didn't record.
            if asset.width > 0 && asset.height > 0 {
                if let Some(thumb) = cache.cached(&asset.path) {
                    let meta = if asset.meta.captured != 0 {
                        asset.meta.clone()
                    } else {
                        crate::metadata::read_photo_meta(&asset.path)
                    };
                    progress.fetch_add(1, Ordering::Relaxed);
                    return Some(ThumbInfo {
                        id: asset.id,
                        thumbnail: thumb,
                        width: asset.width,
                        height: asset.height,
                        orientation: asset.orientation,
                        meta,
                    });
                }
            }
            let result = cache
                .ensure(&asset.path)
                .ok()
                .map(|(thumbnail, width, height)| ThumbInfo {
                    id: asset.id,
                    thumbnail,
                    width,
                    height,
                    orientation: crate::decode::exif_orientation(&asset.path),
                    meta: crate::metadata::read_photo_meta(&asset.path),
                });
            progress.fetch_add(1, Ordering::Relaxed);
            result
        })
        .collect()
}

/// What importing one photo produced.
pub struct ThumbInfo {
    pub id: u64,
    pub thumbnail: PathBuf,
    pub width: u32,
    pub height: u32,
    pub orientation: u8,
    pub meta: rustyedit_core::PhotoMeta,
}
