//! The sharp zoomed-in view: when Develop is zoomed past what the working
//! preview can show, the part of the photo on screen is rendered from the
//! full-size photo at screen resolution and laid over the (enlarged)
//! preview.
//!
//! Only what's visible is rendered, so it costs about as much as one
//! screenful of pixels whatever the photo's size. It waits until panning,
//! zooming or a slider drag pauses (`SETTLE`), and while it's out of date
//! the enlarged preview shows instead, so dragging stays as quick as at
//! Fit. It only runs where the GPU can do it (see
//! `rustyedit_processing::detail_is_fast`).
//!
//! Renders are tiles with a margin: each covers the screen plus a quarter
//! of it on every side, so panning a little (or nudging the zoom back) needs
//! no new render. The last few tiles are kept, so going back to where you
//! were is instant too.

use std::{sync::Arc, time::Duration};

use gpui::{Context, RenderImage, Task};
use rustyedit_core::DevelopSettings;

use crate::workspace::{Mode, Workspace};

/// How long the view has to stay put before the sharp render starts.
const SETTLE: Duration = Duration::from_millis(140);

/// Left, top, width, height as fractions of the cropped photo.
pub(crate) type Region = (f32, f32, f32, f32);

/// Extra photo around the screen that a tile covers, as a fraction of the
/// screen's size on each side (tried in turn until the tile fits `MAX_TILE`).
const MARGINS: [f32; 3] = [0.25, 0.12, 0.0];
/// The largest tile edge and pixel count rendered, to keep GPU memory in check.
const MAX_TILE_EDGE: u32 = 6144;
const MAX_TILE_PIXELS: u64 = 26_000_000;
/// Finished tiles kept for the photo being edited (the newest first).
const KEEP_TILES: usize = 3;

/// What a detail render is of: the photo, its settings, the region
/// (fractions of the cropped photo) and the pixel size it's delivered at.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct DetailKey {
    pub(crate) asset: u64,
    pub(crate) settings: DevelopSettings,
    pub(crate) region: Region,
    pub(crate) size: (u32, u32),
}

#[derive(Default)]
pub(crate) struct DetailView {
    /// Finished renders and what they are of, the newest first.
    tiles: Vec<(Arc<RenderImage>, DetailKey)>,
    /// What was last asked for (waiting to settle, or rendering): the tile,
    /// margin included.
    requested: Option<DetailKey>,
    busy: bool,
    /// The settle timer; replacing it cancels the previous one.
    timer: Option<Task<()>>,
}

impl DetailView {
    /// Takes every finished render out (to be retired), forgetting any
    /// request.
    pub(crate) fn take(&mut self) -> Vec<Arc<RenderImage>> {
        self.requested = None;
        self.timer = None;
        self.tiles.drain(..).map(|(image, _)| image).collect()
    }

    /// The newest tile that shows everything `wanted` needs, as sharp.
    fn covering(&self, wanted: &DetailKey) -> Option<usize> {
        self.tiles.iter().position(|(_, tile)| covers(tile, wanted))
    }

    /// Adds a finished tile; returns the ones it replaces or pushes out, to
    /// be retired.
    fn insert(&mut self, image: Arc<RenderImage>, key: DetailKey) -> Vec<Arc<RenderImage>> {
        let mut gone = Vec::new();
        let old = std::mem::take(&mut self.tiles);
        self.tiles.push((image, key));
        for (image, tile) in old {
            // Another photo or other edits: useless now. Already inside
            // the new tile: redundant.
            let same = tile.asset == key.asset && tile.settings == key.settings;
            if same && !covers(&key, &tile) && self.tiles.len() < KEEP_TILES {
                self.tiles.push((image, tile));
            } else {
                gone.push(image);
            }
        }
        gone
    }
}

/// Whether `tile` shows all of `wanted` at least as sharply: the same photo
/// and edits, the region inside it and no coarser (a hair of slack for
/// rounding).
fn covers(tile: &DetailKey, wanted: &DetailKey) -> bool {
    const EPS: f32 = 1e-4;
    let (tl, tt, tw, th) = tile.region;
    let (wl, wt, ww, wh) = wanted.region;
    let density = |k: &DetailKey| k.size.0 as f32 / k.region.2.max(1e-6);
    tile.asset == wanted.asset
        && tile.settings == wanted.settings
        && wl >= tl - EPS
        && wt >= tt - EPS
        && wl + ww <= tl + tw + EPS
        && wt + wh <= tt + th + EPS
        && density(tile) >= density(wanted) * 0.97
}

