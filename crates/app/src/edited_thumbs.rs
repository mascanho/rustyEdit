//! Grid thumbnails that show each photo's edits. The cached thumbnails are
//! made from the originals, so a photo with a preset or other adjustments
//! looked untouched in the grid. Edited photos get a second, small render of
//! their thumbnail through the develop pipeline, keyed by a fingerprint of
//! their settings so it is redone whenever the edits change.

use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    path::PathBuf,
};

use gpui::Context;
use rustyedit_core::{Asset, DevelopSettings};

use crate::workspace::Workspace;

/// Rendered edit thumbnails, and the ones being made.
#[derive(Default)]
pub(crate) struct EditedThumbs {
    /// Asset id -> (settings fingerprint, rendered file).
    done: HashMap<u64, (u64, PathBuf)>,
    /// (asset id, fingerprint) jobs queued or running.
    pending: HashSet<(u64, u64)>,
    queue: Vec<Job>,
    busy: bool,
}

struct Job {
    id: u64,
    stamp: u64,
    thumbnail: PathBuf,
    settings: DevelopSettings,
}

/// A fingerprint of the edits; equal settings give equal fingerprints.
fn stamp(settings: &DevelopSettings) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{settings:?}").hash(&mut hasher);
    hasher.finish()
}

fn cache_dir() -> PathBuf {
    std::env::temp_dir().join("rustyedit-edited-thumbs")
}

/// The settings to render on the cached thumbnail, which already has the
/// photo's rotation baked in. Spots and gradients are positioned on the
/// unrotated original, so they're left out for rotated photos.
fn thumbnail_settings(asset: &Asset) -> DevelopSettings {
    let mut settings = asset.develop;
    if asset.rotation_quarters() != 0 {
        settings.spots = Default::default();
        settings.gradients = Default::default();
    }
    settings.rotation_degrees = 0.0;
    settings
}

impl Workspace {
    /// The thumbnail the grid should show for `asset`: its edited render
    /// when one is ready for its current settings, else the plain one.
    pub(crate) fn display_thumbnail(&self, asset: &Asset) -> Option<PathBuf> {
        if asset.develop.is_edited() {
            if let Some((done_stamp, path)) = self.edited_thumbs.done.get(&asset.id) {
                if *done_stamp == stamp(&thumbnail_settings(asset)) {
                    return Some(path.clone());
                }
            }
        }
        asset.thumbnail_path.clone()
    }

    /// Queues edit thumbnails for the photos at `indices` (the visible grid
    /// cards) that don't have an up-to-date one.
    pub(crate) fn ensure_edited_thumbnails(&mut self, indices: &[usize], cx: &mut Context<Self>) {
        let mut added = false;
        for &ix in indices {
            let Some(asset) = self.catalog.assets.get(ix) else {
                continue;
            };
            if !asset.develop.is_edited() || asset.missing {
                continue;
            }
            let Some(thumbnail) = asset.thumbnail_path.clone() else {
                continue;
            };
            let settings = thumbnail_settings(asset);
            let s = stamp(&settings);
            let fresh = self
                .edited_thumbs
                .done
                .get(&asset.id)
                .is_some_and(|(done, _)| *done == s);
            if fresh || !self.edited_thumbs.pending.insert((asset.id, s)) {
                continue;
            }
            self.edited_thumbs.queue.push(Job {
                id: asset.id,
                stamp: s,
                thumbnail,
                settings,
            });
            added = true;
        }
        if added {
            self.run_edited_thumbnail_queue(cx);
        }
    }

    /// Renders queued thumbnails one at a time on a background thread.
    fn run_edited_thumbnail_queue(&mut self, cx: &mut Context<Self>) {
        if self.edited_thumbs.busy {
            return;
        }
        // Newest first: the most recent edit is what the user is looking at.
        let Some(job) = self.edited_thumbs.queue.pop() else {
            return;
        };
        // Drop older jobs for the same photo; only its latest edit matters.
        self.edited_thumbs.queue.retain(|j| j.id != job.id);
        self.edited_thumbs.busy = true;

        let render = cx.background_executor().spawn(async move {
            let image = image::open(&job.thumbnail).ok()?;
            // Cached thumbnails are sRGB JPEGs.
            let source = rustyedit_processing::DevelopSource::from_image(&image, None);
            let rendered = rustyedit_processing::apply(&source, &job.settings);
            let dir = cache_dir();
            std::fs::create_dir_all(&dir).ok()?;
            let path = dir.join(format!("{}-{:016x}.jpg", job.id, job.stamp));
            rendered.save(&path).ok()?;
            Some((job.id, job.stamp, path))
        });
        cx.spawn(async move |this, cx| {
            let result = render.await;
            this.update(cx, |this, cx| {
                let thumbs = &mut this.edited_thumbs;
                thumbs.busy = false;
                if let Some((id, s, path)) = result {
                    thumbs.pending.retain(|(pid, _)| *pid != id);
                    if let Some((_, old)) = thumbs.done.insert(id, (s, path)) {
                        let _ = std::fs::remove_file(old);
                    }
                }
                this.run_edited_thumbnail_queue(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
