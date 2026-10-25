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

/// Dot-files and dot-folders: macOS's `._IMG.RAF` companions (which carry a
/// photo's extension but hold no picture), `.Trashes`, `.Spotlight-V100`.
pub(crate) fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

/// Recursively scans `folder` (one level deep by default is too limiting for
/// real shoots, so this walks the whole tree) and returns an `Asset` for
/// every supported image file found, in a stable sorted order.
pub fn scan_folder(folder: &Path) -> Vec<Asset> {
    let mut paths: Vec<_> = WalkDir::new(folder)
        .into_iter()
        .filter_entry(|entry| entry.depth() == 0 || !is_hidden(entry.path()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_files_and_folders_are_not_photos() {
        let dir = std::env::temp_dir().join(format!("rustyedit-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".Trashes")).unwrap();
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for f in ["a.jpg", "._a.jpg", "sub/b.png", "sub/._b.png", ".Trashes/c.jpg"] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        let found: Vec<_> = scan_folder(&dir)
            .into_iter()
            .map(|a| a.path.strip_prefix(&dir).unwrap().to_path_buf())
            .collect();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(found, vec![Path::new("a.jpg"), Path::new("sub/b.png")]);
    }
}
