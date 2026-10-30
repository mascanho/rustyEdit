//! Smart culling in the app (see `rustyedit_core::culling`):
//!
//! - "Best", "Soft", "Dark" and "Bright" badges on grid and filmstrip
//!   cells (the last two from exposure measured on thumbnails in the
//!   background).
//!
//! Each part can be switched off in Settings ▸ Culling.
//! - Photo ▸ Pick Best of Each Burst / Reject Soft Photos: one undo step.
//! - Full-screen cull view (Shift+F): one big photo and nothing else; P/X/U
//!   and 0-5 decide and move on to the next photo, arrows move, Esc leaves.
//!
//! The analysis is cached and only redone when photos, stacks or sharpness
//! scores change, so drawing the grid doesn't pay for it.

use rustyedit_ui::sp;
use std::{
    hash::{Hash, Hasher},
    rc::Rc,
    sync::Arc,
};

use gpui::{div, img, prelude::*, AnyElement, Context, ObjectFit, Window};
use rustyedit_core::{CullInfo, CullMark, Flag};
use rustyedit_ui::Theme;

use crate::{
    actions::{PickBestOfBursts, RejectBadExposures, RejectSoftPhotos, ToggleCullView},
    workspace::{Mode, Workspace},
};

impl Workspace {
    /// Culling suggestions for the catalog, recomputed only when something
    /// they depend on changed.
    pub(crate) fn cull_info(&self) -> Rc<CullInfo> {
        let culling = &self.settings.culling;
        // Asked for by the grid, the filmstrip, the cull bar and the loupe
        // on every frame; fingerprinting every photo each time adds up in a
        // big catalog. So a cached answer is trusted for a moment, as long
        // as the photo count and settings are the same.
        const TRUST: std::time::Duration = std::time::Duration::from_millis(300);
        let now = std::time::Instant::now();
        let shape = (
            self.catalog.assets.len(),
            culling.enabled,
            culling.detect_bursts,
        );
        if let (Some((at, n, on, bursts)), Some((_, info))) =
            (self.cull_checked.get(), &*self.cull_cache.borrow())
        {
            if (n, on, bursts) == shape && now.duration_since(at) < TRUST {
                return info.clone();
            }
        }
        // A cheap fingerprint of what the analysis reads.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        culling.enabled.hash(&mut hasher);
        culling.detect_bursts.hash(&mut hasher);
        self.catalog.assets.len().hash(&mut hasher);
        for asset in &self.catalog.assets {
            asset.sharpness.map(f32::to_bits).hash(&mut hasher);
            asset.exposure.map(|e| e.mean.to_bits()).hash(&mut hasher);
            asset.stack_id.hash(&mut hasher);
            asset.meta.captured.hash(&mut hasher);
            asset.phash.hash(&mut hasher);
            asset.missing.hash(&mut hasher);
            asset.copy_name.is_some().hash(&mut hasher);
        }
        let key = hasher.finish();
        self.cull_checked.set(Some((now, shape.0, shape.1, shape.2)));
        if let Some((cached, info)) = &*self.cull_cache.borrow() {
            if *cached == key {
                return info.clone();
            }
        }
        // Switched off: no suggestions at all.
        let info = Rc::new(if culling.enabled {
            CullInfo::analyze(&self.catalog.assets, culling.detect_bursts)
        } else {
            CullInfo::default()
        });
        *self.cull_cache.borrow_mut() = Some((key, info.clone()));
        info
    }

    /// `index`'s suggestions as its badges show them: only the kinds
    /// switched on in Settings.
    pub(crate) fn shown_mark(&self, info: &CullInfo, index: usize) -> CullMark {
        let culling = &self.settings.culling;
        let mut mark = info.mark(index);
        mark.best &= culling.show_best;
        mark.soft &= culling.show_soft;
        mark.dark &= culling.show_exposure;
        mark.bright &= culling.show_exposure;
        mark
    }

    /// Says so (and returns `false`) when smart culling is switched off.
    fn culling_on(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.settings.culling.enabled {
            self.status_message =
                Some("Smart culling is off. Turn it on in Settings \u{25B8} Culling.".into());
            cx.notify();
        }
        self.settings.culling.enabled
    }

