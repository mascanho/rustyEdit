//! Grid thumbnails that show each photo's edits. The cached thumbnails are
//! made from the originals, so a photo with a preset or other adjustments
//! looked untouched in the grid. Edited photos get a second, small render of
//! their thumbnail through the develop pipeline, keyed by a fingerprint of
//! their settings so it is redone whenever the edits change.

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use gpui::Context;
use rustyedit_core::{Asset, DevelopSettings, FrameSettings};

use crate::workspace::Workspace;

/// Rendered edit thumbnails, and the ones being made.
#[derive(Default)]
pub(crate) struct EditedThumbs {
    /// Asset id -> (settings fingerprint, rendered file).
    done: HashMap<u64, (u64, PathBuf)>,
    /// The same with the photo's frame drawn around it, for the Export
    /// tiles (the grid and filmstrip show photos unframed).
    framed: HashMap<u64, (u64, PathBuf)>,
    /// (asset id, fingerprint) jobs queued or running.
    pending: HashSet<(u64, u64)>,
    queue: Vec<Job>,
    /// Jobs that failed (an unreadable thumbnail): not retried every frame.
    failed: HashSet<(u64, u64)>,
    /// How many renders are running right now.
    running: usize,
    /// Counts requests, so newer ones (what's on screen now) go first.
    tick: u64,
    /// What each photo's settings were when they were last given a version
    /// number (see [`EditedThumbs::version`]).
    versions: RefCell<HashMap<(u64, bool), Seen>>,
    next_version: Cell<u64>,
}

/// The settings a version number was given to.
struct Seen {
    settings: DevelopSettings,
    frame: Option<FrameSettings>,
    version: u64,
}

impl EditedThumbs {
    /// A number that stands for `settings` (and `frame`) of photo `id`: the
    /// same while they stay the same, a new one once they change. Drawing
    /// asks this for every card on screen every frame, so it compares the
    /// settings with the last ones seen instead of fingerprinting them.
    fn version(
        &self,
        id: u64,
        framed: bool,
        settings: &DevelopSettings,
        frame: Option<&FrameSettings>,
    ) -> u64 {
        let mut seen = self.versions.borrow_mut();
        if let Some(entry) = seen.get(&(id, framed)) {
            if entry.settings == *settings && entry.frame.as_ref() == frame {
                return entry.version;
            }
        }
        let version = self.next_version.get() + 1;
        self.next_version.set(version);
        seen.insert(
            (id, framed),
            Seen {
                settings: *settings,
                frame: frame.copied(),
                version,
            },
        );
        version
    }
}

/// How many thumbnails are rendered at once.
const PARALLEL: usize = 3;

struct Job {
    /// Higher goes first: the latest request, then the earliest on screen.
    priority: u64,
    id: u64,
    stamp: u64,
    thumbnail: PathBuf,
    settings: DevelopSettings,
    /// Draw this frame around the render.
    frame: Option<FrameSettings>,
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
    // Frames are drawn separately (and only for the Export tiles).
    settings.frame = FrameSettings::default();
    settings
}

/// The frame to draw around `asset`'s thumbnail, if it has an active one.
fn thumbnail_frame(asset: &Asset) -> Option<FrameSettings> {
    let frame = asset.develop.effective().frame;
    frame.is_active().then_some(frame)
}

impl Workspace {
    /// The thumbnail the grid should show for `asset`: its edited render
    /// when one is ready for its current settings, else the plain one.
    pub(crate) fn display_thumbnail(&self, asset: &Asset) -> Option<PathBuf> {
        let settings = thumbnail_settings(asset);
        if settings.is_edited() {
            if let Some((done_stamp, path)) = self.edited_thumbs.done.get(&asset.id) {
                if *done_stamp == self.edited_thumbs.version(asset.id, false, &settings, None) {
                    return Some(path.clone());
                }
            }
        }
        asset.thumbnail_path.clone()
    }

    /// The thumbnail the Export tiles show: the edited render, with the
    /// photo's frame drawn around it once that's ready.
    pub(crate) fn export_thumbnail(&self, asset: &Asset) -> Option<PathBuf> {
        if let Some(frame) = thumbnail_frame(asset) {
            let wanted = self.edited_thumbs.version(
                asset.id,
                true,
                &thumbnail_settings(asset),
                Some(&frame),
            );
            if let Some((done_stamp, path)) = self.edited_thumbs.framed.get(&asset.id) {
                if *done_stamp == wanted {
                    return Some(path.clone());
                }
            }
        }
        self.display_thumbnail(asset)
    }

