use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::DevelopSettings;

/// Entries kept per photo; the oldest are dropped past this.
const MAX_ENTRIES: usize = 200;
/// Consecutive changes to the same thing within this many seconds (a slider
/// drag, a burst of star clicks) are merged into one entry.
const COALESCE_SECS: u64 = 3;
/// Entries that keep their full settings (to step back to); older ones keep
/// just their label and values. Each snapshot is several KB, and the log is
/// saved with the catalog, so keeping them all made saves large.
const MAX_SNAPSHOTS: usize = 40;

/// One recorded change to a photo: what changed, and its value before and
/// after.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Unix seconds.
    pub at: u64,
    pub label: String,
    pub from: String,
    pub to: String,
    /// The photo's Develop settings right after this change, for edits
    /// that touch them, so the History panel can step back to it. `None`
    /// for flags, ratings, labels and entries saved before this existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub develop: Option<DevelopSettings>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Appends a change to `log`, merging it into the previous entry when it's
/// the same kind of change made moments ago, and dropping the entry
/// altogether if the merged result is back where it started.
pub(crate) fn record(
    log: &mut Vec<HistoryEntry>,
    label: &str,
    from: String,
    to: String,
    develop: Option<DevelopSettings>,
) {
    if from == to {
        return;
    }
    let at = now();
    if let Some(last) = log.last_mut() {
        if last.label == label && at.saturating_sub(last.at) <= COALESCE_SECS {
            last.to = to;
            last.at = at;
            last.develop = develop;
            if last.from == last.to {
                log.pop();
            }
            return;
        }
    }
    log.push(HistoryEntry {
        at,
        label: label.to_string(),
        from,
        to,
        develop,
    });
    if log.len() > MAX_ENTRIES {
        log.remove(0);
    }
    // Only the newest few keep their settings.
    let mut kept = 0;
    for entry in log.iter_mut().rev() {
        if entry.develop.is_some() {
            kept += 1;
            if kept > MAX_SNAPSHOTS {
                entry.develop = None;
            }
        }
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    #[test]
    fn only_the_newest_entries_keep_their_settings() {
        let mut log = Vec::new();
        for i in 0..100 {
            let settings = DevelopSettings { exposure: i as f32 + 1.0, ..Default::default() };
            record(&mut log, &format!("Edit {i}"), "a".into(), "b".into(), Some(settings));
        }
        assert_eq!(log.len(), 100);
        assert_eq!(log.iter().filter(|e| e.develop.is_some()).count(), MAX_SNAPSHOTS);
        // The newest are the ones that kept theirs.
        assert!(log.last().unwrap().develop.is_some());
        assert!(log[0].develop.is_none());
        assert_eq!(log[0].label, "Edit 0");
        let size = |e: &HistoryEntry| serde_json::to_string(e).unwrap().len();
        eprintln!("entry with settings: {} bytes, without: {}", size(&log[99]), size(&log[0]));
        assert!(size(&log[0]) * 10 < size(&log[99]));
    }
}
