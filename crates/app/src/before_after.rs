//! Before / After: compare the photo as shot with its current edits, in the
//! Library loupe and in Develop.
//!
//! "Before" is the photo with every adjustment, retouch spot and gradient
//! removed but its crop, rotation, flips and lens distortion correction kept,
//! so the two always line up.
//! Three views: a split with a draggable divider, the two side by side, and
//! the before image alone. The `\` key switches the comparison on and off.

use rustyedit_ui::sp;
use gpui::{
    div, img, prelude::*, px, AnyElement, App, Bounds, Context, DragMoveEvent,
    ImageSource, MouseButton, ObjectFit, Pixels, Render, Window,
};

use crate::{
    actions::ToggleBeforeAfter,
    workspace::{LibraryView, Mode, Workspace},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum BaMode {
    #[default]
    Off,
    /// The before image on the left of a divider, the after on the right.
    Split,
    SideBySide,
    /// Just the before image.
    Before,
}

impl BaMode {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            BaMode::Off => "Off",
            BaMode::Split => "Split",
            BaMode::SideBySide => "Side by side",
            BaMode::Before => "Before",
        }
    }

    /// What the view shows, for menus.
    pub(crate) fn description(&self) -> &'static str {
        match self {
            BaMode::Off => "Show the edited photo",
            BaMode::Split => "Drag a divider across the photo",
            BaMode::SideBySide => "Both photos next to each other",
            BaMode::Before => "The photo without its edits",
        }
    }
}

pub(crate) struct BaState {
    pub(crate) mode: BaMode,
    /// Where the divider sits across the photo, 0-1.
    pub(crate) split: f32,
    /// The view `\` brings back.
    last: BaMode,
    /// Compare against the JPEG shot with this RAW instead of the photo
    /// without its edits.
    reference: bool,
}

impl Default for BaState {
    fn default() -> Self {
        Self {
            mode: BaMode::Off,
            split: 0.5,
            last: BaMode::Split,
            reference: false,
        }
    }
}

/// Drag payload for the divider handle.
struct BaDrag;

struct DragGhost;

