use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use image::{imageops::FilterType, DynamicImage};
use rustyedit_core::Asset;

/// Longest edge, in pixels, that cached library/filmstrip thumbnails are
/// generated at. Kept small so culling through thousands of images stays
/// fast: decoding + resizing is the dominant cost, not disk IO.
pub const THUMBNAIL_LONG_EDGE: u32 = 512;

/// Longest edge of the loupe preview generated for RAW files.
pub const LOUPE_LONG_EDGE: u32 = 2560;

/// `image` scaled so its longest edge is `edge` (Triangle filter).
/// Shrinking runs on the GPU when there is one; the decode stays the main
/// cost of a thumbnail.
fn fit(image: &DynamicImage, edge: u32) -> DynamicImage {
    if image.width().max(image.height()) > edge {
        rustyedit_processing::downscale_for_preview(image, edge)
    } else {
        image.resize(edge, edge, FilterType::Triangle)
    }
}

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

    /// Cache path for a variant of the thumbnail (rotated, loupe, full).
    fn variant_path_for(&self, source: &Path, suffix: &str) -> Option<PathBuf> {
        let thumb = self.cache_path_for(source)?;
        let stem = thumb.file_stem()?.to_string_lossy().into_owned();
        Some(self.dir.join(format!("{stem}{suffix}.jpg")))
    }

    fn rotated_path_for(&self, source: &Path, quarters: u8) -> Option<PathBuf> {
        self.variant_path_for(source, &format!("-rot{}", quarters % 4))
    }

    /// Whether the original needs a generated, larger preview to be shown
    /// in the loupe (formats GPUI can't render directly, i.e. camera RAW,
    /// EXIF-rotated photos, or library rotations).
    pub fn needs_loupe_preview(source: &Path, orientation: u8, rotation_quarters: u8) -> bool {
        crate::decode::is_raw(source) || orientation > 1 || !rotation_quarters.is_multiple_of(4)
    }

    /// Removes every cached derivative of `source` (thumbnail, loupe, full,
    /// rotated variants). Must be called while the source still exists so
    /// its cache key can be computed.
    pub fn forget(&self, source: &Path) {
        let Some(base) = self.cache_path_for(source) else {
            return;
        };
        let Some(stem) = base.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            return;
        };
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&stem) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    /// Rotates a decoded image by `quarters` × 90° clockwise.
    fn rotate_image(image: DynamicImage, quarters: u8) -> DynamicImage {
        match quarters % 4 {
            1 => image.rotate90(),
            2 => image.rotate180(),
            3 => image.rotate270(),
            _ => image,
        }
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

    /// Like [`Self::cached`], but for a rotated (library rotation) thumbnail.
    pub fn cached_rotated(&self, source: &Path, quarters: u8) -> Option<PathBuf> {
        if quarters.is_multiple_of(4) {
            return self.cached(source);
        }
        let path = self.rotated_path_for(source, quarters)?;
        path.exists().then_some(path)
    }

    /// Generates (or loads) a thumbnail rotated by `quarters` × 90° clockwise.
    /// Returns the rotated pixel dimensions alongside the cached path.
    pub fn ensure_rotated(
        &self,
        source: &Path,
        quarters: u8,
    ) -> anyhow::Result<(PathBuf, u32, u32)> {
        let quarters = quarters % 4;
        if quarters == 0 {
            return self.ensure(source);
        }
        let path = self
            .rotated_path_for(source, quarters)
            .ok_or_else(|| anyhow::anyhow!("could not stat {}", source.display()))?;
        if path.exists() {
            if let Ok((w, h)) = crate::decode::image_dimensions(source) {
                let (w, h) = if quarters % 2 == 1 { (h, w) } else { (w, h) };
                return Ok((path, w, h));
            }
        }
        // Shrink first, then turn the small image: far less to rotate.
        let image = crate::decode::open_image(source)?;
        let (width, height) = if quarters % 2 == 1 {
            (image.height(), image.width())
        } else {
            (image.width(), image.height())
        };
        Self::rotate_image(fit(&image, THUMBNAIL_LONG_EDGE), quarters)
            .to_rgb8()
            .save(&path)?;
        Ok((path, width, height))
    }

    /// A previously generated loupe-size preview, if one exists on disk.
    pub fn cached_loupe(&self, source: &Path) -> Option<PathBuf> {
        let path = self.loupe_path_for(source)?;
        path.exists().then_some(path)
    }

    /// Decodes `source` and writes a loupe-size JPEG (unless cached).
    /// Does real decode work; run off the UI thread.
    pub fn ensure_loupe(&self, source: &Path) -> anyhow::Result<PathBuf> {
        self.ensure_loupe_rotated(source, 0)
    }

    /// Loupe preview with the library rotation applied.
    pub fn ensure_loupe_rotated(&self, source: &Path, quarters: u8) -> anyhow::Result<PathBuf> {
        let quarters = quarters % 4;
        let path = if quarters == 0 {
            self.loupe_path_for(source)
        } else {
            self.variant_path_for(source, &format!("-loupe-rot{quarters}"))
        }
        .ok_or_else(|| anyhow::anyhow!("could not stat {}", source.display()))?;
        if path.exists() {
            return Ok(path);
        }
        let image = crate::decode::open_image(source)?;
        let image = if image.width().max(image.height()) > LOUPE_LONG_EDGE {
            fit(&image, LOUPE_LONG_EDGE)
        } else {
            image
        };
        Self::rotate_image(image, quarters).to_rgb8().save(&path)?;
        Ok(path)
    }

    /// Full-resolution preview with the library rotation applied.
    pub fn ensure_full_rotated(&self, source: &Path, quarters: u8) -> anyhow::Result<PathBuf> {
        let quarters = quarters % 4;
        let path = if quarters == 0 {
            self.full_path_for(source)
        } else {
            self.variant_path_for(source, &format!("-full-rot{quarters}"))
        }
        .ok_or_else(|| anyhow::anyhow!("could not stat {}", source.display()))?;
        if !path.exists() {
            Self::rotate_image(crate::decode::open_image(source)?, quarters)
                .to_rgb8()
                .save(&path)?;
        }
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

        fit(&image, THUMBNAIL_LONG_EDGE)
            .to_rgb8()
            .save(&cache_path)?;

        Ok((cache_path, width, height))
    }
}