    /// Queues the edited and framed thumbnails the Export tiles need.
    pub(crate) fn ensure_export_thumbnails(&mut self, indices: &[usize], cx: &mut Context<Self>) {
        self.queue_thumbnails(indices, true, cx);
    }

    /// Queues edit thumbnails for the photos at `indices` (the visible grid
    /// cards or filmstrip frames) that don't have an up-to-date one.
    pub(crate) fn ensure_edited_thumbnails(&mut self, indices: &[usize], cx: &mut Context<Self>) {
        self.queue_thumbnails(indices, false, cx);
    }

    fn queue_thumbnails(&mut self, indices: &[usize], with_frames: bool, cx: &mut Context<Self>) {
        let mut added = false;
        self.edited_thumbs.tick += 1;
        let tick = self.edited_thumbs.tick;
        for (position, &ix) in indices.iter().enumerate() {
            // This request's photos outrank older requests', and within it
            // the first on screen come first.
            let priority = tick * 1_000_000 + (999_999 - position.min(999_999) as u64);
            let Some(asset) = self.catalog.assets.get(ix) else {
                continue;
            };
            if asset.missing {
                continue;
            }
            let Some(thumbnail) = asset.thumbnail_path.clone() else {
                continue;
            };
            let settings = thumbnail_settings(asset);
            let frame = with_frames.then(|| thumbnail_frame(asset)).flatten();
            if !settings.is_edited() && frame.is_none() {
                continue;
            }
            // The plain edit, then (for the Export tiles) the framed one.
            let mut wanted: Vec<(u64, Option<FrameSettings>)> = Vec::new();
            if settings.is_edited() {
                wanted.push((self.edited_thumbs.version(asset.id, false, &settings, None), None));
            }
            if let Some(frame) = frame {
                let s = self.edited_thumbs.version(asset.id, true, &settings, Some(&frame));
                wanted.push((s, Some(frame)));
            }
            for (s, frame) in wanted {
                let done = if frame.is_some() {
                    &self.edited_thumbs.framed
                } else {
                    &self.edited_thumbs.done
                };
                let fresh = done.get(&asset.id).is_some_and(|(done, _)| *done == s);
                if fresh || self.edited_thumbs.failed.contains(&(asset.id, s)) {
                    continue;
                }
                if !self.edited_thumbs.pending.insert((asset.id, s)) {
                    // Already queued: it's on screen again, so move it up.
                    if let Some(job) = self
                        .edited_thumbs
                        .queue
                        .iter_mut()
                        .find(|j| j.id == asset.id && j.stamp == s)
                    {
                        job.priority = priority;
                    }
                    continue;
                }
                self.edited_thumbs.queue.push(Job {
                    priority,
                    id: asset.id,
                    stamp: s,
                    thumbnail: thumbnail.clone(),
                    settings,
                    frame,
                });
                added = true;
            }
        }
        // Photos scrolled well out of view aren't worth rendering any more;
        // they're asked for again if they come back.
        let thumbs = &mut self.edited_thumbs;
        let pending = &mut thumbs.pending;
        thumbs.queue.retain(|job| {
            let keep = job.priority / 1_000_000 + 120 >= tick;
            if !keep {
                pending.remove(&(job.id, job.stamp));
            }
            keep
        });
        if added {
            self.run_edited_thumbnail_queue(cx);
        }
    }

    /// Renders queued thumbnails, a few at a time on background threads,
    /// the most wanted first.
    fn run_edited_thumbnail_queue(&mut self, cx: &mut Context<Self>) {
        while self.edited_thumbs.running < PARALLEL {
            let thumbs = &mut self.edited_thumbs;
            let Some((job, stale)) = take_best(&mut thumbs.queue) else {
                return;
            };
            for key in stale {
                thumbs.pending.remove(&key);
            }
            thumbs.running += 1;
            self.spawn_edited_thumbnail(job, cx);
        }
    }