/// The tile to render for what's on screen: the screen's region plus the
/// largest margin of `MARGINS` that keeps it within the tile limits, at the
/// same pixel density.
fn with_margin(wanted: DetailKey) -> DetailKey {
    let (l, t, w, h) = wanted.region;
    let (sw, sh) = (wanted.size.0 as f32, wanted.size.1 as f32);
    for margin in MARGINS {
        let left = (l - margin * w).max(0.0);
        let top = (t - margin * h).max(0.0);
        let right = (l + w + margin * w).min(1.0);
        let bottom = (t + h + margin * h).min(1.0);
        let (nw, nh) = (right - left, bottom - top);
        let size = (
            (sw * nw / w).round().max(1.0) as u32,
            (sh * nh / h).round().max(1.0) as u32,
        );
        let fits = size.0 <= MAX_TILE_EDGE
            && size.1 <= MAX_TILE_EDGE
            && size.0 as u64 * size.1 as u64 <= MAX_TILE_PIXELS;
        if fits || margin == 0.0 {
            return DetailKey {
                region: (left, top, nw, nh),
                size: if margin == 0.0 { wanted.size } else { size },
                ..wanted
            };
        }
    }
    wanted
}

impl Workspace {
    /// The photo's size in full-resolution pixels, as cropped: what Develop's
    /// zoom percentages are of, so 100% is one photo pixel per screen pixel.
    pub(crate) fn develop_zoom_size(&self) -> Option<(f32, f32)> {
        let (w, h) = self.develop_image_size()?;
        let factor = match (&self.full_source, &self.source_image) {
            (Some(full), Some(preview)) if preview.width() > 0 => {
                (full.width() as f32 / preview.width() as f32).max(1.0)
            }
            // Still decoding: the preview will be the photo's long edge
            // brought down to the preview size.
            _ => self.catalog.selected_asset().map_or(1.0, |a| {
                let long = a.width.max(a.height) as f32;
                let max_edge = self.settings.develop.preview_quality.max_edge() as f32;
                (long / max_edge).max(1.0)
            }),
        };
        Some((w * factor, h * factor))
    }

    /// Whether the detail view can show now: zoomed in Develop on the
    /// plain photo (no comparison, crop canvas, clipping warnings, soft
    /// proof or preset preview, which only the full preview draws).
    fn detail_allowed(&self) -> bool {
        let zoomed = match self.mode {
            Mode::Develop => self.develop_ui.zoom.scale.is_some(),
            Mode::Library => self.loupe_zoom.is_some() && self.loupe_shows_edits(),
            Mode::Export | Mode::Zines => false,
        };
        zoomed
            && !self.ba_active()
            && !self.crop_in_view()
            && !self.histogram_ui.show_shadow_clipping
            && !self.histogram_ui.show_highlight_clipping
            && !self.develop_ui.proof.active()
            && self.preview_override.is_none()
            && self.live_preview.image().is_some()
            && self
                .full_source
                .as_ref()
                .is_some_and(rustyedit_processing::detail_is_fast)
    }

    /// The detail render the view needs right now, if any: none when the
    /// enlarged preview is already as sharp as the screen.
    fn wanted_detail(&self) -> Option<DetailKey> {
        if !self.detail_allowed() {
            return None;
        }
        let asset = self.catalog.selected_asset()?;
        if self.source_image_for != Some(asset.id) {
            return None;
        }
        let (region, (w, h), ratio) = self.detail_viewport()?;
        let (preview_w, _) = self.develop_image_size()?;
        let (full_w, full_h) = self.develop_zoom_size()?;
        // Screen pixels the region covers, and photo pixels it holds.
        let screen = (region.2 * w * ratio, region.3 * h * ratio);
        if screen.0 <= region.2 * preview_w * 1.05 {
            return None;
        }
        let size = (
            screen.0.min(region.2 * full_w).round().max(1.0) as u32,
            screen.1.min(region.3 * full_h).round().max(1.0) as u32,
        );
        Some(DetailKey {
            asset: asset.id,
            settings: asset.develop,
            region,
            size,
        })
    }

