//! The Zines module's manual layout: the pages in the middle, the photos
//! chosen for the zine in a bar on the right, a strip of page thumbnails
//! along the bottom, and what can be done with the selection above the pages.
//!
//! - Drag a photo from the bar onto a slot of a page, or onto a *free*
//!   page, where it lands under the pointer. Drag a text box or an empty
//!   photo frame from the top of the bar onto a free page.
//! - On a free page, click a box to select it (Shift adds), drag to move,
//!   drag its corner to resize, drag the handle above it to turn it, or
//!   drag across empty paper to select several. Edges snap to the margins
//!   and the middle, with guides.
//! - The strip below shows every page: click one to go to it, drag one onto
//!   another to reorder.
//! - Pages can be shown as spreads (left and right, as a booklet reads) and
//!   previewed as they will print.

use gpui::{
    canvas, div, img, prelude::*, px, relative, AnyElement, ClickEvent, Context, DragMoveEvent,
    MouseButton, MouseDownEvent, ObjectFit, PathBuilder, Pixels, Point, Render, SharedString,
    Window,
};
use rustyedit_core::{
    zine_ink_on, ZineCell, ZineContent, ZineFreeItem, ZinePageLayout, ZineRect, ZineSpot,
    ZINE_SHEET_PANELS,
};
use rustyedit_ui::sp;

use crate::{
    workspace::Workspace,
    zine_edit::{GestureKind, ZOOM_RANGE},
    zine_visuals::Stage,
    zines::{plural, ZineDrag},
};

/// Width of the bar of photos.
const TRAY_W: f32 = 244.0;
/// Height of the strip of page thumbnails.
const STRIP_H: f32 = 104.0;

/// Marks a drag of a box on a free page. Nothing follows the pointer: the
/// box itself moves.
pub(crate) struct ZineGesture;

pub(crate) struct NoGhost;

