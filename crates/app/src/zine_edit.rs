//! Editing state for the Zines workbench: what is selected, drags in
//! progress (move, resize, turn, marquee, reposition in a frame), undo and
//! redo for the whole zine, the keyboard, and copying a page's layout.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use gpui::{Bounds, Context, Pixels, Point, ScrollHandle};
use rustyedit_core::{
    zine_snap_corner, zine_snap_move, Zine, ZineAlignMode, ZineDistributeAxis, ZineFreeItem,
    ZineGuide, ZINE_MIN_ITEM,
};

use crate::{
    workspace::{Mode, Workspace},
    zine_visuals::ComposeCache,
};

/// How near (as a page fraction) a photo's edge must come to a line to
/// snap to it.
const SNAP: f32 = 0.014;
/// The zoom slider's range (a page's height in pixels) and its start.
pub(crate) const ZOOM_RANGE: (f32, f32) = (170.0, 560.0);
pub(crate) const ZOOM_DEFAULT: f32 = 300.0;
/// How much undo remembers.
const HISTORY: usize = 100;
/// Nudging with the arrow keys, as a page fraction (and with Shift).
const NUDGE: f32 = 0.004;
const NUDGE_BIG: f32 = 0.025;

/// Where each page is on screen, to turn the pointer into a place on it.
#[derive(Default, Clone)]
pub(crate) struct PageBounds(Rc<RefCell<HashMap<usize, Bounds<Pixels>>>>);

impl PageBounds {
    pub(crate) fn set(&self, page: usize, bounds: Bounds<Pixels>) {
        self.0.borrow_mut().insert(page, bounds);
    }

    pub(crate) fn get(&self, page: usize) -> Option<Bounds<Pixels>> {
        self.0.borrow().get(&page).copied()
    }
}

/// What a drag on a free page is doing.
#[derive(Clone)]
pub(crate) enum GestureKind {
    Move,
    /// From the corner of the selection's box.
    Resize,
    /// Turning one photo: where its middle is on screen, the pointer's
    /// direction when it was grabbed (degrees), and its turn then.
    Rotate {
        centre: Point<Pixels>,
        from: f32,
        rotation: f32,
    },
    /// Dragging a box round photos; with `additive` the ones already
    /// selected stay.
    Marquee { additive: bool, base: Vec<usize> },
    /// Moving the picture inside a filled frame.
    Pan { item: usize, focus: (f32, f32) },
}

pub(crate) struct Gesture {
    pub(crate) page: usize,
    pub(crate) kind: GestureKind,
    pub(crate) start: Point<Pixels>,
    /// Where the page is on screen.
    pub(crate) bounds: Bounds<Pixels>,
    /// The boxes the gesture moves, as they were when it began.
    pub(crate) originals: Vec<(usize, (f32, f32, f32, f32))>,
    /// The zine before the gesture, for undo.
    pub(crate) before: Zine,
}

/// Everything the Zines workbench remembers between frames.
pub(crate) struct ZineUi {
    pub(crate) zoom: f32,
    /// The page worked on, and the boxes selected on it.
    pub(crate) page: Option<usize>,
    pub(crate) items: Vec<usize>,
    pub(crate) gesture: Option<Gesture>,
    pub(crate) guides: Vec<(usize, ZineGuide)>,
    /// The marquee being dragged, as a page and a box of page fractions.
    pub(crate) marquee: Option<(usize, (f32, f32, f32, f32))>,
    pub(crate) bounds: PageBounds,
    pub(crate) tray_unplaced: bool,
    pub(crate) undo: Vec<Zine>,
    pub(crate) redo: Vec<Zine>,
    pub(crate) spreads: bool,
    /// The page the preview shows, when it is open.
    pub(crate) preview: Option<usize>,
    pub(crate) scroll: ScrollHandle,
    pub(crate) layout_clipboard: Option<Vec<ZineFreeItem>>,
    /// The text box whose words are being typed.
    pub(crate) text_edit: Option<(usize, usize)>,
    /// Dragging a filled photo moves the picture in its frame.
    pub(crate) reposition: bool,
    pub(crate) cache: RefCell<ComposeCache>,
    /// The last change made by a slider, so one drag is one undo step.
    pub(crate) last_edit: Option<(&'static str, std::time::Instant)>,
}

impl Default for ZineUi {
    fn default() -> Self {
        Self {
            zoom: ZOOM_DEFAULT,
            page: None,
            items: Vec::new(),
            gesture: None,
            guides: Vec::new(),
            marquee: None,
            bounds: PageBounds::default(),
            tray_unplaced: false,
            undo: Vec::new(),
            redo: Vec::new(),
            spreads: false,
            preview: None,
            scroll: ScrollHandle::new(),
            layout_clipboard: None,
            text_edit: None,
            reposition: false,
            cache: RefCell::new(ComposeCache::default()),
            last_edit: None,
        }
    }
}

impl ZineUi {
    pub(crate) fn is_selected(&self, page: usize, item: usize) -> bool {
        self.page == Some(page) && self.items.contains(&item)
    }