impl Render for DragGhost {
    fn render(&mut self, _w: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl Workspace {
    pub fn toggle_before_after(
        &mut self,
        _: &ToggleBeforeAfter,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.ba_screen() {
            if self.ba_can_compare() {
                return;
            }
            self.status_message = Some("Before / After needs a photo that has been edited.".into());
            cx.notify();
            return;
        }
        let next = if self.ba.mode == BaMode::Off {
            self.ba.last
        } else {
            BaMode::Off
        };
        self.set_ba_mode(next, cx);
    }

    /// Whether the selected photo has edits worth comparing against.
    pub(crate) fn ba_photo_edited(&self) -> bool {
        self.catalog
            .selected_asset()
            .is_some_and(|asset| asset.develop.is_edited())
    }

    /// Whether this screen can show a comparison: Develop, or the loupe,
    /// for a photo that has been edited (an untouched photo has nothing to
    /// compare).
    pub(crate) fn ba_screen(&self) -> bool {
        let screen = match self.mode {
            Mode::Develop => true,
            Mode::Library => self.library_view == LibraryView::Loupe,
            Mode::Export => false,
        };
        screen && self.ba_can_compare()
    }

    /// The JPEG the comparison is against, when it is against the camera's.
    pub(crate) fn ba_reference(&self) -> Option<&rustyedit_core::Asset> {
        if !self.ba.reference {
            return None;
        }
        self.companion_jpeg(self.catalog.selected)
    }

    /// Something to compare: edits, or a JPEG reference.
    pub(crate) fn ba_can_compare(&self) -> bool {
        self.ba_photo_edited() || self.ba_reference().is_some()
    }

    /// Compares the RAW with the JPEG the camera made of the same shot.
    pub(crate) fn compare_with_jpeg(&mut self, cx: &mut Context<Self>) {
        let ix = self.catalog.selected;
        let Some(jpeg) = self.companion_jpeg(ix).map(|_| self.catalog.companion_of(ix)) else {
            return;
        };
        if self.ba.reference && self.ba.mode != BaMode::Off {
            self.set_ba_mode(BaMode::Off, cx);
            return;
        }
        self.ba.reference = true;
        if let Some(jpeg) = jpeg {
            self.request_loupe_for(jpeg, cx);
        }
        let mode = if self.ba.last == BaMode::Before {
            BaMode::Split
        } else {
            self.ba.last
        };
        self.set_ba_mode(mode, cx);
    }

    /// Comparison is on and this screen shows it.
    pub(crate) fn ba_active(&self) -> bool {
        self.ba.mode != BaMode::Off && self.ba_screen()
    }

    pub(crate) fn set_ba_mode(&mut self, mode: BaMode, cx: &mut Context<Self>) {
        if mode != BaMode::Off && !self.ba_can_compare() {
            return;
        }
        self.ba.mode = mode;
        if mode == BaMode::Off {
            self.ba.reference = false;
        }
        if mode != BaMode::Off {
            self.ba.last = mode;
            // The before image comes out of the develop pipeline, which in
            // the Library has to be woken up for this photo first.
            if self.source_image_for != self.catalog.selected_asset().map(|a| a.id) {
                self.load_source_image(cx);
            } else {
                self.request_preview_update(cx);
            }
        }
        cx.notify();
    }

    /// The rendered (before, after) pair, once both exist.
    fn ba_pair(&self) -> Option<(ImageSource, ImageSource)> {
        let before: ImageSource = match self.ba_reference() {
            Some(jpeg) => self.loupe_source(jpeg).into(),
            None => self.live_preview.before()?.into(),
        };
        Some((before, self.live_preview.image()?.into()))
    }

    /// The two corner tags: what is on the left and what is on the right.
    fn ba_labels(&self) -> (&'static str, &'static str) {
        if self.ba_reference().is_some() {
            ("CAMERA JPEG", "RAW")
        } else {
            ("BEFORE", "AFTER")
        }
    }

    /// What replaces the photo for the Before and Side-by-side views (the
    /// split is an overlay instead). `None` means draw the photo as usual.
    pub(crate) fn ba_replacement(&self, fitted: bool) -> Option<AnyElement> {
        if !self.ba_active() {
            return None;
        }
        let theme = self.theme;
        let mode = self.ba.mode;
        if mode == BaMode::Split || (mode == BaMode::SideBySide && !fitted) {
            return None;
        }
        let Some((before, after)) = self.ba_pair() else {
            return Some(
                div()
                    .text_color(theme.text_muted)
                    .child("Rendering the comparison\u{2026}")
                    .into_any_element(),
            );
        };
        let (before_label, after_label) = self.ba_labels();
        let tag = |text: &'static str| {
            div()
                .absolute()
                .top(sp(8.0))
                .left(sp(8.0))
                .px_2()
                .py_0p5()
                .rounded_sm()
                .bg(gpui::black().opacity(0.6))
                .text_size(sp(10.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(gpui::white())
                .child(text)
        };
        let frame = |source: ImageSource, label: &'static str| {
            div()
                .relative()
                .flex_1()
                .min_w(sp(0.0))
                .h_full()
                .child(img(source).object_fit(ObjectFit::Contain).size_full())
                .child(tag(label))
        };
        Some(match mode {
            BaMode::Before => div()
                .relative()
                .size_full()
                .child(img(before).object_fit(ObjectFit::Contain).size_full())
                .child(tag(before_label))
                .into_any_element(),
            _ => div()
                .flex()
                .flex_row()
                .gap_2()
                .size_full()
                .child(frame(before, before_label))
                .child(frame(after, after_label))
                .into_any_element(),
        })
    }

    /// The split view's overlay: the before image clipped at the divider,
    /// the divider itself with its handle, and corner tags. `photo` is where
    /// the photo is on screen and `container` the element the overlay is
    /// positioned inside, both in window coordinates.
    pub(crate) fn ba_split_overlay(
        &self,
        photo: Bounds<Pixels>,
        container: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.ba_active() || self.ba.mode != BaMode::Split {
            return None;
        }
        let (before, _) = self.ba_pair()?;
        let (before_label, after_label) = self.ba_labels();
        let split = self.ba.split.clamp(0.02, 0.98);
        let rx = f32::from(photo.origin.x - container.origin.x);
        let ry = f32::from(photo.origin.y - container.origin.y);
        let (rw, rh) = (f32::from(photo.size.width), f32::from(photo.size.height));
        let photo_left = f32::from(photo.origin.x);
        let entity = cx.entity();

        let tag = |text: &'static str, left: bool| {
            let t = div()
                .absolute()
                .top(px(ry + 8.0))
                .px_2()
                .py_0p5()
                .rounded_sm()
                .bg(gpui::black().opacity(0.6))
                .text_size(sp(10.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(gpui::white())
                .child(text);
            if left {
                t.left(px(rx + 8.0))
            } else {
                t.left(px(rx + rw - 52.0))
            }
        };

        let handle = div()
            .id("ba-handle")
            .absolute()
            .left(px(rx + rw * split - 14.0))
            .top(px(ry + rh / 2.0 - 14.0))
            .size(sp(28.0))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_ew_resize()
            .bg(gpui::white())
            .text_color(gpui::black())
            .text_size(sp(11.0))
            .shadow_md()
            .child("\u{25C2}\u{25B8}")
            .on_mouse_down(MouseButton::Left, |_, _w, cx| cx.stop_propagation())
            .on_drag(BaDrag, |_, _, _, cx| cx.new(|_| DragGhost))
            .on_drag_move(
                move |event: &DragMoveEvent<BaDrag>, _w: &mut Window, cx: &mut App| {
                    let x = f32::from(event.event.position.x) - photo_left;
                    entity.update(cx, |this, cx| {
                        this.ba.split = (x / rw.max(1.0)).clamp(0.02, 0.98);
                        cx.notify();
                    });
                },
            );

        Some(
            div()
                .absolute()
                .size_full()
                .child(
                    div()
                        .absolute()
                        .left(px(rx))
                        .top(px(ry))
                        .w(px(rw * split))
                        .h(px(rh))
                        .overflow_hidden()
                        .child(
                            img(before)
                                .absolute()
                                .left_0()
                                .top_0()
                                .w(px(rw))
                                .h(px(rh))
                                .object_fit(ObjectFit::Fill),
                        ),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(rx + rw * split - 1.0))
                        .top(px(ry))
                        .w(sp(2.0))
                        .h(px(rh))
                        .bg(gpui::white().opacity(0.9)),
                )
                .child(tag(before_label, true))
                .child(tag(after_label, false))
                .child(handle)
                .into_any_element(),
        )
    }
}