impl Render for NoGhost {
    fn render(&mut self, _w: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Something new dragged from the bar onto a free page.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NewItem {
    Text,
    Frame,
}

#[derive(Clone)]
pub(crate) struct NewItemDrag {
    kind: NewItem,
    theme: rustyedit_ui::Theme,
}

impl Render for NewItemDrag {
    fn render(&mut self, _w: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .h(sp(26.0))
            .px_3()
            .rounded_md()
            .bg(theme.surface_elevated)
            .border_1()
            .border_color(theme.accent)
            .shadow_lg()
            .text_size(sp(11.5))
            .text_color(theme.text)
            .child(match self.kind {
                NewItem::Text => "Text box",
                NewItem::Frame => "Photo frame",
            })
    }
}

/// A page thumbnail being dragged to a new place.
#[derive(Clone)]
pub(crate) struct PageDrag {
    page: usize,
    theme: rustyedit_ui::Theme,
}

impl Render for PageDrag {
    fn render(&mut self, _w: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .h(sp(26.0))
            .px_3()
            .rounded_md()
            .bg(theme.surface_elevated)
            .border_1()
            .border_color(theme.accent)
            .shadow_lg()
            .text_size(sp(11.5))
            .text_color(theme.text)
            .child(format!("Page {}", self.page + 1))
    }
}

impl Workspace {
    /// The whole manual workbench: the pages and what controls them, then
    /// the bar of photos.
    pub(crate) fn render_zine_workbench(
        &self,
        layout: &ZinePageLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let middle = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(sp(0.0))
            .min_h(sp(0.0))
            .bg(theme.background)
            .child(self.zine_toolbar(layout, cx))
            .child(self.zine_context_bar(cx))
            .child(
                div()
                    .id("zine-pages-scroll")
                    .track_scroll(&self.zine_ui.scroll)
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .content_start()
                    .items_start()
                    .gap_6()
                    .flex_1()
                    .min_h(sp(0.0))
                    .overflow_y_scroll()
                    .px_6()
                    .py_4()
                    .children(self.zine_cards(cx)),
            )
            .child(self.zine_strip(layout, cx));
        let workbench = div()
            .flex()
            .flex_row()
            .flex_1()
            .min_w(sp(0.0))
            .min_h(sp(0.0))
            .child(middle)
            .child(self.zine_tray(cx))
            .into_any_element();
        self.zine_ui.cache.borrow_mut().prune(window);
        workbench
    }

    // --- Toolbar ----------------------------------------------------------------

    fn zine_toolbar(&self, layout: &ZinePageLayout, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let count = self.zine.photos.len();
        let unplaced = self
            .zine
            .photos
            .iter()
            .filter(|p| self.zine.placements(p) == 0)
            .count();
        let mut summary = format!(
            "{count} photo{} \u{00B7} {} page{}",
            plural(count),
            layout.pages.len(),
            plural(layout.pages.len())
        );
        if unplaced > 0 {
            summary.push_str(&format!(" \u{00B7} {unplaced} not on a page yet"));
        }
        let entity = cx.entity();
        let button = |id: &'static str, label: &'static str, tip: &'static str, on: bool, enabled: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .h(sp(26.0))
                .px_2()
                .rounded_md()
                .text_size(sp(11.0))
                .border_1()
                .when(on, |el| {
                    el.bg(theme.accent_muted)
                        .border_color(theme.accent)
                        .text_color(theme.text)
                })
                .when(!on && enabled, |el| {
                    el.border_color(theme.border_subtle)
                        .text_color(theme.text_muted)
                        .cursor_pointer()
                        .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                })
                .when(!enabled, |el| {
                    el.border_color(theme.border_subtle)
                        .text_color(theme.text_faint.opacity(0.5))
                })
                .tooltip(rustyedit_ui::tip(tip, &theme))
                .child(label)
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_4()
            .flex_shrink_0()
            .px_6()
            .py_2()
            .bg(theme.surface)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w(sp(0.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(sp(16.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child(self.zine.options.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(sp(11.0))
                            .text_color(if unplaced > 0 { theme.accent } else { theme.text_faint })
                            .child(summary),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        button("zine-undo", "\u{21B6} Undo", "Undo the last change to the zine (\u{2318}Z)", false, !self.zine_ui.undo.is_empty())
                            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.zine_undo(cx))),
                    )
                    .child(
                        button("zine-redo", "\u{21B7} Redo", "Redo (\u{21E7}\u{2318}Z)", false, !self.zine_ui.redo.is_empty())
                            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.zine_redo(cx))),
                    )
                    .child(div().w(sp(8.0)))
                    .child(
                        button("zine-spreads", "Spreads", "Show the pages as left and right pages, as a booklet reads", self.zine_ui.spreads, true)
                            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                                this.zine_ui.spreads = !this.zine_ui.spreads;
                                cx.notify();
                            })),
                    )
                    .child(
                        button("zine-preview", "Preview", "See the zine as it will print", false, true)
                            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.open_zine_preview(cx))),
                    )
                    .child(
                        div().w(sp(180.0)).child(rustyedit_ui::labeled_slider(
                            "zine-zoom",
                            "Zoom",
                            self.zine_ui.zoom,
                            rustyedit_ui::SliderSpec {
                                min: ZOOM_RANGE.0,
                                max: ZOOM_RANGE.1,
                                default: crate::zine_edit::ZOOM_DEFAULT,
                                signed: false,
                                track: rustyedit_ui::SliderTrack::Accent,
                            },
                            &theme,
                            move |value, _w: &mut Window, cx: &mut gpui::App| {
                                entity.update(cx, |this, cx| {
                                    this.zine_ui.zoom = value;
                                    cx.notify();
                                });
                            },
                        )),
                    )
                    .child(
                        button("zine-reseed", "Start again", "Throw the pages away and lay the photos out automatically again", false, true)
                            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                                this.zine_ui.clear_selection();
                                this.zine_ui.page = None;
                                this.update_zine(cx, |z| z.seed_manual())
                            })),
                    ),
            )
            .into_any_element()
    }

    // --- The pages ------------------------------------------------------------------

    /// The size cards are drawn at.
    pub(crate) fn zine_stage(&self) -> Stage {
        let size_mm = self.zine.manual_page_size();
        let zoom = self.zine_ui.zoom.clamp(ZOOM_RANGE.0, ZOOM_RANGE.1);
        let sheet = self.zine.manual.pages.iter().any(|p| p.is_sheet());
        // Cards are `zoom` high; a sheet is wide, so it is drawn wider.
        let width_px = if sheet { zoom * 1.7 } else { zoom * size_mm.0 / size_mm.1 };
        Stage { size_mm, width_px }
    }

    /// Every page as a card (the cover first), then the card that adds one.
    /// With spreads on they come two to a spread, as a booklet reads.
    fn zine_cards(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let options = &self.zine.options;
        let stage = self.zine_stage();
        let sheet = self.zine.manual.pages.iter().any(|p| p.is_sheet());
        let paper = options.paper.rgb();

        let mut cards: Vec<AnyElement> = Vec::new();
        if options.cover && !sheet {
            let m = options.margin_mm as f32;
            let size = stage.size_mm;
            let hero = ZineRect {
                x: m,
                y: m,
                w: size.0 - 2.0 * m,
                h: (size.1 - 2.0 * m) * 0.75,
            };
            let slot = self.zine_slot(
                "zine-cover-slot".into(),
                hero,
                size,
                ZineSpot::Cover,
                Some("Cover photo".into()),
                false,
                cx,
            );
            let title = div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(relative(0.05))
                .text_center()
                .text_size(px(10.0 * stage.width_px / 200.0 * 1.4))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(gpui::black().opacity(0.7))
                .child(options.title.clone())
                .into_any_element();
            cards.push(self.page_card("Cover", stage, paper, None, vec![slot, title], cx));
        }
        for (page_ix, page) in self.zine.manual.pages.iter().enumerate() {
            let title = if page.is_sheet() {
                "Folded sheet".to_string()
            } else {
                format!("Page {}", page_ix + usize::from(options.cover) + 1)
            };
            let background = page.background.unwrap_or(paper);
            if page.is_free() {
                let contents = self.free_page_contents(page_ix, stage, cx);
                cards.push(self.page_card(&title, stage, background, Some(page_ix), contents, cx));
                continue;
            }
            let Some((size, places)) = self.zine.slot_places(page_ix) else {
                continue;
            };
            let slots = places
                .iter()
                .enumerate()
                .map(|(slot_ix, (rect, rotated))| {
                    let label = page.is_sheet().then(|| match (options.cover, slot_ix) {
                        (true, 0) => "1 \u{00B7} Cover".to_string(),
                        (true, n) if n == ZINE_SHEET_PANELS - 1 => format!("{} \u{00B7} Back", n + 1),
                        _ => format!("{}", slot_ix + 1),
                    });
                    self.zine_slot(
                        SharedString::from(format!("zine-slot-{page_ix}-{slot_ix}")),
                        *rect,
                        size,
                        ZineSpot::Slot { page: page_ix, slot: slot_ix },
                        label,
                        *rotated,
                        cx,
                    )
                })
                .collect();
            cards.push(self.page_card(&title, stage, background, Some(page_ix), slots, cx));
        }

        let mut out: Vec<AnyElement> = if self.zine_ui.spreads && !sheet {
            // The first page stands alone on the right; the rest pair up.
            let mut spreads = Vec::new();
            let mut cards = cards.into_iter();
            spreads.push(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_0()
                    .child(
                        div()
                            .w(px(stage.width_px))
                            .h(px(stage.height_px()))
                            .border_1()
                            .border_color(theme.border_subtle)
                            .opacity(0.5),
                    )
                    .children(cards.next())
                    .into_any_element(),
            );
            loop {
                let (a, b) = (cards.next(), cards.next());
                if a.is_none() {
                    break;
                }
                spreads.push(
                    div()
                        .relative()
                        .flex()
                        .flex_row()
                        .items_start()
                        .gap_0()
                        .child(
                            div()
                                .absolute()
                                .left(px(stage.width_px - 1.0))
                                .top_0()
                                .w(px(2.0))
                                .h(px(stage.height_px()))
                                .bg(gpui::black().opacity(0.25)),
                        )
                        .children(a)
                        .children(b)
                        .into_any_element(),
                );
            }
            spreads
        } else {
            cards
        };
        if !sheet {
            out.push(self.add_page_card(stage, cx));
        }
        out
    }

    /// A page on a card: its paper with `contents` on it, a title and, for
    /// an ordinary page, controls. A photo dropped on the paper itself goes
    /// in the first free slot (or lands under the pointer on a free page).
    fn page_card(
        &self,
        title: &str,
        stage: Stage,
        paper: [u8; 3],
        page: Option<usize>,
        contents: Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let free = page.is_some_and(|p| self.zine.manual.pages.get(p).is_some_and(|m| m.is_free()));
        let sheet = page.is_some_and(|p| self.zine.manual.pages.get(p).is_some_and(|m| m.is_sheet()));
        let focused = page.is_some() && self.zine_ui.page == page;
        let controls = page.filter(|_| !sheet).map(|p| self.page_controls(p, cx));
        let paper_colour = gpui::rgb(u32::from(paper[0]) << 16 | u32::from(paper[1]) << 8 | u32::from(paper[2]));
        div()
            .flex()
            .flex_col()
            .gap_1p5()
            .flex_shrink_0()
            .w(px(stage.width_px))
            .child(
                div()
                    .id(SharedString::from(format!("zine-card-{}", page.map_or(-1, |i| i as i64))))
                    .relative()
                    .w(px(stage.width_px))
                    .h(px(stage.height_px()))
                    .overflow_hidden()
                    .bg(paper_colour)
                    .border_1()
                    .border_color(if focused { theme.accent.opacity(0.7) } else { theme.border })
                    .shadow_md()
                    .when_some(page, |el, page| {
                        el.drag_over::<ZineDrag>(move |style, _, _, _| {
                            style.border_color(theme.accent).border_2()
                        })
                        .drag_over::<NewItemDrag>(move |style, _, _, _| {
                            style.border_color(theme.accent).border_2()
                        })
                        .on_drop(cx.listener(move |this, drag: &ZineDrag, window, cx| {
                            if free {
                                let at = window.mouse_position();
                                this.drop_on_free_page(drag, page, at, cx);
                            } else {
                                this.drop_on_page(drag, page, cx);
                            }
                        }))
                        .on_drop(cx.listener(move |this, drag: &NewItemDrag, window, cx| {
                            let at = window.mouse_position();
                            this.drop_new_item(drag.kind, page, at, cx);
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, _w, cx| {
                                // A press on bare paper: this is the page
                                // being worked on, and a drag from here
                                // draws a box round photos.
                                this.zine_ui.page = Some(page);
                                let extend = event.modifiers.shift;
                                let base = if extend { this.zine_ui.items.clone() } else { Vec::new() };
                                if !extend {
                                    this.zine_ui.items.clear();
                                    this.zine_ui.reposition = false;
                                }
                                if free {
                                    this.begin_zine_gesture(
                                        page,
                                        GestureKind::Marquee { additive: extend, base },
                                        event.position,
                                        cx,
                                    );
                                }
                                cx.notify();
                            }),
                        )
                    })
                    .when(free, |el| {
                        el.on_drag(ZineGesture, |_, _, _, cx| cx.new(|_| NoGhost)).on_drag_move(
                            cx.listener(move |this, event: &DragMoveEvent<ZineGesture>, _w, cx| {
                                if let Some(page) = page {
                                    this.zine_gesture_move(page, event.event.position, cx);
                                }
                            }),
                        )
                    })
                    .children(contents),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(sp(11.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(if focused { theme.accent } else { theme.text_muted })
                            .child(title.to_string()),
                    )
                    .children(controls),
            )
            .into_any_element()
    }

    /// The card at the end that adds a page.
    fn add_page_card(&self, stage: Stage, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let add = |id: &'static str, label: &'static str, tip: &'static str, free: bool, slots: usize, cx: &mut Context<Self>| {
            div()
                .id(id)
                .flex()
                .items_center()
                .justify_center()
                .h(sp(26.0))
                .w(sp(150.0))
                .rounded_md()
                .text_size(sp(11.0))
                .text_color(theme.text_muted)
                .cursor_pointer()
                .border_1()
                .border_color(theme.border)
                .hover(|el| el.border_color(theme.accent).text_color(theme.text).bg(theme.surface))
                .tooltip(rustyedit_ui::tip(tip, &theme))
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.update_zine(cx, |z| {
                        if free {
                            z.add_free_page()
                        } else {
                            z.add_page(slots)
                        }
                    });
                    let last = this.zine.manual.pages.len().saturating_sub(1);
                    this.zine_ui.page = Some(last);
                    this.zine_ui.items.clear();
                    cx.notify();
                }))
        };
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .flex_shrink_0()
            .w(px(stage.width_px))
            .h(px(stage.height_px()))
            .rounded_md()
            .border_1()
            .border_color(theme.border_subtle)
            .child(
                div()
                    .text_size(sp(12.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text_muted)
                    .child("Add a page"),
            )
            .child(add("zine-add-free", "Free layout", "Place and size photos and text anywhere", true, 0, cx))
            .child(add("zine-add-1", "1 photo", "A page with one photo", false, 1, cx))
            .child(add("zine-add-2", "2 photos", "A page with two photos", false, 2, cx))
            .child(add("zine-add-4", "4 photos", "A page of four photos", false, 4, cx))
            .into_any_element()
    }

    /// The row under a page: how its photos are arranged, move and delete.
    fn page_controls(&self, page: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let last = self.zine.manual.pages.len().saturating_sub(1);
        let (slots, free) = self
            .zine
            .manual
            .pages
            .get(page)
            .map_or((0, false), |p| (p.slots.len(), p.is_free()));
        let small = |id: SharedString, text: &'static str, on: bool, enabled: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .justify_center()
                .h(sp(20.0))
                .min_w(sp(20.0))
                .px_1()
                .rounded_md()
                .text_size(sp(10.5))
                .when(on, |el| el.bg(theme.accent_muted).text_color(theme.text))
                .when(!on && enabled, |el| {
                    el.text_color(theme.text_muted)
                        .cursor_pointer()
                        .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                })
                .when(!enabled, |el| el.text_color(theme.text_faint.opacity(0.4)))
                .child(text)
        };
        let per = [(1usize, "1"), (2, "2"), (4, "4")].map(|(n, text)| {
            small(
                SharedString::from(format!("zine-page-{page}-per-{n}")),
                text,
                !free && slots == n,
                true,
            )
            .tooltip(rustyedit_ui::tip(
                format!("{n} photo{} on this page, in a grid", plural(n)),
                &theme,
            ))
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                this.zine_ui.clear_selection();
                this.update_zine(cx, |z| z.set_page_slots(page, n))
            }))
            .into_any_element()
        });
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_0p5()
            .children(per)
            .child(
                small(SharedString::from(format!("zine-page-{page}-free")), "Free", free, true)
                    .tooltip(rustyedit_ui::tip("Place and size photos and text anywhere on the page", &theme))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.zine_ui.clear_selection();
                        this.update_zine(cx, |z| z.make_page_free(page))
                    })),
            )
            .child(div().w(sp(6.0)))
            .child(
                small(SharedString::from(format!("zine-page-{page}-left")), "\u{25C0}", false, page > 0)
                    .tooltip(rustyedit_ui::tip("Move the page earlier", &theme))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.zine_ui.clear_selection();
                        this.update_zine(cx, |z| {
                            z.shift_page(page, -1);
                        })
                    })),
            )
            .child(
                small(SharedString::from(format!("zine-page-{page}-right")), "\u{25B6}", false, page < last)
                    .tooltip(rustyedit_ui::tip("Move the page later", &theme))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.zine_ui.clear_selection();
                        this.update_zine(cx, |z| {
                            z.shift_page(page, 1);
                        })
                    })),
            )
            .child(
                small(SharedString::from(format!("zine-page-{page}-delete")), "\u{2715}", false, true)
                    .tooltip(rustyedit_ui::tip("Delete the page (its photos stay in the zine)", &theme))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.zine_ui.clear_selection();
                        this.zine_ui.page = None;
                        this.update_zine(cx, |z| z.remove_page(page))
                    })),
            )
            .into_any_element()
    }

    // --- Slots ----------------------------------------------------------------

    /// A slot: a rectangle on a card that takes a dropped photo and, once
    /// filled, can be dragged elsewhere or cleared.
    #[allow(clippy::too_many_arguments)]
    fn zine_slot(
        &self,
        id: SharedString,
        rect: ZineRect,
        size: (f32, f32),
        spot: ZineSpot,
        label: Option<String>,
        rotated: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let photo = self.zine.at(spot).cloned();
        let fixed = self.zine.is_fixed(spot);
        let thumb = photo.as_ref().and_then(|p| self.zine_thumb(p));
        let frame = div()
            .id(id)
            .absolute()
            .left(relative(rect.x / size.0))
            .top(relative(rect.y / size.1))
            .w(relative(rect.w / size.0))
            .h(relative(rect.h / size.1))
            .rounded_sm()
            .overflow_hidden()
            .border_1()
            .border_color(if fixed {
                theme.border_subtle
            } else if photo.is_some() {
                theme.accent.opacity(0.35)
            } else {
                theme.accent.opacity(0.55)
            })
            .bg(if photo.is_some() {
                gpui::black().opacity(0.08)
            } else {
                theme.accent.opacity(0.07)
            });
        if fixed {
            return frame
                .flex()
                .items_center()
                .justify_center()
                .text_size(sp(9.0))
                .text_color(gpui::black().opacity(0.45))
                .child(label.unwrap_or_default())
                .into_any_element();
        }
        let frame = frame
            .drag_over::<ZineDrag>(move |style, _, _, _| {
                style.border_color(theme.accent).bg(theme.accent.opacity(0.25))
            })
            .on_drop(cx.listener(move |this, drag: &ZineDrag, _w, cx| {
                cx.stop_propagation();
                this.drop_on_spot(drag, spot, cx);
            }));
        match photo {
            Some(photo) => {
                let name = photo
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let drag = ZineDrag {
                    photo,
                    from: Some(spot),
                    label: name,
                    theme,
                };
                frame
                    .cursor_grab()
                    .on_drag(drag, |drag, _offset, _window, cx| cx.new(|_| drag.clone()))
                    .child(match thumb {
                        Some(path) => img(path)
                            .object_fit(ObjectFit::Contain)
                            .size_full()
                            .into_any_element(),
                        None => div().size_full().into_any_element(),
                    })
                    .child(
                        div()
                            .id(SharedString::from(format!("zine-slot-clear-{:?}", spot_key(spot))))
                            .absolute()
                            .top(sp(2.0))
                            .right(sp(2.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(sp(16.0))
                            .rounded_full()
                            .bg(gpui::black().opacity(0.6))
                            .text_size(sp(8.0))
                            .text_color(gpui::white())
                            .cursor_pointer()
                            .hover(|el| el.bg(theme.danger))
                            .child("\u{2715}")
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                this.update_zine(cx, |z| z.clear_spot(spot))
                            })),
                    )
                    .into_any_element()
            }
            None => frame
                .flex()
                .items_center()
                .justify_center()
                .text_size(sp(9.0))
                .text_color(gpui::black().opacity(0.45))
                .child(match (label, rotated) {
                    (Some(label), _) => label,
                    (None, true) => "\u{21BB} drop a photo".to_string(),
                    (None, false) => "Drop a photo".to_string(),
                })
                .into_any_element(),
        }
    }

    // --- Free pages -------------------------------------------------------------

    /// A box of a free page as the cell the layout would print.
    fn item_cell(&self, item: &ZineFreeItem, stage: Stage) -> Option<ZineCell> {
        let (pw, ph) = stage.size_mm;
        let rect = ZineRect {
            x: item.x * pw,
            y: item.y * ph,
            w: item.w * pw,
            h: item.h * ph,
        };
        let content = match (&item.text, &item.photo) {
            (Some(_), _) => ZineContent::Text,
            (None, Some(photo)) => ZineContent::Photo(self.zine.position(photo)?),
            (None, None) => return None,
        };
        Some(ZineCell {
            rotation: item.rotation,
            frame: item.frame,
            text: item.text.clone(),
            ..ZineCell::plain(rect, content, false)
        })
    }

    /// What is drawn on a free page: its boxes, then what selecting them
    /// shows (outline, handles), the guides of a drag in progress and the
    /// box being dragged round photos.
    fn free_page_contents(&self, page: usize, stage: Stage, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(manual) = self.zine.manual.pages.get(page) else {
            return Vec::new();
        };
        let (pw, ph) = stage.size_mm;
        let s = stage.px_per_mm();
        let ui = &self.zine_ui;
        let background = manual.background.unwrap_or(self.zine.options.paper.rgb());
        let ink = zine_ink_on(background);
        let bounds = ui.bounds.clone();
        let mut out: Vec<AnyElement> = Vec::new();

        // The page's place on screen, as the pointer needs it.
        out.push(
            canvas(move |b, _, _| bounds.set(page, b), |_, _, _, _| {})
                .absolute()
                .size_full()
                .into_any_element(),
        );
        // The margins, faintly.
        let m = self.zine.options.margin_mm as f32;
        out.push(
            div()
                .absolute()
                .left(relative(m / pw))
                .top(relative(m / ph))
                .w(relative(1.0 - 2.0 * m / pw))
                .h(relative(1.0 - 2.0 * m / ph))
                .border_1()
                .border_color(gpui::black().opacity(0.10))
                .into_any_element(),
        );
        if manual.items.is_empty() {
            out.push(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .text_size(sp(11.0))
                    .text_color(gpui::black().opacity(0.4))
                    .child("Drag photos or text here")
                    .child(div().text_size(sp(10.0)).child("or pick a template above"))
                    .into_any_element(),
            );
        }

        // The text being typed shows as it is typed.
        let typing = ui.text_edit.filter(|(p, _)| *p == page).and_then(|(_, i)| {
            self.draft_for(crate::workspace::TextTarget::ZineText)
                .map(|d| (i, d.to_string()))
        });

        for (ix, item) in manual.items.iter().enumerate() {
            let selected = ui.is_selected(page, ix);
            let rect_px = (item.x * pw * s, item.y * ph * s, item.w * pw * s, item.h * ph * s);
            let mut item_view = item.clone();
            if let (Some((i, draft)), Some(text)) = (&typing, item_view.text.as_mut()) {
                if *i == ix {
                    text.text = draft.clone();
                }
            }
            // How it looks.
            match self.item_cell(&item_view, stage) {
                Some(cell) => out.extend(self.cell_visual(&cell, stage, ink)),
                None => out.push(self.empty_frame(page, ix, rect_px, selected, cx)),
            }
            // How it is grabbed.
            out.push(self.item_handle(page, ix, item, rect_px, selected, cx));
        }
        out.extend(self.selection_chrome(page, stage, cx));

        for (guide_page, guide) in &ui.guides {
            if *guide_page != page {
                continue;
            }
            let line = div().absolute().bg(theme.accent);
            out.push(
                if guide.vertical {
                    line.left(relative(guide.at)).top_0().h_full().w(px(1.0))
                } else {
                    line.top(relative(guide.at)).left_0().w_full().h(px(1.0))
                }
                .into_any_element(),
            );
        }
        if let Some((_, r)) = ui.marquee.filter(|(p, _)| *p == page) {
            out.push(
                div()
                    .absolute()
                    .left(relative(r.0))
                    .top(relative(r.1))
                    .w(relative(r.2))
                    .h(relative(r.3))
                    .bg(theme.accent.opacity(0.12))
                    .border_1()
                    .border_color(theme.accent)
                    .into_any_element(),
            );
        }
        out
    }

    /// A frame with no photo in it yet: a place to drop one.
    fn empty_frame(&self, page: usize, item: usize, rect_px: (f32, f32, f32, f32), selected: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        div()
            .id(SharedString::from(format!("zine-empty-{page}-{item}")))
            .absolute()
            .left(px(rect_px.0))
            .top(px(rect_px.1))
            .w(px(rect_px.2))
            .h(px(rect_px.3))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_1()
            .bg(theme.accent.opacity(0.08))
            .border_1()
            .border_color(if selected { theme.accent } else { theme.accent.opacity(0.5) })
            .text_size(sp(10.0))
            .text_color(gpui::black().opacity(0.5))
            .drag_over::<ZineDrag>(move |style, _, _, _| {
                style.bg(theme.accent.opacity(0.25)).border_color(theme.accent)
            })
            .on_drop(cx.listener(move |this, drag: &ZineDrag, _w, cx| {
                cx.stop_propagation();
                this.fill_frame(drag, page, item, cx);
            }))
            .child("Drop a photo")
            .into_any_element()
    }

    /// The part of a box that takes the mouse: click, Shift-click, drag to
    /// move (or to move the picture inside a filled frame), double-click to
    /// edit text, and drop a photo on a frame.
    fn item_handle(
        &self,
        page: usize,
        ix: usize,
        item: &ZineFreeItem,
        rect_px: (f32, f32, f32, f32),
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let is_text = item.is_text();
        let is_frame = !is_text;
        let rotation = item.rotation;
        div()
            .id(SharedString::from(format!("zine-item-{page}-{ix}")))
            .absolute()
            .left(px(rect_px.0))
            .top(px(rect_px.1))
            .w(px(rect_px.2))
            .h(px(rect_px.3))
            .cursor_grab()
            .when(rotation.abs() < 0.01 && !selected, |el| {
                el.border_1()
                    .border_color(gpui::transparent_black())
                    .hover(|el| el.border_color(theme.accent.opacity(0.6)))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _w, cx| {
                    // The page behind would start a marquee.
                    cx.stop_propagation();
                    this.zine_click_item(page, ix, event.modifiers.shift, cx);
                    let ui = &this.zine_ui;
                    let pan = ui.reposition
                        && ui.single() == Some((page, ix))
                        && this
                            .zine
                            .manual
                            .pages
                            .get(page)
                            .and_then(|p| p.items.get(ix))
                            .is_some_and(|i| i.frame.fit == rustyedit_core::ZineFit::Fill && i.photo.is_some());
                    let kind = if pan {
                        let focus = this.zine.manual.pages[page].items[ix].frame.focus;
                        GestureKind::Pan { item: ix, focus }
                    } else {
                        GestureKind::Move
                    };
                    this.begin_zine_gesture(page, kind, event.position, cx);
                }),
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, _w, cx| {
                if event.click_count() >= 2 && is_text {
                    this.start_text_edit(page, ix, cx);
                }
            }))
            .on_drag(ZineGesture, |_, _, _, cx| cx.new(|_| NoGhost))
            .on_drag_move(cx.listener(move |this, event: &DragMoveEvent<ZineGesture>, _w, cx| {
                this.zine_gesture_move(page, event.event.position, cx);
            }))
            .when(is_frame, |el| {
                // A photo dropped on a frame goes into it.
                el.drag_over::<ZineDrag>(move |style, _, _, _| style.border_color(theme.accent).border_2())
                    .on_drop(cx.listener(move |this, drag: &ZineDrag, _w, cx| {
                        cx.stop_propagation();
                        this.fill_frame(drag, page, ix, cx);
                    }))
            })
            .into_any_element()
    }

    /// The outline and handles of the selection on `page`: a corner to
    /// resize by and, for one box, a handle above it to turn it by.
    fn selection_chrome(&self, page: usize, stage: Stage, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let ui = &self.zine_ui;
        if ui.page != Some(page) || ui.items.is_empty() {
            return Vec::new();
        }
        let Some(manual) = self.zine.manual.pages.get(page) else {
            return Vec::new();
        };
        let (w, h) = (stage.width_px, stage.height_px());
        let rects: Vec<_> = ui
            .items
            .iter()
            .filter_map(|&i| manual.items.get(i))
            .map(|i| i.rect())
            .collect();
        let Some((bx, by, bw, bh)) = rustyedit_core::zine_bounding_box(&rects) else {
            return Vec::new();
        };
        let single_rotation = (ui.items.len() == 1)
            .then(|| manual.items.get(ui.items[0]).map(|i| i.rotation))
            .flatten()
            .unwrap_or(0.0);
        // The box's corners on screen, turned when one box is turned.
        let (cx_px, cy_px) = ((bx + bw / 2.0) * w, (by + bh / 2.0) * h);
        let (sin, cos) = single_rotation.to_radians().sin_cos();
        let corner = |fx: f32, fy: f32| -> Point<Pixels> {
            let (dx, dy) = ((fx - 0.5) * bw * w, (fy - 0.5) * bh * h);
            gpui::point(px(cx_px + dx * cos - dy * sin), px(cy_px + dx * sin + dy * cos))
        };
        let corners = [corner(0.0, 0.0), corner(1.0, 0.0), corner(1.0, 1.0), corner(0.0, 1.0)];
        let accent = theme.accent;
        let outline_points = corners;
        let mut out = vec![canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let points: Vec<Point<Pixels>> = outline_points
                    .iter()
                    .map(|p| gpui::point(bounds.origin.x + p.x, bounds.origin.y + p.y))
                    .collect();
                let mut path = PathBuilder::stroke(px(1.5));
                path.add_polygon(&points, true);
                if let Ok(path) = path.build() {
                    window.paint_path(path, accent);
                }
            },
        )
        .absolute()
        .size_full()
        .into_any_element()];

        let handle = |id: &'static str, at: Point<Pixels>, cursor: gpui::CursorStyle| {
            div()
                .id(id)
                .absolute()
                .left(at.x - px(7.0))
                .top(at.y - px(7.0))
                .size(px(14.0))
                .rounded_full()
                .bg(theme.accent)
                .border_2()
                .border_color(gpui::white())
                .cursor(cursor)
        };
        out.push(
            handle("zine-resize", corners[2], gpui::CursorStyle::ResizeUpLeftDownRight)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        this.begin_zine_gesture(page, GestureKind::Resize, event.position, cx);
                    }),
                )
                .on_drag(ZineGesture, |_, _, _, cx| cx.new(|_| NoGhost))
                .on_drag_move(cx.listener(move |this, event: &DragMoveEvent<ZineGesture>, _w, cx| {
                    this.zine_gesture_move(page, event.event.position, cx);
                }))
                .into_any_element(),
        );
        // One photo can be turned (text is set straight, as the screen cannot
        // show it turned).
        let one_photo = ui.items.len() == 1
            && manual.items.get(ui.items[0]).is_some_and(|i| !i.is_text());
        if one_photo {
            let top = corner(0.5, 0.0);
            // 22 pixels beyond the top edge, in the box's own up direction.
            let at = gpui::point(top.x + px(22.0 * sin), top.y - px(22.0 * cos));
            let origin = ui.bounds.get(page).map(|b| b.origin).unwrap_or_default();
            let centre = gpui::point(origin.x + px(cx_px), origin.y + px(cy_px));
            let rotation = single_rotation;
            out.push(
                handle("zine-rotate", at, gpui::CursorStyle::PointingHand)
                    .bg(gpui::white())
                    .border_color(theme.accent)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(sp(9.0))
                    .text_color(theme.accent)
                    .child("\u{21BB}")
                    .tooltip(rustyedit_ui::tip("Drag to turn", &theme))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            let from = f32::from(event.position.y - centre.y)
                                .atan2(f32::from(event.position.x - centre.x))
                                .to_degrees();
                            this.begin_zine_gesture(
                                page,
                                GestureKind::Rotate { centre, from, rotation },
                                event.position,
                                cx,
                            );
                        }),
                    )
                    .on_drag(ZineGesture, |_, _, _, cx| cx.new(|_| NoGhost))
                    .on_drag_move(cx.listener(move |this, event: &DragMoveEvent<ZineGesture>, _w, cx| {
                        this.zine_gesture_move(page, event.event.position, cx);
                    }))
                    .into_any_element(),
            );
        }
        out
    }

    // --- Dropping ------------------------------------------------------------------

    /// A photo dropped into a frame.
    fn fill_frame(&mut self, drag: &ZineDrag, page: usize, item: usize, cx: &mut Context<Self>) {
        let (photo, from) = (drag.photo.clone(), drag.from);
        let mut filled = false;
        self.update_zine(cx, |z| {
            filled = z.free_fill_frame(page, item, photo);
            if let (Some(from), true) = (from, filled) {
                z.clear_spot(from);
            }
        });
        if filled {
            self.zine_select(page, vec![item], cx);
        }
    }

    /// A photo dropped on a free page lands under the pointer, as big as
    /// is sensible for its shape.
    pub(crate) fn drop_on_free_page(&mut self, drag: &ZineDrag, page: usize, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(bounds) = self.zine_ui.bounds.get(page) else {
            return;
        };
        let fx = f32::from(at.x - bounds.origin.x) / f32::from(bounds.size.width).max(1.0);
        let fy = f32::from(at.y - bounds.origin.y) / f32::from(bounds.size.height).max(1.0);
        let (pw, ph) = self.zine.manual_page_size();
        let aspect = self
            .zine_photo_size(&drag.photo)
            .map_or(1.5, |(w, h)| w as f32 / h.max(1) as f32);
        // Half the page along the photo's long side.
        let (mut w, mut h) = if aspect >= 1.0 {
            (0.5, 0.5 * pw / (aspect * ph))
        } else {
            (0.5 * ph * aspect / pw, 0.5)
        };
        let over = w.max(h);
        if over > 0.9 {
            (w, h) = (w * 0.9 / over, h * 0.9 / over);
        }
        let rect = (fx - w / 2.0, fy - h / 2.0, w, h);
        let (photo, from) = (drag.photo.clone(), drag.from);
        let mut placed = None;
        self.update_zine(cx, |z| {
            placed = z.free_place(page, photo, rect);
            // Carried here from a slot: the slot lets go.
            if let (Some(from), Some(_)) = (from, placed) {
                z.clear_spot(from);
            }
        });
        if let Some(item) = placed {
            self.zine_select(page, vec![item], cx);
        }
    }

    /// A text box or empty frame dropped on a page (turning a grid page into
    /// a free one first).
    pub(crate) fn drop_new_item(&mut self, kind: NewItem, page: usize, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(bounds) = self.zine_ui.bounds.get(page) else {
            // A grid page has no bounds yet: the middle of it.
            self.add_new_item(kind, page, None, cx);
            return;
        };
        let fx = f32::from(at.x - bounds.origin.x) / f32::from(bounds.size.width).max(1.0);
        let fy = f32::from(at.y - bounds.origin.y) / f32::from(bounds.size.height).max(1.0);
        self.add_new_item(kind, page, Some((fx, fy)), cx);
    }

    /// Puts a new text box or empty frame on `page` (centred, or at `at`).
    pub(crate) fn add_new_item(&mut self, kind: NewItem, page: usize, at: Option<(f32, f32)>, cx: &mut Context<Self>) {
        let (w, h) = match kind {
            NewItem::Text => (0.5, 0.12),
            NewItem::Frame => (0.4, 0.3),
        };
        let (cx_, cy) = at.unwrap_or((0.5, 0.5));
        let rect = (cx_ - w / 2.0, cy - h / 2.0, w, h);
        let font = self.zine.options.font;
        let mut made = None;
        self.update_zine(cx, |z| {
            z.make_page_free(page);
            let Some(manual) = z.manual.pages.get_mut(page).filter(|p| p.is_free()) else {
                return;
            };
            let item = match kind {
                NewItem::Text => {
                    let mut text = rustyedit_core::ZineTextBox::new("Your text", font);
                    text.size_pt = 18.0;
                    ZineFreeItem::text_box(text, rect)
                }
                NewItem::Frame => ZineFreeItem::photo_frame(
                    None,
                    rect,
                    rustyedit_core::ZineFrameStyle {
                        fit: rustyedit_core::ZineFit::Fill,
                        ..Default::default()
                    },
                ),
            };
            manual.items.push(item.clamped());
            made = Some(manual.items.len() - 1);
        });
        if let Some(item) = made {
            self.zine_select(page, vec![item], cx);
            if kind == NewItem::Text {
                self.start_text_edit(page, item, cx);
            }
        }
    }

    // --- The bar of photos -------------------------------------------------------

    fn zine_tray(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let total = self.zine.photos.len();
        let unplaced: Vec<usize> = (0..total)
            .filter(|&n| self.zine.placements(&self.zine.photos[n]) == 0)
            .collect();
        let only_unplaced = self.zine_ui.tray_unplaced && !unplaced.is_empty();
        let shown: Vec<usize> = if only_unplaced {
            unplaced.clone()
        } else {
            (0..total).collect()
        };
        let filter = |id: &'static str, label: String, on: bool, cx: &mut Context<Self>, only: bool| {
            div()
                .id(id)
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .h(sp(24.0))
                .rounded_md()
                .text_size(sp(11.0))
                .cursor_pointer()
                .border_1()
                .when(on, |el| {
                    el.bg(theme.accent_muted)
                        .border_color(theme.accent)
                        .text_color(theme.text)
                })
                .when(!on, |el| {
                    el.border_color(theme.background)
                        .text_color(theme.text_muted)
                        .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                })
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.zine_ui.tray_unplaced = only;
                    cx.notify();
                }))
        };

        let rows: Vec<AnyElement> = shown
            .iter()
            .map(|&n| {
                let photo = &self.zine.photos[n];
                let placed = self.zine.placements(photo);
                let name = photo
                    .path
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let thumb = self.zine_thumb(photo);
                let drag = ZineDrag {
                    photo: photo.clone(),
                    from: None,
                    label: name.clone(),
                    theme,
                };
                let photo = photo.clone();
                div()
                    .id(SharedString::from(format!("zine-tray-row-{n}")))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .p_1()
                    .rounded_lg()
                    .cursor_grab()
                    .border_1()
                    .border_color(if placed == 0 { theme.accent.opacity(0.7) } else { theme.border_subtle })
                    .bg(theme.background)
                    .hover(|el| el.border_color(theme.accent).bg(theme.surface_elevated))
                    .on_drag(drag, |drag, _offset, _window, cx| cx.new(|_| drag.clone()))
                    .child(
                        div()
                            .relative()
                            .flex_shrink_0()
                            .w(sp(78.0))
                            .h(sp(58.0))
                            .rounded_md()
                            .overflow_hidden()
                            .bg(gpui::black())
                            .child(match thumb {
                                Some(path) => img(path)
                                    .object_fit(ObjectFit::Contain)
                                    .size_full()
                                    .into_any_element(),
                                None => div().size_full().into_any_element(),
                            })
                            .child(
                                div()
                                    .absolute()
                                    .top(sp(3.0))
                                    .left(sp(3.0))
                                    .px_1()
                                    .rounded_sm()
                                    .bg(theme.background.opacity(0.8))
                                    .text_size(sp(9.0))
                                    .text_color(theme.text)
                                    .child(format!("{}", n + 1)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(sp(0.0))
                            .gap_0p5()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(sp(11.0))
                                    .text_color(theme.text)
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_size(sp(10.0))
                                    .text_color(if placed == 0 { theme.accent } else { theme.text_faint })
                                    .child(if placed == 0 {
                                        "Not on a page".to_string()
                                    } else {
                                        format!("On {placed} page spot{}", plural(placed))
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("zine-tray-remove-{n}")))
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .justify_center()
                            .size(sp(20.0))
                            .rounded_md()
                            .text_size(sp(10.0))
                            .text_color(theme.text_faint)
                            .cursor_pointer()
                            .hover(|el| el.bg(theme.surface).text_color(theme.danger))
                            .tooltip(rustyedit_ui::tip("Take out of the zine", &theme))
                            .child("\u{2715}")
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                let photo = photo.clone();
                                this.zine_ui.clear_selection();
                                this.update_zine(cx, |z| {
                                    z.remove(&[photo]);
                                })
                            })),
                    )
                    .into_any_element()
            })
            .collect();

        let tool = |id: &'static str, label: &'static str, hint: &'static str, kind: NewItem| {
            let drag = NewItemDrag { kind, theme };
            div()
                .id(id)
                .flex()
                .flex_1()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_0p5()
                .h(sp(46.0))
                .rounded_lg()
                .cursor_grab()
                .border_1()
                .border_color(theme.border_subtle)
                .bg(theme.background)
                .hover(|el| el.border_color(theme.accent).bg(theme.surface_elevated))
                .tooltip(rustyedit_ui::tip(hint, &theme))
                .on_drag(drag, |drag, _offset, _window, cx| cx.new(|_| drag.clone()))
                .child(
                    div()
                        .text_size(sp(14.0))
                        .text_color(theme.text)
                        .child(if kind == NewItem::Text { "T" } else { "\u{25A2}" }),
                )
                .child(div().text_size(sp(10.0)).text_color(theme.text_muted).child(label))
        };

        div()
            .id("zine-tray")
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(sp(TRAY_W))
            .h_full()
            .bg(theme.surface)
            .border_l_1()
            .border_color(theme.border)
            // A photo dragged here from a page comes off it.
            .drag_over::<ZineDrag>(move |style, drag, _, _| {
                if drag.from.is_some() {
                    style.bg(theme.accent.opacity(0.08)).border_color(theme.accent)
                } else {
                    style
                }
            })
            .on_drop(cx.listener(|this, drag: &ZineDrag, _w, cx| {
                if let Some(from) = drag.from {
                    this.update_zine(cx, |z| z.clear_spot(from));
                }
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_shrink_0()
                    .gap_2()
                    .p_3()
                    .border_b_1()
                    .border_color(theme.border_subtle)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(sp(13.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child("Photos"),
                            )
                            .child(
                                div()
                                    .text_size(sp(10.5))
                                    .text_color(theme.text_faint)
                                    .child(format!("{total} chosen")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(tool("zine-tool-text", "Text box", "Drag onto a page to write on it", NewItem::Text))
                            .child(tool("zine-tool-frame", "Photo frame", "Drag onto a page for an empty frame to fill", NewItem::Frame)),
                    )
                    .child(
                        div()
                            .text_size(sp(10.5))
                            .text_color(theme.text_muted)
                            .child("Drag a photo onto a page. Drop one back here to take it off."),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_1()
                            .p_0p5()
                            .rounded_lg()
                            .bg(theme.background)
                            .child(filter("zine-tray-all", format!("All {total}"), !only_unplaced, cx, false))
                            .child(filter(
                                "zine-tray-unplaced",
                                format!("Not placed {}", unplaced.len()),
                                only_unplaced,
                                cx,
                                true,
                            )),
                    ),
            )
            .child(
                div()
                    .id("zine-tray-list")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(sp(0.0))
                    .overflow_y_scroll()
                    .gap_1p5()
                    .p_2()
                    .children(rows)
                    .when(shown.is_empty(), |el| {
                        el.child(
                            div()
                                .p_3()
                                .text_size(sp(11.0))
                                .text_color(theme.text_faint)
                                .child("Every photo is on a page."),
                        )
                    }),
            )
            .into_any_element()
    }

    // --- The strip of pages ---------------------------------------------------------

    /// Every page small along the bottom: click one to go to it, drag one
    /// onto another to move it there.
    fn zine_strip(&self, layout: &ZinePageLayout, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let sheet = self.zine.manual.pages.iter().any(|p| p.is_sheet());
        let cover = self.zine.options.cover && !sheet;
        let thumbs = layout.pages.iter().enumerate().map(|(layout_ix, page)| {
            let manual_ix = layout_ix.checked_sub(usize::from(cover));
            let focused = manual_ix.is_some() && self.zine_ui.page == manual_ix;
            let picked = manual_ix.is_some_and(|m| self.zine_ui.pages_selected.contains(&m));
            let label = match manual_ix {
                None => "Cover".to_string(),
                Some(_) if sheet => "Sheet".to_string(),
                Some(_) => format!("{}", layout_ix + 1),
            };
            let mini = self.page_mini(page, layout.size, STRIP_H - 34.0);
            let thumb = div()
                .id(SharedString::from(format!("zine-strip-{layout_ix}")))
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .flex_shrink_0()
                .p_1()
                .rounded_md()
                .cursor_pointer()
                .border_1()
                .border_color(if focused { theme.accent } else { gpui::transparent_black() })
                .when(picked, |el| el.bg(theme.accent_muted))
                .hover(|el| el.bg(theme.surface_elevated))
                .child(div().border_1().border_color(theme.border).child(mini))
                .child(
                    div()
                        .text_size(sp(9.5))
                        .text_color(if focused { theme.accent } else { theme.text_faint })
                        .child(label),
                )
                .on_click(cx.listener(move |this, event: &ClickEvent, _w, cx| {
                    // Cmd- or Shift-click picks pages for copying a look to.
                    let m = event.modifiers();
                    if let (Some(page), true) = (manual_ix, m.platform || m.control || m.shift) {
                        let picked = &mut this.zine_ui.pages_selected;
                        match picked.iter().position(|&p| p == page) {
                            Some(at) => {
                                picked.remove(at);
                            }
                            None => picked.push(page),
                        }
                        cx.notify();
                        return;
                    }
                    this.zine_ui.pages_selected.clear();
                    this.zine_ui.page = manual_ix;
                    this.zine_ui.items.clear();
                    let child = if this.zine_ui.spreads { (layout_ix + 1) / 2 } else { layout_ix };
                    this.zine_ui.scroll.scroll_to_top_of_item(child);
                    cx.notify();
                }));
            match manual_ix {
                Some(from) => thumb
                    .on_drag(PageDrag { page: from, theme }, |drag, _o, _w, cx| cx.new(|_| drag.clone()))
                    .drag_over::<PageDrag>(move |style, _, _, _| {
                        style.border_color(theme.accent).bg(theme.accent.opacity(0.15))
                    })
                    .on_drop(cx.listener(move |this, drag: &PageDrag, _w, cx| {
                        this.zine_ui.page = Some(from);
                        this.zine_ui.clear_selection();
                        this.update_zine(cx, |z| {
                            z.move_page(drag.page, from);
                        });
                    }))
                    .into_any_element(),
                None => thumb.into_any_element(),
            }
        });
        div()
            .id("zine-strip")
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .flex_shrink_0()
            .h(sp(STRIP_H))
            .px_4()
            .overflow_x_scroll()
            .bg(theme.surface)
            .border_t_1()
            .border_color(theme.border)
            .children(thumbs)
            .into_any_element()
    }

    // --- Preview -------------------------------------------------------------------------

    pub(crate) fn open_zine_preview(&mut self, cx: &mut Context<Self>) {
        self.zine_ui.preview = Some(0);
        cx.notify();
    }

    /// The finished zine, a page (or a spread) at a time.
    pub(crate) fn render_zine_preview(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let layout = self.zine.layout();
        let total = layout.pages.len();
        let spreads = self.zine_ui.spreads && total > 1;
        let current = self.zine_ui.preview.unwrap_or(0).min(total.saturating_sub(1));
        // Pages shown: one, or a left and right page (the first stands alone).
        let shown: Vec<usize> = if spreads {
            if current == 0 {
                vec![0]
            } else {
                let left = if current % 2 == 1 { current } else { current - 1 };
                (left..=left + 1).filter(|&i| i < total).collect()
            }
        } else {
            vec![current]
        };
        let viewport = window.viewport_size();
        let avail_w = (f32::from(viewport.width) - self.settings.layout.inspector_width as f32 - 80.0).max(300.0);
        let avail_h = (f32::from(viewport.height) - 230.0).max(300.0);
        let (pw, ph) = layout.size;
        let across = if spreads { 2.0 } else { 1.0 };
        let width = (avail_w / across).min(avail_h * pw / ph).max(120.0);
        let stage = Stage { size_mm: layout.size, width_px: width };
        let pages = shown
            .iter()
            .filter_map(|&i| layout.pages.get(i))
            .map(|page| self.layout_page(page, stage))
            .collect::<Vec<_>>();
        let lone_first = spreads && current == 0;
        let nav = |id: &'static str, label: &'static str, enabled: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .justify_center()
                .h(sp(30.0))
                .px_3()
                .rounded_md()
                .text_size(sp(12.0))
                .border_1()
                .when(enabled, |el| {
                    el.border_color(theme.border)
                        .text_color(theme.text)
                        .cursor_pointer()
                        .hover(|el| el.bg(theme.surface_elevated))
                })
                .when(!enabled, |el| {
                    el.border_color(theme.border_subtle)
                        .text_color(theme.text_faint.opacity(0.5))
                })
                .child(label)
        };
        let step = if spreads { 2 } else { 1 };
        let prev_to = if spreads && current <= 2 { 0 } else { current.saturating_sub(step) };
        let next_to = (current + step).min(total.saturating_sub(1));
        let label = if shown.len() == 2 {
            format!("Pages {} and {} of {total}", shown[0] + 1, shown[1] + 1)
        } else {
            format!("Page {} of {total}", shown.first().map_or(0, |i| i + 1))
        };
        let result = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(sp(0.0))
            .min_h(sp(0.0))
            .bg(gpui::rgb(0x2b2b2b))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .flex_shrink_0()
                    .px_6()
                    .py_2()
                    .bg(theme.surface)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(sp(14.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child(format!("Preview \u{2014} {}", self.zine.options.title)),
                            )
                            .child(
                                div()
                                    .text_size(sp(11.0))
                                    .text_color(theme.text_faint)
                                    .child("As it will print. The type is set a touch differently on screen."),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(
                                nav("zine-preview-spreads", if spreads { "Single pages" } else { "Spreads" }, total > 1)
                                    .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                                        this.zine_ui.spreads = !this.zine_ui.spreads;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                nav("zine-preview-close", "Close preview", true)
                                    .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                                        this.zine_ui.preview = None;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h(sp(0.0))
                    .items_center()
                    .justify_center()
                    .gap_0()
                    .when(lone_first, |el| {
                        // The first page sits on the right, as in a booklet.
                        el.child(div().w(px(width)).h(px(stage.height_px())))
                    })
                    .children(pages),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .flex_shrink_0()
                    .py_3()
                    .child(
                        nav("zine-preview-prev", "\u{25C0} Previous", current > 0)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                if current > 0 {
                                    this.zine_ui.preview = Some(prev_to);
                                    cx.notify();
                                }
                            })),
                    )
                    .child(div().text_size(sp(12.0)).text_color(theme.text_muted).child(label))
                    .child(
                        nav("zine-preview-next", "Next \u{25B6}", current + 1 < total)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                                if current + 1 < total {
                                    this.zine_ui.preview = Some(next_to);
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .into_any_element();
        self.zine_ui.cache.borrow_mut().prune(window);
        result
    }
}

/// A key for a spot's element id.
fn spot_key(spot: ZineSpot) -> (usize, usize) {
    match spot {
        ZineSpot::Cover => (usize::MAX, 0),
        ZineSpot::Slot { page, slot } => (page, slot),
    }
}