    /// Whether the Library loupe shows the selected photo with its edits
    /// (rendered) rather than its file.
    pub(crate) fn loupe_shows_edits(&self) -> bool {
        self.mode == Mode::Library
            && self.library_view == crate::workspace::LibraryView::Loupe
            && self.catalog.selected_asset().is_some_and(|a| {
                a.develop.is_edited() && self.source_image_for == Some(a.id)
            })
            && self.live_preview.image().is_some()
    }

    /// What's in view of the zoomed photo (fractions), the photo's size on
    /// screen (logical pixels) and the display's pixel ratio.
    fn detail_viewport(&self) -> Option<(Region, (f32, f32), f32)> {
        match self.mode {
            Mode::Develop => {
                let zoom = &self.develop_ui.zoom;
                Some((zoom.visible_region()?, zoom.zoomed_size.get(), zoom.pixel_ratio.get()))
            }
            Mode::Library => {
                let (w, h) = self.loupe_zoomed_size.get();
                let view = self.loupe_scroll.bounds().size;
                let (vw, vh) = (f32::from(view.width), f32::from(view.height));
                if w <= 0.0 || h <= 0.0 || vw <= 0.0 || vh <= 0.0 {
                    return None;
                }
                let offset = self.loupe_scroll.offset();
                let (fw, fh) = ((vw / w).min(1.0), (vh / h).min(1.0));
                let left = (-f32::from(offset.x) / w).clamp(0.0, 1.0 - fw);
                let top = (-f32::from(offset.y) / h).clamp(0.0, 1.0 - fh);
                Some(((left, top, fw, fh), (w, h), self.pixel_ratio.get()))
            }
            Mode::Export | Mode::Zines => None,
        }
    }

    /// The finished render to lay over the zoomed photo: only one of this
    /// photo with its current settings (an out-of-date one would show old
    /// edits).
    pub(crate) fn detail_tile(&self) -> Option<(Arc<RenderImage>, Region)> {
        if !self.detail_allowed() {
            return None;
        }
        let asset = self.catalog.selected_asset()?;
        let detail = &self.live_preview.detail;
        // A tile that shows the whole view; failing that the newest one of
        // this photo and these edits, which fills in part of it while the
        // next renders.
        let tiles = || {
            detail
                .tiles
                .iter()
                .filter(|(_, k)| k.asset == asset.id && k.settings == asset.develop)
        };
        let best = self
            .wanted_detail()
            .and_then(|wanted| tiles().find(|(_, k)| covers(k, &wanted)))
            .or_else(|| tiles().next())?;
        Some((best.0.clone(), best.1.region))
    }

