//! Snapshots: named, saved states of one photo's Develop settings, so
//! alternative edits can be kept and returned to.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::DevelopSettings;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub name: String,
    /// Unix seconds.
    pub at: u64,
    pub develop: DevelopSettings,
}

impl Snapshot {
    pub fn new(name: impl Into<String>, develop: DevelopSettings) -> Self {
        Self {
            name: name.into(),
            at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            develop,
        }
    }
}

/// "Snapshot N" for the first N not already taken in `existing`.
pub fn next_snapshot_name(existing: &[Snapshot]) -> String {
    (1..)
        .map(|n| format!("Snapshot {n}"))
        .find(|name| !existing.iter().any(|s| &s.name == name))
        .expect("an unused name always exists")
}
