use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static PREVIEW_COUNTER: AtomicU64 = AtomicU64::new(0);

fn live_preview_dir() -> PathBuf {
    let base = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
    let dir = base.join("rustyedit").join("live_preview");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// A fresh, unique path to render the next Develop-panel live preview into.
/// Callers write a JPEG here and pass the path to GPUI's `img()`; using a
/// unique path per render sidesteps GPUI's own image cache ever showing a
/// stale frame for a previous set of adjustments.
pub fn next_live_preview_path() -> PathBuf {
    let n = PREVIEW_COUNTER.fetch_add(1, Ordering::Relaxed);
    live_preview_dir().join(format!("preview-{n}.jpg"))
}

/// Best-effort deletion of a previous live preview file; failures are
/// ignored since this is purely disk-cache hygiene.
pub fn cleanup_live_preview(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
}
