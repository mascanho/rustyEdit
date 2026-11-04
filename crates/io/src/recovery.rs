//! Automatic catalog backups and crash detection: a rolling set of
//! timestamped backups kept without the user asking, and a marker that
//! notices when the app didn't shut down cleanly last time.
//!
//! Every normal save already writes its sidecar atomically (a temp file,
//! then a rename), so a crash mid-write can't corrupt it, and edits reach
//! disk within under a second of being made (`CATALOG_SAVE_DEBOUNCE` in the
//! app crate). These automatic backups are for the rarer need: going back
//! further than that, whether because of a crash or just wanting an
//! earlier point in time than Undo's history reaches.

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rustyedit_core::CatalogDef;

use crate::maintenance::{build_backup, read_backup, write_backup, CatalogBackup};

/// Automatic backups kept per catalog; older ones are pruned as new ones
/// are made.
const KEEP: usize = 8;

fn root() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rustyedit").join("backups"))
}

fn catalog_dir(id: &str) -> Option<PathBuf> {
    root().map(|d| d.join(id))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// One automatic backup: when it was made, and where.
#[derive(Debug, Clone)]
pub struct AutoBackup {
    pub path: PathBuf,
    pub created_at: u64,
}

/// Every automatic backup of catalog `id`, newest first.
pub fn list_auto_backups(id: &str) -> Vec<AutoBackup> {
    let Some(dir) = catalog_dir(id) else {
        return Vec::new();
    };
    let mut backups: Vec<AutoBackup> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "json"))
        .filter_map(|entry| {
            let stem = entry.path().file_stem()?.to_str()?.to_string();
            let created_at: u64 = stem.parse().ok()?;
            Some(AutoBackup {
                path: entry.path(),
                created_at,
            })
        })
        .collect();
    backups.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    backups
}

/// The most recent automatic backup of catalog `id`, if there is one.
pub fn latest_auto_backup(id: &str) -> Option<AutoBackup> {
    list_auto_backups(id).into_iter().next()
}

/// Writes a fresh automatic backup of `def` (and the sidecars its folders
/// hold), then prunes older ones beyond [`KEEP`]. Cheap compared to a photo
/// catalog's thumbnails (it holds no images), so safe to call often;
/// nothing is ever overwritten, so a mistake made since the last one stays
/// recoverable from an earlier point.
pub fn write_auto_backup(def: &CatalogDef) -> anyhow::Result<PathBuf> {
    let dir = catalog_dir(&def.id).ok_or_else(|| anyhow::anyhow!("no config directory"))?;
    let backup = build_backup(def)?;
    let path = dir.join(format!("{}.json", backup.created_at));
    write_backup(&backup, &path)?;
    prune(&dir);
    Ok(path)
}

fn prune(dir: &Path) {
    let mut files: Vec<(PathBuf, u64)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "json"))
        .filter_map(|entry| {
            let stem = entry.path().file_stem()?.to_str()?.to_string();
            let created_at: u64 = stem.parse().ok()?;
            Some((entry.path(), created_at))
        })
        .collect();
    files.sort_by_key(|(_, created_at)| std::cmp::Reverse(*created_at));
    for (path, _) in files.into_iter().skip(KEEP) {
        let _ = std::fs::remove_file(path);
    }
}

/// Reads an automatic backup back in, to restore it.
pub fn read_auto_backup(path: &Path) -> anyhow::Result<CatalogBackup> {
    read_backup(path)
}

/// How long ago `created_at` (a Unix timestamp) was, in a few words.
pub fn relative_time(created_at: u64) -> String {
    let elapsed = now_secs().saturating_sub(created_at);
    if elapsed < 60 {
        "moments ago".to_string()
    } else if elapsed < 3600 {
        let n = elapsed / 60;
        format!("{n} minute{} ago", if n == 1 { "" } else { "s" })
    } else if elapsed < 86_400 {
        let n = elapsed / 3600;
        format!("{n} hour{} ago", if n == 1 { "" } else { "s" })
    } else {
        let n = elapsed / 86_400;
        format!("{n} day{} ago", if n == 1 { "" } else { "s" })
    }
}

// --- Crash detection ---------------------------------------------------

fn session_marker() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rustyedit").join("session.lock"))
}