/// Generates (or loads from cache) thumbnails for every asset in parallel
/// using a rayon thread pool, returning `(asset id, thumbnail path, width,
/// height)` for each asset that decoded successfully. Intended to be run
/// inside `cx.background_executor().spawn(..)` so the UI thread is never
/// blocked while a large folder is being culled.
///
/// With `measure_sharpness` off, photos keep whatever score they have and
/// none are measured (no extra decoding).
pub fn generate_thumbnails(
    cache: &ThumbnailCache,
    assets: &[Asset],
    progress: &std::sync::atomic::AtomicUsize,
    measure_sharpness: bool,
) -> Vec<ThumbInfo> {
    use rayon::prelude::*;
    use std::sync::atomic::Ordering;

    assets
        .par_iter()
        .filter_map(|asset| {
            let quarters = asset.rotation_quarters();
            // Already thumbnailed on a previous run and its size is known:
            // no need to decode the (possibly RAW) original again; only
            // fill in EXIF facts an older catalog didn't record.
            if asset.width > 0 && asset.height > 0 {
                if let Some(thumb) = cache.cached_rotated(&asset.path, quarters) {
                    let meta = if asset.meta.captured != 0 {
                        asset.meta.clone()
                    } else {
                        crate::metadata::read_photo_meta(&asset.path)
                    };
                    progress.fetch_add(1, Ordering::Relaxed);
                    // Both come from the thumbnail; decode it only if one
                    // is still missing.
                    let (phash, sharpness) = match (asset.phash, asset.sharpness) {
                        (phash, Some(sharpness)) if phash != 0 => (phash, Some(sharpness)),
                        (phash, None) if phash != 0 && !measure_sharpness => (phash, None),
                        (phash, known) => {
                            let (fresh_hash, fresh_sharpness) = thumbnail_facts(&thumb);
                            (
                                if phash != 0 { phash } else { fresh_hash },
                                known.or(fresh_sharpness.filter(|_| measure_sharpness)),
                            )
                        }
                    };
                    let content_hash = if asset.content_hash != 0 {
                        asset.content_hash
                    } else {
                        sampled_content_hash(&asset.path)
                    };
                    return Some(ThumbInfo {
                        id: asset.id,
                        thumbnail: thumb,
                        width: asset.width,
                        height: asset.height,
                        orientation: asset.orientation,
                        meta,
                        content_hash,
                        phash,
                        sharpness,
                    });
                }
            }
            let result = cache.ensure_rotated(&asset.path, quarters).ok().map(
                |(thumbnail, width, height)| {
                    let (phash, sharpness) = thumbnail_facts(&thumbnail);
                    let sharpness = sharpness.filter(|_| measure_sharpness).or(asset.sharpness);
                    ThumbInfo {
                        id: asset.id,
                        thumbnail,
                        width,
                        height,
                        orientation: crate::decode::exif_orientation(&asset.path),
                        meta: crate::metadata::read_photo_meta(&asset.path),
                        content_hash: sampled_content_hash(&asset.path),
                        phash,
                        sharpness,
                    }
                },
            );
            progress.fetch_add(1, Ordering::Relaxed);
            result
        })
        .collect()
}

