use std::path::Path;

use rustyedit_core::Asset;
use walkdir::WalkDir;

const SUPPORTED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "tif", "tiff", "bmp"];

pub(crate) fn has_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let lower = ext.to_ascii_lowercase();
            SUPPORTED_EXTENSIONS.contains(&lower.as_str())
                || crate::decode::raw_extensions().contains(&lower.as_str())
        })
        .unwrap_or(false)
}

/// Recursively scans `folder` (one level deep by default is too limiting for
/// real shoots, so this walks the whole tree) and returns an `Asset` for
/// every supported image file found, in a stable sorted order.
pub fn scan_folder(folder: &Path) -> Vec<Asset> {
    let mut paths: Vec<_> = WalkDir::new(folder)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| has_supported_extension(path))
        .collect();

    paths.sort();

    paths
        .into_iter()
        .enumerate()
        .map(|(ix, path)| Asset::new(ix as u64, path))
        .collect()
}
