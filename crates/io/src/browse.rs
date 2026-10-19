use std::path::{Path, PathBuf};

use crate::scan::has_supported_extension;

/// One child of a directory, as shown in the Navigator's folder tree.
#[derive(Debug, Clone)]
pub struct FsEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    /// A file the app can open (see `scan::SUPPORTED_EXTENSIONS`).
    pub is_image: bool,
    pub is_hidden: bool,
}

/// Lists the folders and supported image files directly inside `dir`
/// (non-recursive, so it stays fast for lazily expanding a tree). Other
/// file types are skipped. Folders come first, then files, each sorted
/// case-insensitively. Symlinks are followed to decide whether an entry is
/// a folder, but never traversed here.
pub fn list_directory(dir: &Path) -> std::io::Result<Vec<FsEntry>> {
    let mut entries: Vec<FsEntry> = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = std::fs::metadata(&path).map(|m| m.is_dir()).ok()?;
            let is_image = !is_dir && has_supported_extension(&path);
            if !is_dir && !is_image {
                return None;
            }
            Some(FsEntry {
                is_hidden: name.starts_with('.'),
                path,
                name,
                is_dir,
                is_image,
            })
        })
        .collect();

    entries.sort_by_cached_key(|e| (!e.is_dir, e.name.to_lowercase()));
    Ok(entries)
}

/// Starting points for browsing: the home folder, the usual photo
/// locations inside it, and every mounted volume (external drives, card
/// readers). Only locations that exist are returned.
pub fn browse_roots() -> Vec<(String, PathBuf)> {
    let mut roots = Vec::new();
    if let Some(home) = dirs::home_dir() {
        roots.push(("Home".to_string(), home));
    }
    for (label, dir) in [
        ("Pictures", dirs::picture_dir()),
        ("Desktop", dirs::desktop_dir()),
        ("Downloads", dirs::download_dir()),
    ] {
        if let Some(dir) = dir {
            roots.push((label.to_string(), dir));
        }
    }
    if cfg!(target_os = "macos") {
        if let Ok(volumes) = list_directory(Path::new("/Volumes")) {
            roots.extend(
                volumes
                    .into_iter()
                    .filter(|v| v.is_dir && !v.is_hidden)
                    .map(|v| (v.name, v.path)),
            );
        }
    } else {
        roots.push(("Computer".to_string(), PathBuf::from("/")));
    }
    roots.retain(|(_, path)| path.is_dir());
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_folders_first_then_photos_and_skips_other_files() {
        let dir =
            std::env::temp_dir().join(format!("rustyedit-browse-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("zeta")).unwrap();
        std::fs::create_dir_all(dir.join("Alpha")).unwrap();
        std::fs::create_dir_all(dir.join(".cache")).unwrap();
        for file in ["b.JPG", "a.png", "notes.txt", ".hidden.jpg"] {
            std::fs::write(dir.join(file), b"").unwrap();
        }

        let entries = list_directory(&dir).unwrap();
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            [".cache", "Alpha", "zeta", ".hidden.jpg", "a.png", "b.JPG"]
        );
        assert!(entries[0].is_hidden && entries[0].is_dir);
        assert!(entries[5].is_image && !entries[5].is_dir);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
