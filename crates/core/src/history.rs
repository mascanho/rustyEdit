use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Entries kept per photo; the oldest are dropped past this.
const MAX_ENTRIES: usize = 200;
/// Consecutive changes to the same thing within this many seconds (a slider
/// drag, a burst of star clicks) are merged into one entry.
const COALESCE_SECS: u64 = 3;

/// One recorded change to a photo: what changed, and its value before and
/// after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Unix seconds.
    pub at: u64,
    pub label: String,
    pub from: String,
    pub to: String,
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
pub(crate) fn record(log: &mut Vec<HistoryEntry>, label: &str, from: String, to: String) {
    if from == to {
        return;
    }
    let at = now();
    if let Some(last) = log.last_mut() {
        if last.label == label && at.saturating_sub(last.at) <= COALESCE_SECS {
            last.to = to;
            last.at = at;
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
    });
    if log.len() > MAX_ENTRIES {
        log.remove(0);
    }
}
