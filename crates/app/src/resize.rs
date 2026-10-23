//! Drag-to-resize for the side docks and the filmstrip. A grip records
//! which edge is being dragged; the workspace root then tracks the mouse
//! and converts its position into a size until the button is released.

use rustyedit_ui::sp;
use gpui::{
    div, prelude::*, px, AnyElement, Context, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Window,
};
use rustyedit_core::settings::{DockSide, FilmstripPosition};
use rustyedit_ui::{STATUS_BAR_HEIGHT, TITLE_BAR_HEIGHT};

use crate::{
    filmstrip::{FILM_FIXED_HEIGHT, FILM_PICTURE_MAX, FILM_PICTURE_MIN},
    workspace::Workspace,
};

const DOCK_MIN: f32 = 180.0;
const DOCK_MAX: f32 = 560.0;
/// Width of the invisible hit area straddling a dragged edge.
const GRIP_THICKNESS: f32 = 8.0;
/// Height of the filmstrip's toggle header, which sits outside the picture.
const FILM_HEADER_HEIGHT: f32 = 20.0;
const TREE_MIN: f32 = 80.0;
const TREE_MAX: f32 = 900.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeTarget {
    Dock(DockSide),
    Filmstrip,
    /// The Folders tree in the Navigator, resized vertically.
    FolderTree,
}

impl Workspace {
    /// A zero-footprint drag handle straddling an edge: it takes no layout
    /// space of its own, so adding it never shifts anything.
    pub(crate) fn resize_grip(&self, target: ResizeTarget, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let active = self.resizing == Some(target);
        let hit = div()
            .absolute()
            .bg(if active {
                theme.accent.opacity(0.6)
            } else {
                gpui::transparent_black()
            })
            .hover(|el| el.bg(theme.accent.opacity(0.35)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _w, cx| {
                    this.resizing = Some(target);
                    // Relative drags (the tree) measure from where they began.
                    this.resize_anchor = (
                        f32::from(event.position.y),
                        this.settings.folders.tree_height as f32,
                    );
                    cx.notify();
                }),
            );

        match target {
            ResizeTarget::Dock(_) => div()
                .relative()
                .flex_shrink_0()
                .w(sp(0.0))
                .h_full()
                .child(
                    hit.top_0()
                        .bottom_0()
                        .left(px(-GRIP_THICKNESS / 2.0))
                        .w(px(GRIP_THICKNESS))
                        .cursor_col_resize(),
                )
                .into_any_element(),
            ResizeTarget::FolderTree => div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(-GRIP_THICKNESS / 2.0))
                .h(px(GRIP_THICKNESS))
                .child(
                    hit.left_0()
                        .right_0()
                        .top_0()
                        .h(px(GRIP_THICKNESS))
                        .cursor_row_resize(),
                )
                .into_any_element(),
            ResizeTarget::Filmstrip => {
                let at_top = self.settings.layout.filmstrip_position == FilmstripPosition::Top;
                let hit = hit
                    .left_0()
                    .right_0()
                    .h(px(GRIP_THICKNESS))
                    .cursor_row_resize();
                // Straddle the edge that faces the photo.
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .when(at_top, |el| el.bottom(px(-GRIP_THICKNESS / 2.0)))
                    .when(!at_top, |el| el.top(px(-GRIP_THICKNESS / 2.0)))
                    .h(px(GRIP_THICKNESS))
                    .child(hit.top_0())
                    .into_any_element()
            }
        }
    }

    pub(crate) fn on_resize_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.histogram_drag_move(event, cx) || self.develop_drag_move(event, cx) {
            return;
        }
        if let Some((start, start_offset)) = self.loupe_drag {
            if event.dragging() {
                let max = self.loupe_scroll.max_offset();
                let x = (start_offset.x + (event.position.x - start.x)).clamp(-max.width, sp(0.0));
                let y = (start_offset.y + (event.position.y - start.y)).clamp(-max.height, sp(0.0));
                self.loupe_scroll.set_offset(gpui::point(x, y));
                cx.notify();
            } else {
                self.loupe_drag = None;
            }
            return;
        }
        let Some(target) = self.resizing else {
            return;
        };
        if !event.dragging() {
            // The button was released outside the window.
            self.finish_resize(cx);
            return;
        }
        let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
        let viewport = window.viewport_size();
        let (width, height) = (f32::from(viewport.width), f32::from(viewport.height));

        match target {
            ResizeTarget::Dock(side) => {
                // The grip sits between the dock and its rail, so the dock
                // is everything from the window edge to the pointer.
                let size = match side {
                    DockSide::Left => x,
                    DockSide::Right => width - x,
                }
                .clamp(DOCK_MIN, DOCK_MAX)
                .round() as u32;
                let arrangement = self.settings.layout.preset.arrangement();
                let panels = match side {
                    DockSide::Left => arrangement.left,
                    DockSide::Right => arrangement.right,
                };
                for panel in panels {
                    match panel {
                        rustyedit_core::settings::PanelKind::Navigator => {
                            self.settings.layout.navigator_width = size
                        }
                        rustyedit_core::settings::PanelKind::Inspector => {
                            self.settings.layout.inspector_width = size
                        }
                    }
                }
                // Develop shows its editing panel in the Inspector's slot
                // even in layouts that have no Inspector.
                if panels.is_empty() {
                    self.settings.layout.inspector_width = size;
                }
            }
            ResizeTarget::FolderTree => {
                let (start_y, start_height) = self.resize_anchor;
                let height = (start_height + (y - start_y)).clamp(TREE_MIN, TREE_MAX);
                self.settings.folders.tree_height = height.round() as _;
            }
            ResizeTarget::Filmstrip => {
                let available = match self.settings.layout.filmstrip_position {
                    FilmstripPosition::Bottom => height - STATUS_BAR_HEIGHT - y,
                    FilmstripPosition::Top => y - TITLE_BAR_HEIGHT,
                };
                let picture = (available - FILM_HEADER_HEIGHT - FILM_FIXED_HEIGHT)
                    .clamp(FILM_PICTURE_MIN, FILM_PICTURE_MAX);
                self.settings.layout.filmstrip_height = picture.round() as u32;
                self.reveal_in_filmstrip(self.catalog.selected);
            }
        }
        cx.notify();
    }

    pub(crate) fn on_resize_up(
        &mut self,
        _: &MouseUpEvent,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.loupe_drag = None;
        self.end_develop_drag();
        self.end_histogram_drag();
        self.finish_resize(cx);
    }

    fn finish_resize(&mut self, cx: &mut Context<Self>) {
        if self.resizing.take().is_some() {
            if let Err(err) = rustyedit_io::save_settings(&self.settings) {
                self.status_message = Some(format!("Couldn't save settings: {err}").into());
            }
            cx.notify();
        }
    }
}