    /// The one selected box, when exactly one is.
    pub(crate) fn single(&self) -> Option<(usize, usize)> {
        match (self.page, self.items.as_slice()) {
            (Some(page), [item]) => Some((page, *item)),
            _ => None,
        }
    }

    pub(crate) fn clear_selection(&mut self) {
        self.items.clear();
        self.reposition = false;
        self.text_edit = None;
    }
}

/// The keys the workbench answers to (when no text is being typed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ZineKey {
    Nudge(i8, i8, bool),
    Delete,
    SelectAll,
    Escape,
}

/// `value` kept between `lo` and `hi` (and never panicking when rounding
/// leaves `hi` a hair below `lo`).
fn clamp_to(value: f32, lo: f32, hi: f32) -> f32 {
    value.max(lo).min(hi.max(lo))
}

fn rects_overlap(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

impl Workspace {
    // --- Undo and redo ---------------------------------------------------------

    /// Changes the zine as one undoable step, and saves it.
    pub(crate) fn update_zine(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Zine)) {
        let before = self.zine.clone();
        change(&mut self.zine);
        crate::zines::sync_manual(&mut self.zine);
        self.record_zine_step(before);
        self.zine_ui.last_edit = None;
        self.zine_changed(cx);
    }

    /// Like [`Self::update_zine`], but changes made under the same `key` in
    /// quick succession (a slider being dragged) are one undo step.
    pub(crate) fn update_zine_keyed(&mut self, cx: &mut Context<Self>, key: &'static str, change: impl FnOnce(&mut Zine)) {
        let before = self.zine.clone();
        change(&mut self.zine);
        crate::zines::sync_manual(&mut self.zine);
        let folds = matches!(
            self.zine_ui.last_edit,
            Some((k, at)) if k == key && at.elapsed() < std::time::Duration::from_millis(900)
        );
        if !folds {
            self.record_zine_step(before);
        }
        self.zine_ui.last_edit = Some((key, std::time::Instant::now()));
        self.zine_changed(cx);
    }

    /// Remembers `before` for undo if the zine is no longer the same.
    fn record_zine_step(&mut self, before: Zine) {
        if self.zine != before {
            let ui = &mut self.zine_ui;
            ui.undo.push(before);
            if ui.undo.len() > HISTORY {
                ui.undo.remove(0);
            }
            ui.redo.clear();
        }
    }

    pub(crate) fn zine_undo(&mut self, cx: &mut Context<Self>) {
        let Some(previous) = self.zine_ui.undo.pop() else {
            self.status_message = Some("Nothing to undo in the zine.".into());
            cx.notify();
            return;
        };
        let current = std::mem::replace(&mut self.zine, previous);
        self.zine_ui.redo.push(current);
        self.after_zine_history(cx);
    }

    pub(crate) fn zine_redo(&mut self, cx: &mut Context<Self>) {
        let Some(next) = self.zine_ui.redo.pop() else {
            self.status_message = Some("Nothing to redo in the zine.".into());
            cx.notify();
            return;
        };
        let current = std::mem::replace(&mut self.zine, next);
        self.zine_ui.undo.push(current);
        self.after_zine_history(cx);
    }

    /// After undo or redo: keep the selection to what still exists.
    fn after_zine_history(&mut self, cx: &mut Context<Self>) {
        let ui = &mut self.zine_ui;
        let count = |page: usize| {
            self.zine
                .manual
                .pages
                .get(page)
                .map_or(0, |p| p.items.len())
        };
        if let Some(page) = ui.page {
            if page >= self.zine.manual.pages.len() {
                ui.page = None;
            }
            ui.items.retain(|&i| i < count(page));
        }
        ui.gesture = None;
        ui.guides.clear();
        ui.text_edit = None;
        self.zine_changed(cx);
    }

    // --- Selection -------------------------------------------------------------

    /// Selects `items` of `page`.
    pub(crate) fn zine_select(&mut self, page: usize, items: Vec<usize>, cx: &mut Context<Self>) {
        let ui = &mut self.zine_ui;
        if ui.page != Some(page) || ui.items != items {
            ui.reposition = false;
            ui.text_edit = None;
        }
        ui.page = Some(page);
        ui.items = items;
        cx.notify();
    }

    /// A click on a box: selects it, or with Shift adds it to (or takes it
    /// from) the selection.
    pub(crate) fn zine_click_item(&mut self, page: usize, item: usize, extend: bool, cx: &mut Context<Self>) {
        let ui = &self.zine_ui;
        let mut items = if ui.page == Some(page) { ui.items.clone() } else { Vec::new() };
        if extend {
            match items.iter().position(|&i| i == item) {
                Some(at) => {
                    items.remove(at);
                }
                None => items.push(item),
            }
        } else if !items.contains(&item) {
            items = vec![item];
        }
        self.zine_select(page, items, cx);
    }

    /// Cmd-A: every box on the page being worked on.
    fn zine_select_all(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.zine_ui.page.or(self.zine.manual.pages.len().checked_sub(1)) else {
            return;
        };
        let count = self.zine.manual.pages.get(page).map_or(0, |p| p.items.len());
        self.zine_select(page, (0..count).collect(), cx);
    }

    /// Cmd-D.
    pub fn zine_duplicate_action(&mut self, _: &crate::actions::ZineDuplicate, _w: &mut gpui::Window, cx: &mut Context<Self>) {
        if self.mode == Mode::Zines && self.text_draft.is_none() && !self.zine_ui.items.is_empty() {
            self.zine_duplicate_selection(cx);
        }
    }

    /// Removes the selected boxes from their page.
    pub(crate) fn zine_delete_selection(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.zine_ui.page else { return };
        let mut items = std::mem::take(&mut self.zine_ui.items);
        items.sort_unstable_by(|a, b| b.cmp(a));
        self.zine_ui.reposition = false;
        self.update_zine(cx, |z| {
            for item in items {
                z.free_remove(page, item);
            }
        });
    }

    /// Copies the selected boxes in front of the rest and selects the copies.
    pub(crate) fn zine_duplicate_selection(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.zine_ui.page else { return };
        let items = self.zine_ui.items.clone();
        let mut made = Vec::new();
        self.update_zine(cx, |z| {
            for item in items {
                made.extend(z.free_duplicate(page, item));
            }
        });
        if !made.is_empty() {
            self.zine_select(page, made, cx);
        }
    }

    pub(crate) fn zine_align(&mut self, mode: ZineAlignMode, cx: &mut Context<Self>) {
        let Some(page) = self.zine_ui.page else { return };
        let items = self.zine_ui.items.clone();
        self.update_zine(cx, |z| z.free_align(page, &items, mode));
    }

    pub(crate) fn zine_distribute(&mut self, axis: ZineDistributeAxis, cx: &mut Context<Self>) {
        let Some(page) = self.zine_ui.page else { return };
        let items = self.zine_ui.items.clone();
        self.update_zine(cx, |z| z.free_distribute(page, &items, axis));
    }

    /// Changes every selected box, a slider's drag being one undo step.
    pub(crate) fn zine_edit_selection_keyed(&mut self, key: &'static str, cx: &mut Context<Self>, change: impl Fn(&mut ZineFreeItem)) {
        let Some(page) = self.zine_ui.page else { return };
        let items = self.zine_ui.items.clone();
        self.update_zine_keyed(cx, key, |z| {
            for item in items {
                z.free_edit(page, item, &change);
            }
        });
    }

    /// Changes every selected box.
    pub(crate) fn zine_edit_selection(&mut self, cx: &mut Context<Self>, change: impl Fn(&mut ZineFreeItem)) {
        let Some(page) = self.zine_ui.page else { return };
        let items = self.zine_ui.items.clone();
        self.update_zine(cx, |z| {
            for item in items {
                z.free_edit(page, item, &change);
            }
        });
    }

    // --- The keyboard ------------------------------------------------------------

    /// A key the workbench may take. Returns whether it did (so the key
    /// does not also do what it does elsewhere).
    pub(crate) fn zine_key(&mut self, key: ZineKey, cx: &mut Context<Self>) -> bool {
        if self.mode != Mode::Zines || self.text_draft.is_some() {
            return false;
        }
        let ui = &self.zine_ui;
        let has_selection = ui.page.is_some() && !ui.items.is_empty();
        match key {
            ZineKey::Escape => {
                if ui.preview.is_some() {
                    self.zine_ui.preview = None;
                } else if ui.reposition {
                    self.zine_ui.reposition = false;
                } else if has_selection {
                    self.zine_ui.clear_selection();
                } else {
                    return false;
                }
                cx.notify();
                true
            }
            ZineKey::Delete => {
                if has_selection && self.zine.options.mode == rustyedit_core::ZineMode::Manual {
                    self.zine_delete_selection(cx);
                }
                // Never the Library's remove-from-catalog.
                true
            }
            ZineKey::SelectAll => {
                if self.zine.options.mode == rustyedit_core::ZineMode::Manual {
                    self.zine_select_all(cx);
                }
                true
            }
            ZineKey::Nudge(dx, dy, big) => {
                if has_selection {
                    let page = ui.page.unwrap_or(0);
                    let items = ui.items.clone();
                    let step = if big { NUDGE_BIG } else { NUDGE };
                    self.update_zine(cx, |z| z.free_nudge(page, &items, dx as f32 * step, dy as f32 * step));
                }
                true
            }
        }
    }

    // --- Gestures ----------------------------------------------------------------

    /// A drag begins on a free page.
    pub(crate) fn begin_zine_gesture(
        &mut self,
        page: usize,
        kind: GestureKind,
        start: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(bounds) = self.zine_ui.bounds.get(page) else {
            return;
        };
        let affected: Vec<usize> = match &kind {
            GestureKind::Move | GestureKind::Resize => self.zine_ui.items.clone(),
            GestureKind::Pan { item, .. } => vec![*item],
            GestureKind::Rotate { .. } => self.zine_ui.items.clone(),
            GestureKind::Marquee { .. } => Vec::new(),
        };
        let originals = affected
            .into_iter()
            .filter_map(|i| {
                self.zine
                    .manual
                    .pages
                    .get(page)
                    .and_then(|p| p.items.get(i))
                    .map(|item| (i, item.rect()))
            })
            .collect();
        self.zine_ui.gesture = Some(Gesture {
            page,
            kind,
            start,
            bounds,
            originals,
            before: self.zine.clone(),
        });
        cx.notify();
    }

    /// The pointer moved during a drag on `page`.
    pub(crate) fn zine_gesture_move(&mut self, page: usize, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(g) = self.zine_ui.gesture.as_ref().filter(|g| g.page == page) else {
            return;
        };
        let (bw, bh) = (
            f32::from(g.bounds.size.width).max(1.0),
            f32::from(g.bounds.size.height).max(1.0),
        );
        let (dx, dy) = (f32::from(at.x - g.start.x) / bw, f32::from(at.y - g.start.y) / bh);
        let (pw, ph) = self.zine.manual_page_size();
        let margin = (self.zine.options.margin_mm as f32 / pw, self.zine.options.margin_mm as f32 / ph);
        let rects: Vec<_> = g.originals.iter().map(|(_, r)| *r).collect();
        let bbox = rustyedit_core::zine_bounding_box(&rects);
        let kind = g.kind.clone();
        let originals = g.originals.clone();
        let (start_px, bounds) = (g.start, g.bounds);
        let mut guides = Vec::new();
        let mut updates: Vec<(usize, (f32, f32, f32, f32))> = Vec::new();
        let mut marquee = None;
        let mut selection = None;
        let mut turn: Option<(usize, f32)> = None;
        let mut focus: Option<(usize, (f32, f32))> = None;

        match kind {
            GestureKind::Move => {
                let Some((bx, by, bw, bh)) = bbox else { return };
                // The whole selection moves as one box, kept on the page
                // and snapped to the margins and the middle.
                let (snapped, found) = zine_snap_move(
                    (
                        clamp_to(bx + dx, 0.0, 1.0 - bw),
                        clamp_to(by + dy, 0.0, 1.0 - bh),
                        bw,
                        bh,
                    ),
                    margin,
                    SNAP,
                );
                let (sx, sy) = (
                    clamp_to(snapped.0, 0.0, 1.0 - bw) - bx,
                    clamp_to(snapped.1, 0.0, 1.0 - bh) - by,
                );
                guides = found;
                updates = originals
                    .iter()
                    .map(|&(i, r)| (i, (r.0 + sx, r.1 + sy, r.2, r.3)))
                    .collect();
            }
            GestureKind::Resize => {
                let Some((bx, by, bw, bh)) = bbox else { return };
                // Scaled from the box's top-left, keeping its shape.
                let width = clamp_to(bw + dx, ZINE_MIN_ITEM, 1.0 - bx);
                let (snapped, found) = zine_snap_corner((bx, by, width, bh * width / bw.max(1e-4)), margin, SNAP);
                let k = clamp_to(snapped.2 / bw.max(1e-4), 0.05, (1.0 - bx) / bw.max(1e-4));
                guides = found;
                updates = originals
                    .iter()
                    .map(|&(i, r)| {
                        (
                            i,
                            (
                                bx + (r.0 - bx) * k,
                                by + (r.1 - by) * k,
                                r.2 * k,
                                r.3 * k,
                            ),
                        )
                    })
                    .collect();
            }
            GestureKind::Rotate { centre, from, rotation } => {
                let angle = f32::from(at.y - centre.y)
                    .atan2(f32::from(at.x - centre.x))
                    .to_degrees();
                let mut turned = (rotation + (angle - from)).rem_euclid(360.0);
                if turned > 180.0 {
                    turned -= 360.0;
                }
                // Square-on is easy to find: within a few degrees of a
                // multiple of 15 it snaps there.
                let nearest = (turned / 15.0).round() * 15.0;
                if (turned - nearest).abs() < 2.5 {
                    turned = nearest;
                }
                if let Some(&(i, _)) = originals.first() {
                    turn = Some((i, turned));
                }
            }
            GestureKind::Marquee { additive, base } => {
                let (x0, y0) = (
                    f32::from(start_px.x - bounds.origin.x) / bw,
                    f32::from(start_px.y - bounds.origin.y) / bh,
                );
                let (x1, y1) = (x0 + dx, y0 + dy);
                let r = (x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs());
                marquee = Some((page, r));
                let hit: Vec<usize> = self
                    .zine
                    .manual
                    .pages
                    .get(page)
                    .map(|p| {
                        p.items
                            .iter()
                            .enumerate()
                            .filter(|(_, item)| rects_overlap(r, item.rect()))
                            .map(|(i, _)| i)
                            .collect()
                    })
                    .unwrap_or_default();
                let mut items = if additive { base } else { Vec::new() };
                for i in hit {
                    if !items.contains(&i) {
                        items.push(i);
                    }
                }
                selection = Some(items);
            }
            GestureKind::Pan { item, focus: start_focus } => {
                // The picture follows the pointer: dragging right shows
                // more of its left.
                if let Some(extent) = self.pan_extent(page, item) {
                    let mm_per_px = pw / f32::from(bounds.size.width).max(1.0);
                    let (dx_mm, dy_mm) = (
                        f32::from(at.x - start_px.x) * mm_per_px,
                        f32::from(at.y - start_px.y) * mm_per_px,
                    );
                    let fx = if extent.0 > 0.01 { start_focus.0 - dx_mm / extent.0 } else { start_focus.0 };
                    let fy = if extent.1 > 0.01 { start_focus.1 - dy_mm / extent.1 } else { start_focus.1 };
                    focus = Some((item, (fx.clamp(0.0, 1.0), fy.clamp(0.0, 1.0))));
                }
            }
        }

        for (i, rect) in updates {
            self.zine.free_set_rect(page, i, rect);
        }
        if let Some((i, rotation)) = turn {
            self.zine.free_edit(page, i, |item| item.rotation = rotation);
        }
        if let Some((i, f)) = focus {
            self.zine.free_edit(page, i, |item| item.frame.focus = f);
        }
        if let Some(items) = selection {
            self.zine_ui.page = Some(page);
            self.zine_ui.items = items;
        }
        self.zine_ui.marquee = marquee;
        self.zine_ui.guides = guides.into_iter().map(|g| (page, g)).collect();
        cx.notify();
    }

    /// How far (in millimetres, across and down) the picture of a filled
    /// photo reaches past its frame: what dragging it can reveal.
    fn pan_extent(&self, page: usize, item: usize) -> Option<(f32, f32)> {
        let it = self.zine.manual.pages.get(page)?.items.get(item)?;
        let photo = it.photo.as_ref()?;
        let size = self.zine_photo_size(photo)?;
        let (pw, ph) = self.zine.manual_page_size();
        let rect = rustyedit_core::ZineRect {
            x: it.x * pw,
            y: it.y * ph,
            w: it.w * pw,
            h: it.h * ph,
        };
        let g = rustyedit_core::zine_photo_geometry(rect, &it.frame, size, 0.0);
        Some((g.image.w - g.inner.w, g.image.h - g.inner.h))
    }

    /// The drag is over: what it did is kept as one undo step.
    pub(crate) fn end_zine_gesture(&mut self, cx: &mut Context<Self>) {
        let Some(g) = self.zine_ui.gesture.take() else {
            return;
        };
        self.zine_ui.guides.clear();
        self.zine_ui.marquee = None;
        self.record_zine_step(g.before);
        self.zine_changed(cx);
    }

    // --- Copying a page's layout -------------------------------------------------

    pub(crate) fn zine_copy_layout(&mut self, page: usize, cx: &mut Context<Self>) {
        let items = self.zine.page_items(page);
        self.status_message = Some(format!("Copied the layout of the page ({} boxes)", items.len()).into());
        self.zine_ui.layout_clipboard = Some(items);
        cx.notify();
    }

    /// Lays `page` out as the copied page was: its boxes and text, with
    /// the photos the page already had put into the photo frames in order.
    pub(crate) fn zine_paste_layout(&mut self, page: usize, cx: &mut Context<Self>) {
        let Some(mut items) = self.zine_ui.layout_clipboard.clone() else {
            return;
        };
        let mut own = self.zine.page_items(page).into_iter().filter_map(|i| i.photo);
        for item in items.iter_mut().filter(|i| !i.is_text()) {
            item.photo = own.next();
        }
        self.update_zine(cx, |z| {
            if let Some(manual) = z.manual.pages.get_mut(page) {
                if manual.is_sheet() {
                    return;
                }
                manual.slots.clear();
                manual.items = items;
            }
        });
        self.zine_select(page, Vec::new(), cx);
    }
}
