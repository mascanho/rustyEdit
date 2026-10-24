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
    busy: bool,
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

struct Job {
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
        for &ix in indices {
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
                if fresh || !self.edited_thumbs.pending.insert((asset.id, s)) {
                    continue;
                }
                self.edited_thumbs.queue.push(Job {
                    id: asset.id,
                    stamp: s,
                    thumbnail: thumbnail.clone(),
                    settings,
                    frame,
                });
                added = true;
            }
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
        self.edited_thumbs
            .queue
            .retain(|j| !(j.id == job.id && j.frame.is_some() == job.frame.is_some()));
        self.edited_thumbs.busy = true;

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
            Some((job.id, job.stamp, job.frame.is_some(), path))
        });
        cx.spawn(async move |this, cx| {
            let result = render.await;
            this.update(cx, |this, cx| {
                let thumbs = &mut this.edited_thumbs;
                thumbs.busy = false;
                if let Some((id, s, framed, path)) = result {
                    thumbs.pending.remove(&(id, s));
                    let map = if framed { &mut thumbs.framed } else { &mut thumbs.done };
                    if let Some((_, old)) = map.insert(id, (s, path)) {
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

#[cfg(test)]
mod tests {
    use super::*;

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