    /// Called as the window is drawn: asks for a new detail render once the
    /// view settles, if what's shown isn't what's needed.
    pub(crate) fn sync_detail_view(&mut self, cx: &mut Context<Self>) {
        let Some(wanted) = self.wanted_detail() else {
            self.live_preview.detail.requested = None;
            self.live_preview.detail.timer = None;
            return;
        };
        let detail = &mut self.live_preview.detail;
        // Already shown by a tile (kept at the front, so it's the last to
        // go), or about to be by the one being rendered.
        if let Some(at) = detail.covering(&wanted) {
            let tile = detail.tiles.remove(at);
            detail.tiles.insert(0, tile);
            detail.requested = None;
            detail.timer = None;
            return;
        }
        if detail.requested.as_ref().is_some_and(|r| covers(r, &wanted)) {
            return;
        }
        detail.requested = Some(with_margin(wanted));
        self.live_preview.detail.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            this.update(cx, |this, cx| this.start_detail_render(cx)).ok();
        }));
    }

    fn start_detail_render(&mut self, cx: &mut Context<Self>) {
        self.live_preview.detail.timer = None;
        if self.live_preview.detail.busy {
            // Asked again when the running one finishes.
            self.live_preview.detail.requested = None;
            return;
        }
        // The latest view, which may have moved since the timer started.
        let Some(key) = self.wanted_detail().map(with_margin) else {
            return;
        };
        let Some(full) = self.full_source.clone() else {
            return;
        };
        self.live_preview.detail.requested = Some(key);
        self.live_preview.detail.busy = true;
        let epoch = self.preview_epoch;
        let render = cx.background_executor().spawn(async move {
            let bgra =
                rustyedit_processing::render_detail(&full, &key.settings, key.region, key.size);
            Arc::new(RenderImage::new(vec![image::Frame::new(bgra)]))
        });
        cx.spawn(async move |this, cx| {
            let image = render.await;
            this.update(cx, |this, cx| {
                let detail = &mut this.live_preview.detail;
                detail.busy = false;
                if this.preview_epoch == epoch {
                    for old in detail.insert(image, key) {
                        this.live_preview.retire_image(old);
                    }
                } else {
                    this.live_preview.retire_image(image);
                }
                if this.live_preview.detail.requested == Some(key) {
                    this.live_preview.detail.requested = None;
                }
                // Draws it, and asks again if the view moved meanwhile.
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

    fn key(region: Region, size: (u32, u32)) -> DetailKey {
        DetailKey { asset: 1, settings: DevelopSettings::default(), region, size }
    }

    fn image() -> Arc<RenderImage> {
        Arc::new(RenderImage::new(vec![image::Frame::new(image::RgbaImage::new(1, 1))]))
    }

    #[test]
    fn the_tile_has_a_margin_at_the_same_sharpness() {
        let screen = key((0.4, 0.4, 0.1, 0.1), (1000, 800));
        let tile = with_margin(screen);
        // A quarter of the screen more on each side, the same pixels per
        // photo pixel.
        assert!((tile.region.0 - 0.375).abs() < 1e-5 && (tile.region.2 - 0.15).abs() < 1e-5);
        assert_eq!(tile.size, (1500, 1200));
        assert!(covers(&tile, &screen));
        // A small pan stays inside it; a big one doesn't.
        assert!(covers(&tile, &key((0.42, 0.38, 0.1, 0.1), (1000, 800))));
        assert!(!covers(&tile, &key((0.6, 0.4, 0.1, 0.1), (1000, 800))));
    }

    #[test]
    fn the_margin_stops_at_the_photos_edge_and_shrinks_for_huge_screens() {
        let at_edge = with_margin(key((0.0, 0.0, 0.2, 0.2), (1000, 1000)));
        assert_eq!(at_edge.region.0, 0.0);
        assert!((at_edge.region.2 - 0.25).abs() < 1e-5);
        // An 8K-wide view can't take any margin on top of the cap.
        let huge = key((0.2, 0.2, 0.5, 0.5), (6000, 3000));
        let tile = with_margin(huge);
        assert!(tile.size.0 <= MAX_TILE_EDGE && tile.size.1 <= MAX_TILE_EDGE);
        assert!(covers(&tile, &huge));
    }

    #[test]
    fn a_blurrier_or_other_tile_does_not_cover() {
        let screen = key((0.4, 0.4, 0.1, 0.1), (1000, 800));
        // Zoomed in further: needs more pixels per photo pixel.
        let sharper = key((0.42, 0.42, 0.05, 0.05), (1000, 800));
        assert!(!covers(&screen, &sharper));
        // Zoomed out slightly (same pixels over a bigger region = coarser).
        let coarser = key((0.4, 0.4, 0.1, 0.1), (900, 720));
        assert!(!covers(&coarser, &screen));
        let mut other = screen;
        other.asset = 2;
        assert!(!covers(&screen, &other));
        other = screen;
        other.settings.exposure = 1.0;
        assert!(!covers(&screen, &other));
    }

    #[test]
    fn recent_tiles_are_kept_and_stale_ones_dropped() {
        let mut view = DetailView::default();
        let a = key((0.0, 0.0, 0.2, 0.2), (400, 400));
        let b = key((0.5, 0.5, 0.2, 0.2), (400, 400));
        assert!(view.insert(image(), a).is_empty());
        assert!(view.insert(image(), b).is_empty());
        // Going back to `a` needs no render.
        assert_eq!(view.covering(&a), Some(1));
        // A tile that contains an older one replaces it.
        let wide = key((0.0, 0.0, 0.4, 0.4), (800, 800));
        assert_eq!(view.insert(image(), wide).len(), 1);
        assert_eq!(view.tiles.len(), 2);
        // Never more than KEEP_TILES.
        let c = key((0.8, 0.0, 0.1, 0.1), (200, 200));
        let d = key((0.8, 0.8, 0.1, 0.1), (200, 200));
        view.insert(image(), c);
        view.insert(image(), d);
        assert_eq!(view.tiles.len(), KEEP_TILES);
        // Edits to the photo make every old tile stale.
        let mut edited = d;
        edited.settings.exposure = 0.5;
        assert_eq!(view.insert(image(), edited).len(), KEEP_TILES);
        assert_eq!(view.tiles.len(), 1);
        assert_eq!(view.take().len(), 1);
    }
}