/// A 64-bit difference hash (dHash) of an image: resize to 9×8 greyscale and
/// record whether each pixel is brighter than its right-hand neighbour.
/// Stable across re-encodes, so near-duplicate photos get close hashes.
pub fn perceptual_hash(image: &DynamicImage) -> u64 {
    let grey = image.resize_exact(9, 8, FilterType::Triangle).to_luma8();
    let mut hash = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            let left = grey.get_pixel(x, y)[0];
            let right = grey.get_pixel(x + 1, y)[0];
            if left > right {
                hash |= 1 << (y * 8 + x);
            }
        }
    }
    hash
}

/// The difference hash and sharpness of a thumbnail, decoding it once.
fn thumbnail_facts(path: &Path) -> (u64, Option<f32>) {
    let Ok(image) = image::open(path) else {
        return (0, None);
    };
    let luma = image.to_luma8();
    let sharpness = rustyedit_core::culling::sharpness(
        luma.as_raw(),
        luma.width() as usize,
        luma.height() as usize,
    );
    (perceptual_hash(&image), sharpness)
}

/// A cheap content fingerprint: file length plus a sample of bytes from the
/// start, middle and end. Fast even for huge RAW files, and decisive enough
/// to call two files identical in practice.
pub fn sampled_content_hash(path: &Path) -> u64 {
    const SAMPLE: u64 = 64 * 1024;
    let Ok(mut file) = std::fs::File::open(path) else {
        return 0;
    };
    let Ok(metadata) = file.metadata() else {
        return 0;
    };
    let len = metadata.len();
    let mut hasher = DefaultHasher::new();
    len.hash(&mut hasher);
    let offsets = [0u64, len / 2, len.saturating_sub(SAMPLE)];
    let mut buf = vec![0u8; SAMPLE as usize];
    for offset in offsets {
        if file.seek(SeekFrom::Start(offset)).is_err() {
            break;
        }
        if let Ok(read) = file.read(&mut buf) {
            buf[..read].hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// What importing one photo produced.
fn cache_base() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("rustyedit")
}

fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                dir_size(&path)
            } else {
                entry.metadata().map(|m| m.len()).unwrap_or(0)
            }
        })
        .sum()
}

/// Total bytes currently used by generated thumbnails and live previews.
pub fn cache_size() -> u64 {
    dir_size(&cache_base())
}

/// Deletes every generated thumbnail, loupe/full preview and live develop
/// preview. The next catalog load regenerates what it needs. Returns the
/// number of bytes freed.
pub fn clear_cache() -> u64 {
    let base = cache_base();
    let freed = dir_size(&base);
    for name in ["thumbnails", "live_preview"] {
        let _ = std::fs::remove_dir_all(base.join(name));
    }
    freed
}

/// What importing one photo produced.
pub struct ThumbInfo {
    pub id: u64,
    pub thumbnail: PathBuf,
    pub width: u32,
    pub height: u32,
    pub orientation: u8,
    pub meta: rustyedit_core::PhotoMeta,
    pub content_hash: u64,
    pub phash: u64,
    /// See `rustyedit_core::culling::sharpness`.
    pub sharpness: Option<f32>,
}