    /// Measures the photos that have no sharpness score yet (taken while
    /// smart culling was off), from their cached thumbnails, in the
    /// background.
    pub(crate) fn measure_missing_sharpness(&mut self, cx: &mut Context<Self>) {
        let Some(cache) = self.thumbnail_cache.clone() else {
            return;
        };
        let jobs: Vec<rustyedit_core::Asset> = self
            .catalog
            .assets
            .iter()
            .filter(|a| a.sharpness.is_none() && !a.missing && a.width > 0)
            .cloned()
            .collect();
        if jobs.is_empty() {
            return;
        }
        if self.sharpness_progress.is_some() {
            return;
        }
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        self.sharpness_progress = Some((counter.clone(), jobs.len()));
        let work = cx.background_executor().spawn(async move {
            rustyedit_io::generate_thumbnails(&cache, &jobs, &counter, true)
        });
        cx.spawn(async move |this, cx| {
            let infos = work.await;
            this.update(cx, |this, cx| {
                this.sharpness_progress = None;
                let mut changed = false;
                for info in infos {
                    if let Some(asset) = this
                        .catalog
                        .assets
                        .get_mut(info.id as usize)
                        .filter(|a| a.id == info.id && a.sharpness.is_none())
                    {
                        asset.sharpness = info.sharpness;
                        changed |= info.sharpness.is_some();
                    }
                }
                if changed {
                    this.mark_catalog_dirty(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn pick_best_of_bursts(
        &mut self,
        _: &PickBestOfBursts,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.culling_on(cx) {
            return;
        }
        let best = self.cull_info().best();
        let done = self.flag_photos(&best, Flag::Picked, "Pick best of bursts", cx);
        self.status_message = Some(if self.cull_info().groups.is_empty() {
            "No bursts found (frames shot within 2 seconds, or stacks).".into()
        } else {
            format!(
                "Picked the sharpest frame of {} burst{}.",
                done,
                if done == 1 { "" } else { "s" }
            )
            .into()
        });
        cx.notify();
    }

    pub fn reject_soft_photos(
        &mut self,
        _: &RejectSoftPhotos,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.culling_on(cx) {
            return;
        }
        let soft = self.cull_info().soft();
        let done = self.flag_photos(&soft, Flag::Rejected, "Reject soft photos", cx);
        self.status_message = Some(match done {
            0 => "No soft photos found.".into(),
            n => format!(
                "Rejected {n} soft photo{} (undo with \u{2318}Z).",
                if n == 1 { "" } else { "s" }
            )
            .into(),
        });
        cx.notify();
    }

    pub fn reject_bad_exposures(
        &mut self,
        _: &RejectBadExposures,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.culling_on(cx) {
            return;
        }
        let bad = self.cull_info().badly_exposed();
        let done = self.flag_photos(&bad, Flag::Rejected, "Reject bad exposures", cx);
        self.status_message = Some(match done {
            0 => "No badly exposed photos found yet (they're measured in the background).".into(),
            n => format!(
                "Rejected {n} badly exposed photo{} (undo with \u{2318}Z).",
                if n == 1 { "" } else { "s" }
            )
            .into(),
        });
        cx.notify();
    }

    /// Measures the exposure of photos that haven't been, on their cached
    /// thumbnails, a batch at a time in the background. Cheap to call: it
    /// looks again at most every few seconds.
    pub(crate) fn scan_exposure(&mut self, cx: &mut Context<Self>) {
        const BATCH: usize = 300;
        const EVERY: std::time::Duration = std::time::Duration::from_secs(3);
        if !self.settings.culling.enabled
            || !self.settings.culling.show_exposure
            || self.exposure_scan_busy
            || self.exposure_scan_at.is_some_and(|at| at.elapsed() < EVERY)
        {
            return;
        }
        self.exposure_scan_at = Some(std::time::Instant::now());
        let all: Vec<(u64, std::path::PathBuf)> = self
            .catalog
            .assets
            .iter()
            .filter(|a| {
                a.exposure.is_none()
                    && !a.missing
                    && !a.is_virtual_copy()
                    && !self.exposure_failed.contains(&a.id)
            })
            .filter_map(|a| Some((a.id, a.thumbnail_path.clone()?)))
            .collect();
        self.exposure_left = all.len();
        let jobs: Vec<_> = all.into_iter().take(BATCH).collect();
        if jobs.is_empty() {
            return;
        }
        self.exposure_scan_busy = true;
        let work = cx.background_executor().spawn(async move {
            jobs.into_iter()
                .map(|(id, path)| {
                    let stats = image::open(&path)
                        .ok()
                        .and_then(|img| rustyedit_core::culling::exposure_stats(img.to_luma8().as_raw()));
                    (id, stats)
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let results = work.await;
            this.update(cx, |this, cx| {
                this.exposure_scan_busy = false;
                // Ids are positions in the catalog, as long as nothing was
                // removed meanwhile; check each.
                let mut any = false;
                for (id, stats) in results {
                    match stats {
                        Some(stats) => {
                            if let Some(asset) = this.catalog.assets.get_mut(id as usize).filter(|a| a.id == id) {
                                asset.exposure = Some(stats);
                                any = true;
                            }
                        }
                        None => {
                            this.exposure_failed.insert(id);
                        }
                    }
                }
                if any {
                    this.mark_catalog_dirty(cx);
                    // Look for the next batch soon.
                    this.exposure_scan_at = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Flags `indices` (skipping ones already flagged so) as one undo step.
    /// Returns how many changed.
    fn flag_photos(
        &mut self,
        indices: &[usize],
        flag: Flag,
        label: &str,
        cx: &mut Context<Self>,
    ) -> usize {
        let mut changes = Vec::new();
        for &ix in indices {
            let Some(asset) = self.catalog.assets.get(ix) else {
                continue;
            };
            if asset.flag == flag || asset.missing {
                continue;
            }
            let before = asset.state();
            self.catalog.assign_flag(ix, flag);
            changes.push((ix, before, self.catalog.assets[ix].state()));
        }
        let done = changes.len();
        if done > 0 {
            self.push_undo(label.to_string(), changes, false);
            self.mark_catalog_dirty(cx);
        }
        done
    }

    // --- Full-screen cull view ---------------------------------------------

    pub fn toggle_cull_view(&mut self, _: &ToggleCullView, window: &mut Window, cx: &mut Context<Self>) {
        if self.cull_view {
            self.leave_cull_view(window, cx);
        } else {
            self.enter_cull_view(window, cx);
        }
    }

    fn enter_cull_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.catalog.selected_asset().is_none() || self.is_importing {
            return;
        }
        // Library mode: Develop would decode every photo arrowed past.
        self.mode = Mode::Library;
        self.cull_view = true;
        if self.settings.culling.full_screen && !window.is_fullscreen() {
            window.toggle_fullscreen();
            self.cull_made_fullscreen = true;
        }
        self.prefetch_cull_neighbours(cx);
        cx.notify();
    }

    pub(crate) fn leave_cull_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cull_view = false;
        if std::mem::take(&mut self.cull_made_fullscreen) && window.is_fullscreen() {
            window.toggle_fullscreen();
        }
        cx.notify();
    }

    /// The photos either side of the selected one in the visible order.
    fn cull_neighbours(&self) -> (Option<usize>, Option<usize>) {
        let visible = self.catalog.visible_indices();
        let Some(at) = self.catalog.visible_position(self.catalog.selected) else {
            return (None, None);
        };
        (
            at.checked_sub(1).and_then(|p| visible.get(p).copied()),
            visible.get(at + 1).copied(),
        )
    }

    /// Starts making the large previews of the photos either side (RAW
    /// files need one), so moving to them is instant.
    pub(crate) fn prefetch_cull_neighbours(&mut self, cx: &mut Context<Self>) {
        if !self.cull_view {
            return;
        }
        let (prev, next) = self.cull_neighbours();
        self.request_loupe(cx);
        for ix in [next, prev].into_iter().flatten() {
            self.request_loupe_for(ix, cx);
        }
    }

    pub(crate) fn render_cull_view(&self, _cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let Some(asset) = self.catalog.selected_asset() else {
            return div().into_any_element();
        };
        let ix = self.catalog.selected;
        let mark = self.shown_mark(&self.cull_info(), ix);
        let visible = self.catalog.visible_indices();
        let position = self
            .catalog
            .visible_position(ix)
            .map(|p| format!("{} / {}", p + 1, visible.len()))
            .unwrap_or_default();

        let mut badges: Vec<AnyElement> = Vec::new();
        if mark.best {
            badges.push(cull_pill("BEST IN BURST", theme.pick, &theme));
        }
        if mark.soft {
            badges.push(cull_pill("SOFT", theme.star, &theme));
        }
        if mark.dark {
            badges.push(cull_pill("DARK", theme.text_muted, &theme));
        }
        if mark.bright {
            badges.push(cull_pill("BRIGHT", theme.text_muted, &theme));
        }
        if let Some((pos, len)) = mark.burst {
            badges.push(cull_pill(&format!("Burst {pos} of {len}"), theme.text_muted, &theme));
        }
        let flag = match asset.flag {
            Flag::Picked => ("\u{2691} Picked", theme.pick),
            Flag::Rejected => ("\u{2715} Rejected", theme.reject),
            Flag::Unflagged => ("", theme.text_muted),
        };
        let stars = asset.rating.value();
        let sharpness = asset
            .sharpness
            .filter(|_| self.settings.culling.enabled)
            .map(|s| format!("Sharpness {:.0}", s))
            .unwrap_or_default();

        let top = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .h(sp(40.0))
            .px_4()
            .text_size(sp(12.0))
            .text_color(gpui::white().opacity(0.85))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(asset.display_name())
                    .child(div().text_color(gpui::white().opacity(0.5)).child(position)),
            )
            .child(div().flex().flex_row().gap_2().children(badges))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(div().text_color(gpui::white().opacity(0.5)).child(sharpness))
                    .child(
                        div()
                            .text_color(theme.star)
                            .child("\u{2605}".repeat(stars as usize)),
                    )
                    .child(div().text_color(flag.1).child(flag.0)),
            );

        let hint = div()
            .flex()
            .justify_center()
            .h(sp(32.0))
            .items_center()
            .text_size(sp(11.0))
            .text_color(gpui::white().opacity(0.45))
            .child(
                "P pick \u{00B7} X reject \u{00B7} U unflag \u{00B7} 0\u{2013}5 rate \u{00B7} \
                 \u{2190} \u{2192} move \u{00B7} Esc leave",
            );

        // The neighbours' images, laid out but invisible, so they're
        // decoded before they're needed.
        let (prev, next) = self.cull_neighbours();
        let prefetch: Vec<AnyElement> = [next, prev]
            .into_iter()
            .flatten()
            .filter_map(|i| self.catalog.assets.get(i))
            .filter(|a| !a.missing)
            .map(|a| {
                img(self.loupe_source(a))
                    .absolute()
                    .size(sp(2.0))
                    .opacity(0.0)
                    .into_any_element()
            })
            .collect();

        div()
            .id("cull-view")
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .flex_col()
            .bg(gpui::black())
            .child(top)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h(sp(0.0))
                    .p_2()
                    .child(
                        img(self.loupe_source(asset))
                            .object_fit(ObjectFit::Contain)
                            .size_full(),
                    )
                    .children(prefetch),
            )
            .child(hint)
            .into_any_element()
    }
}

/// A small outlined label in the cull view's top bar.
fn cull_pill(text: &str, color: gpui::Hsla, _theme: &Theme) -> AnyElement {
    div()
        .h(sp(20.0))
        .px_2()
        .flex()
        .items_center()
        .rounded_sm()
        .border_1()
        .border_color(color.opacity(0.8))
        .text_size(sp(10.0))
        .text_color(color)
        .child(text.to_string())
        .into_any_element()
}

/// The badges in a thumbnail's bottom-right corner: Best / Soft from
/// culling, and a virtual copy's name.
pub(crate) fn corner_badges(
    copy_name: Option<String>,
    pair_tag: Option<&str>,
    mark: CullMark,
    theme: &Theme,
) -> Option<AnyElement> {
    let pill = |text: String, color: gpui::Hsla| {
        div()
            .h(sp(16.0))
            .px_1p5()
            .flex()
            .items_center()
            .rounded_sm()
            .bg(theme.background.opacity(0.85))
            .border_1()
            .border_color(color.opacity(0.7))
            .text_size(sp(9.0))
            .text_color(theme.text)
            .child(text)
            .into_any_element()
    };
    let mut pills = Vec::new();
    if mark.best {
        pills.push(pill("BEST".into(), theme.pick));
    }
    if mark.soft {
        pills.push(pill("SOFT".into(), theme.star));
    }
    if mark.dark {
        pills.push(pill("DARK".into(), theme.text_muted));
    }
    if mark.bright {
        pills.push(pill("BRIGHT".into(), theme.text_muted));
    }
    // RAW and JPG are told apart by colour as well as by name.
    if let Some(tag) = pair_tag {
        let color = if tag == "JPG" { theme.star } else { theme.accent };
        pills.push(pill(tag.to_string(), color));
    }
    if let Some(name) = copy_name {
        pills.push(pill(name, theme.accent));
    }
    (!pills.is_empty()).then(|| {
        div()
            .absolute()
            .right(sp(4.0))
            .bottom(sp(4.0))
            .flex()
            .flex_row()
            .gap_1()
            .children(pills)
            .into_any_element()
    })
}