    fn spawn_edited_thumbnail(&mut self, job: Job, cx: &mut Context<Self>) {
        let (id, stamp, framed) = (job.id, job.stamp, job.frame.is_some());
        let render = cx.background_executor().spawn(async move {
            let image = image::open(&job.thumbnail).ok()?;
            // Cached thumbnails are sRGB JPEGs.
            let source = rustyedit_processing::DevelopSource::from_image(&image, None);
            let mut rendered = rustyedit_processing::apply(&source, &job.settings);
            if let Some(frame) = &job.frame {
                rendered = rustyedit_processing::frame_rgb8(&rendered, frame);
            }
            let dir = cache_dir();
            std::fs::create_dir_all(&dir).ok()?;
            let tag = if job.frame.is_some() { "-framed" } else { "" };
            let path = dir.join(format!("{}-{:016x}{tag}.jpg", job.id, job.stamp));
            rendered.save(&path).ok()?;
            Some(path)
        });
        cx.spawn(async move |this, cx| {
            let result = render.await;
            this.update(cx, |this, cx| {
                let thumbs = &mut this.edited_thumbs;
                thumbs.running = thumbs.running.saturating_sub(1);
                // Done or failed, it's no longer in flight (a failure would
                // otherwise block that photo from ever being tried again).
                thumbs.pending.remove(&(id, stamp));
                if result.is_none() {
                    thumbs.failed.insert((id, stamp));
                }
                if let Some(path) = result {
                    let map = if framed { &mut thumbs.framed } else { &mut thumbs.done };
                    if let Some((_, old)) = map.insert(id, (stamp, path)) {
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

/// Takes the most wanted job off `queue`, along with the keys of the
/// older jobs for the same photo, which it makes pointless and drops.
fn take_best(queue: &mut Vec<Job>) -> Option<(Job, Vec<(u64, u64)>)> {
    let best = queue
        .iter()
        .enumerate()
        .max_by_key(|(_, job)| job.priority)
        .map(|(i, _)| i)?;
    let job = queue.swap_remove(best);
    let same = |j: &Job| j.id == job.id && j.frame.is_some() == job.frame.is_some();
    let stale = queue.iter().filter(|j| same(j)).map(|j| (j.id, j.stamp)).collect();
    queue.retain(|j| !same(j));
    Some((job, stale))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(priority: u64, id: u64, stamp: u64) -> Job {
        Job {
            priority,
            id,
            stamp,
            thumbnail: PathBuf::new(),
            settings: DevelopSettings::default(),
            frame: None,
        }
    }

    #[test]
    fn the_most_recently_wanted_photo_renders_first() {
        let mut queue = vec![job(1_000_900, 1, 1), job(2_000_999, 2, 1), job(2_000_998, 3, 1)];
        // The newest request, earliest on screen first.
        assert_eq!(take_best(&mut queue).unwrap().0.id, 2);
        assert_eq!(take_best(&mut queue).unwrap().0.id, 3);
        assert_eq!(take_best(&mut queue).unwrap().0.id, 1);
        assert!(take_best(&mut queue).is_none());
    }

    #[test]
    fn an_older_job_for_the_same_photo_is_dropped() {
        let mut queue = vec![job(1, 7, 10), job(5, 7, 11), job(3, 8, 1)];
        let (best, stale) = take_best(&mut queue).unwrap();
        assert_eq!((best.id, best.stamp), (7, 11));
        assert_eq!(stale, vec![(7, 10)]);
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn a_version_changes_only_when_the_settings_do() {
        let thumbs = EditedThumbs::default();
        let mut settings = DevelopSettings::default();
        settings.exposure = 10.0;
        let a = thumbs.version(1, false, &settings, None);
        assert_eq!(a, thumbs.version(1, false, &settings, None));
        settings.exposure = 20.0;
        let b = thumbs.version(1, false, &settings, None);
        assert_ne!(a, b);
        // Another photo, and the framed variant, have their own.
        assert_ne!(b, thumbs.version(2, false, &settings, None));
        let frame = rustyedit_core::FrameStyle::Matte.defaults();
        let framed = thumbs.version(1, true, &settings, Some(&frame));
        assert_eq!(framed, thumbs.version(1, true, &settings, Some(&frame)));
        let mut other = frame;
        other.size = 50.0;
        assert_ne!(framed, thumbs.version(1, true, &settings, Some(&other)));
    }

    /// The point of versions: asking is far cheaper than fingerprinting.
    #[test]
    #[ignore = "timing: cargo test -p rustyedit --release -- --ignored --nocapture"]
    fn asking_for_a_version_is_cheap() {
        let thumbs = EditedThumbs::default();
        let mut settings = DevelopSettings::default();
        settings.exposure = 10.0;
        thumbs.version(1, false, &settings, None);
        let t = std::time::Instant::now();
        for _ in 0..10_000 {
            std::hint::black_box(thumbs.version(1, false, &settings, None));
        }
        let versions = t.elapsed();
        let t = std::time::Instant::now();
        for _ in 0..10_000 {
            std::hint::black_box(format!("{settings:?}").len());
        }
        eprintln!("version: {versions:?}, formatting: {:?} (10k calls)", t.elapsed());
    }
}