/// Call once, early at startup, before [`begin_session`]: whether the
/// previous session's marker is still there, meaning it never reached a
/// clean shutdown (a crash, a force quit, the machine losing power).
pub fn had_unclean_exit() -> bool {
    session_marker().is_some_and(|path| had_unclean_exit_at(&path))
}

fn had_unclean_exit_at(path: &Path) -> bool {
    path.exists()
}

/// Marks a session as running: call once at startup, after
/// [`had_unclean_exit`].
pub fn begin_session() {
    if let Some(path) = session_marker() {
        begin_session_at(&path);
    }
}

fn begin_session_at(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, std::process::id().to_string());
}

/// Marks a clean shutdown: call from the quit handler. The next launch's
/// [`had_unclean_exit`] then reports `false`.
pub fn end_session() {
    if let Some(path) = session_marker() {
        end_session_at(&path);
    }
}

fn end_session_at(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyedit_core::Asset;

    fn temp_config_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rustyedit-recovery-{name}-{}-{}",
            std::process::id(),
            now_secs()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// `write_auto_backup`/`list_auto_backups` work against any directory,
    /// so the tests don't touch the real config dir: they call the file
    /// operations directly with a temp root rather than through `root()`.
    fn write_at(dir: &Path, def: &CatalogDef, created_at: u64) -> PathBuf {
        let backup = build_backup(def).unwrap();
        let backup = CatalogBackup {
            created_at,
            ..backup
        };
        let path = dir.join(format!("{created_at}.json"));
        write_backup(&backup, &path).unwrap();
        path
    }

    #[test]
    fn backups_list_newest_first_and_prune_keeps_the_most_recent() {
        let dir = temp_config_dir("prune");
        let catalog_dir = dir.join("photos");
        std::fs::create_dir_all(&catalog_dir).unwrap();
        let catalog = rustyedit_core::Catalog::new(
            catalog_dir.clone(),
            vec![Asset::new(0, catalog_dir.join("a.jpg"))],
        );
        crate::catalog_store::save(&catalog).unwrap();
        let mut def = CatalogDef::new("test", "Test");
        def.add_folder(&catalog_dir);

        for t in 0..(KEEP as u64 + 3) {
            write_at(&dir, &def, 1_000 + t);
        }
        let mut files: Vec<(PathBuf, u64)> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| {
                let stem = e.path().file_stem()?.to_str()?.to_string();
                Some((e.path(), stem.parse().ok()?))
            })
            .collect();
        files.sort_by_key(|(_, t)| std::cmp::Reverse(*t));
        prune(&dir);
        let after: Vec<u64> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| e.path().file_stem()?.to_str()?.parse().ok())
            .collect();
        assert_eq!(after.len(), KEEP);
        // The newest KEEP survive.
        let mut expected: Vec<u64> = files.iter().take(KEEP).map(|(_, t)| *t).collect();
        expected.sort_unstable();
        let mut got = after;
        got.sort_unstable();
        assert_eq!(got, expected);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_written_backup_reads_back_with_its_photo() {
        let dir = temp_config_dir("roundtrip");
        let catalog_dir = dir.join("photos");
        std::fs::create_dir_all(&catalog_dir).unwrap();
        let mut catalog = rustyedit_core::Catalog::new(
            catalog_dir.clone(),
            vec![Asset::new(0, catalog_dir.join("a.jpg"))],
        );
        catalog.assets[0].caption = "hello".into();
        crate::catalog_store::save(&catalog).unwrap();
        let mut def = CatalogDef::new("test", "Test");
        def.add_folder(&catalog_dir);

        let path = write_at(&dir, &def, 5_000);
        let read = read_auto_backup(&path).unwrap();
        assert_eq!(read.sidecars[0].catalog.assets[0].caption, "hello");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn relative_time_reads_naturally() {
        let now = now_secs();
        assert_eq!(relative_time(now), "moments ago");
        assert_eq!(relative_time(now - 125), "2 minutes ago");
        assert_eq!(relative_time(now - 3_661), "1 hour ago");
        assert_eq!(relative_time(now - 90_000), "1 day ago");
    }

    #[test]
    fn session_marker_round_trips() {
        let dir = temp_config_dir("session");
        let marker = dir.join("session.lock");
        assert!(!had_unclean_exit_at(&marker));
        begin_session_at(&marker);
        assert!(had_unclean_exit_at(&marker));
        end_session_at(&marker);
        assert!(!had_unclean_exit_at(&marker));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
