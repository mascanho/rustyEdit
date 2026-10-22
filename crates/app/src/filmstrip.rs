use gpui::{
    canvas, div, img, linear_color_stop, linear_gradient, prelude::*, px, AnyElement, Context,
    ObjectFit, ScrollWheelEvent, SharedString,
};
use rustyedit_core::{
    settings::{FilmstripPosition, FilmstripSettings},
    Asset, Flag,
};
use rustyedit_ui::{FilmLook, Sprockets, Theme};

use crate::workspace::Workspace;

// Dimensions of one frame of "film". Frames butt up against each other with
// no gap so the sprocket holes run continuously along the whole strip.
// The picture height is user-resizable; the frame keeps this aspect to it.
const FRAME_ASPECT: f32 = 132.0 / 80.0;
const SPROCKET_ROW_HEIGHT: f32 = 14.0;
const EDGE_PRINT_HEIGHT: f32 = 12.0;
const META_ROW_HEIGHT: f32 = 16.0;
const ARROW_WIDTH: f32 = 30.0;
const HEADER_HEIGHT: f32 = 20.0;

/// Fraction of the visible strip width that one arrow click scrolls by, so
/// a frame or two of context stays on screen after paging.
const PAGE_FRACTION: f32 = 0.8;

/// Extra frames rendered past each edge of the viewport so fast scrolling
/// never reveals an empty gap before the next render.
const OVERSCAN_FRAMES: usize = 2;

/// Viewport width assumed before the strip has been laid out once. Only
/// affects the very first frame; generous so it never under-renders.
const FALLBACK_VIEWPORT_WIDTH: f32 = 4096.0;

/// Pixels scrolled per "line" for mouse wheels that report line deltas.
const WHEEL_LINE_PX: f32 = 40.0;

const EDGE_PRINT_FONT: &str = "Menlo";

/// Everything in a frame that doesn't scale with the picture.
pub(crate) const FILM_FIXED_HEIGHT: f32 =
    SPROCKET_ROW_HEIGHT * 2.0 + EDGE_PRINT_HEIGHT + META_ROW_HEIGHT + 12.0;
pub(crate) const FILM_PICTURE_MIN: f32 = 48.0;
pub(crate) const FILM_PICTURE_MAX: f32 = 260.0;

impl Workspace {
    fn film_picture_height(&self) -> f32 {
        (self.settings.layout.filmstrip_height as f32).clamp(FILM_PICTURE_MIN, FILM_PICTURE_MAX)
    }

    fn film_frame_width(&self) -> f32 {
        (self.film_picture_height() * FRAME_ASPECT).round()
    }

    /// Height of the roll (excluding the header).
    pub(crate) fn film_roll_height(&self) -> f32 {
        FILM_FIXED_HEIGHT + self.film_picture_height()
    }

    /// A horizontally scrolling strip of every photo in the catalog, drawn
    /// in the user's chosen [`FilmLook`] (colour negative by default:
    /// sprocket holes, amber edge-print frame numbers), with each frame's
    /// rating, flag and colour label beneath the picture. Arrow buttons at
    /// either end page through the roll.
    pub(crate) fn render_filmstrip(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let at_top = self.settings.layout.filmstrip_position == FilmstripPosition::Top;
        let header = self.render_filmstrip_header(at_top, cx);
        let container = div().relative().flex().flex_col().flex_shrink_0().w_full();

        if self.filmstrip_collapsed {
            return container.child(header).into_any_element();
        }
        // The header always sits on the edge facing the photo, so it reads
        // as the strip's "handle" whichever side the strip is docked to.
        let roll = self.render_film_roll(cx);
        let grip = self.resize_grip(crate::resize::ResizeTarget::Filmstrip, cx);
        if at_top {
            container
                .child(roll)
                .child(header)
                .child(grip)
                .into_any_element()
        } else {
            container
                .child(header)
                .child(roll)
                .child(grip)
                .into_any_element()
        }
    }

    pub(crate) fn film_look(&self) -> FilmLook {
        FilmLook::for_style(self.settings.filmstrip.style, &self.theme)
    }

    /// Stacks `main` with the filmstrip on whichever edge the layout
    /// settings choose. Shared by Library and Develop.
    pub(crate) fn with_filmstrip(
        &mut self,
        main: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let filmstrip = self.render_filmstrip(cx);
        let column = div().flex().flex_col().flex_1().min_h(px(0.0));
        match self.settings.layout.filmstrip_position {
            FilmstripPosition::Bottom => column.child(main).child(filmstrip),
            FilmstripPosition::Top => column.child(filmstrip).child(main),
        }
        .into_any_element()
    }

    /// Thin bar above the roll that stays visible when the filmstrip is
    /// collapsed; clicking anywhere on it toggles the strip.
    fn render_filmstrip_header(&self, at_top: bool, cx: &mut Context<Self>) -> AnyElement {
        let look = self.film_look();
        let count = self.catalog.assets.len();
        let position = if count == 0 {
            String::new()
        } else {
            format!("{} / {count}", self.catalog.selected + 1)
        };

        div()
            .id("filmstrip-header")
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .h(px(HEADER_HEIGHT))
            .px_3()
            .bg(look.chrome_bg)
            .when(at_top, |el| el.border_b_1())
            .when(!at_top, |el| el.border_t_1())
            .border_color(look.divider)
            .when(look.monospace, |el| el.font_family(EDGE_PRINT_FONT))
            .text_size(px(9.0))
            .text_color(look.chrome_text)
            .cursor_pointer()
            .hover(|el| el.bg(look.chrome_hover))
            .child(look.title)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(SharedString::from(position))
                    // Points the way the strip will move when clicked.
                    .child(if self.filmstrip_collapsed != at_top {
                        "\u{25B4}"
                    } else {
                        "\u{25BE}"
                    }),
            )
            .on_click(cx.listener(|this, _, _window, cx| {
                this.filmstrip_collapsed = !this.filmstrip_collapsed;
                if !this.filmstrip_collapsed {
                    this.reveal_in_filmstrip(this.catalog.selected);
                }
                cx.notify();
            }))
            .into_any_element()
    }

    /// The roll is virtualized: only the frames inside the viewport (plus a
    /// little overscan) are built, positioned relative to the viewport. The
    /// scroll position is kept as an `f64` in `filmstrip_scroll_x` rather
    /// than in a GPUI scroll container, because a container wide enough for
    /// hundreds of thousands of frames exceeds what `f32` layout can place
    /// precisely.
    fn render_film_roll(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let look = self.film_look();
        let entity = cx.entity();
        let selected = self.catalog.selected;

        self.clamp_filmstrip_scroll();
        let prefs = &self.settings.filmstrip;
        let scroll_x = self.filmstrip_scroll_x;
        let viewport = self.filmstrip_viewport_width() as f64;
        let frame_px = self.film_frame_width();
        let frame_width = frame_px as f64;
        // Show the same photos as the grid: filters and collapsed stacks
        // apply to the filmstrip too.
        let visible = self.catalog.visible_indices();
        let count = visible.len();

        let first = ((scroll_x / frame_width).floor() as usize)
            .saturating_sub(OVERSCAN_FRAMES)
            .min(count);
        let last =
            (((scroll_x + viewport) / frame_width).ceil() as usize + OVERSCAN_FRAMES).min(count);

        let frames: Vec<AnyElement> = (first..last)
            .map(|ix| {
                let entity = entity.clone();
                let asset_ix = visible[ix];
                let left = (ix as f64 * frame_width - scroll_x) as f32;
                film_frame(
                    asset_ix,
                    &self.catalog.assets[asset_ix],
                    asset_ix == selected || self.in_selection(asset_ix),
                    &theme,
                    &look,
                    prefs,
                    frame_px,
                )
                .absolute()
                .top_0()
                .left(px(left))
                .on_mouse_down(
                    gpui::MouseButton::Right,
                    Self::right_click_listener(entity.clone(), asset_ix),
                )
                .on_click(move |event: &gpui::ClickEvent, _window, cx| {
                    // Shift-click selects a range, ⌘-click toggles one.
                    entity.update(cx, |this, cx| {
                        this.click_select(asset_ix, event.modifiers(), cx)
                    });
                })
                .into_any_element()
            })
            .collect();

        let can_scroll_left = scroll_x > 0.0;
        let can_scroll_right = scroll_x < self.filmstrip_max_scroll();

        // Records the roll's on-screen width each frame so the visible range
        // and scroll clamping know how much fits.
        let viewport_cell = self.filmstrip_viewport.clone();
        let measure = canvas(
            move |bounds, _window, _cx| viewport_cell.set(f32::from(bounds.size.width)),
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();

        let roll = div()
            .id("filmstrip")
            .relative()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .overflow_hidden()
            .child(measure)
            .children(frames)
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _window, cx| {
                let delta = event.delta.pixel_delta(px(WHEEL_LINE_PX));
                // Plain vertical mouse wheels scroll the strip sideways too.
                let dx = if delta.x != px(0.0) { delta.x } else { delta.y };
                this.filmstrip_scroll_x -= f32::from(dx) as f64;
                this.clamp_filmstrip_scroll();
                cx.notify();
            }));

        div()
            .flex()
            .flex_row()
            .flex_shrink_0()
            .w_full()
            .h(px(self.film_roll_height()))
            .bg(linear_gradient(
                180.0,
                linear_color_stop(look.base_top, 0.0),
                linear_color_stop(look.base_bottom, 1.0),
            ))
            .child(self.filmstrip_arrow(
                "filmstrip-prev",
                "\u{25C0}",
                "Scroll left",
                -1.0,
                can_scroll_left,
                cx,
            ))
            .child(roll)
            .child(self.filmstrip_arrow(
                "filmstrip-next",
                "\u{25B6}",
                "Scroll right",
                1.0,
                can_scroll_right,
                cx,
            ))
            .into_any_element()
    }

    fn filmstrip_viewport_width(&self) -> f32 {
        match self.filmstrip_viewport.get() {
            w if w > 0.0 => w,
            _ => FALLBACK_VIEWPORT_WIDTH,
        }
    }

    fn filmstrip_max_scroll(&self) -> f64 {
        let content = self.catalog.visible_indices().len() as f64 * self.film_frame_width() as f64;
        (content - self.filmstrip_viewport_width() as f64).max(0.0)
    }

    fn clamp_filmstrip_scroll(&mut self) {
        self.filmstrip_scroll_x = self
            .filmstrip_scroll_x
            .clamp(0.0, self.filmstrip_max_scroll());
    }

    /// Scrolls the minimum amount needed to bring frame `ix` fully into view.
    pub(crate) fn reveal_in_filmstrip(&mut self, ix: usize) {
        let Some(position) = self.catalog.visible_position(ix) else {
            return;
        };
        let frame_width = self.film_frame_width() as f64;
        let viewport = self.filmstrip_viewport_width() as f64;
        let left = position as f64 * frame_width;
        let right = left + frame_width;

        if left < self.filmstrip_scroll_x {
            self.filmstrip_scroll_x = left;
        } else if right > self.filmstrip_scroll_x + viewport {
            self.filmstrip_scroll_x = right - viewport;
        }
        self.clamp_filmstrip_scroll();
    }

    fn filmstrip_arrow(
        &self,
        id: &'static str,
        glyph: &'static str,
        tooltip: &'static str,
        direction: f32,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = self.film_look();

        div()
            .id(id)
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .w(px(ARROW_WIDTH))
            .h_full()
            .bg(look.chrome_bg)
            .text_size(px(12.0))
            .when(direction < 0.0, |el| el.border_r_1())
            .when(direction > 0.0, |el| el.border_l_1())
            .border_color(look.divider)
            .when(enabled, |el| {
                el.text_color(look.chrome_text)
                    .cursor_pointer()
                    .hover(|el| el.bg(look.chrome_hover).text_color(look.selected))
            })
            .when(!enabled, |el| el.text_color(look.chrome_text.opacity(0.25)))
            .child(glyph)
            .tooltip(rustyedit_ui::tip(tooltip, &self.theme))
            .on_click(cx.listener(move |this, _, _window, cx| {
                this.scroll_filmstrip_page(direction, cx);
            }))
            .into_any_element()
    }

    /// Pages the filmstrip left (`direction < 0`) or right (`direction > 0`)
    /// by most of its visible width, clamped to the ends of the roll.
    fn scroll_filmstrip_page(&mut self, direction: f32, cx: &mut Context<Self>) {
        let page = (self.filmstrip_viewport_width() * PAGE_FRACTION) as f64;
        self.filmstrip_scroll_x += page * direction as f64;
        self.clamp_filmstrip_scroll();
        cx.notify();
    }
}

/// One frame of the strip, structured by `look`: optional sprocket rows
/// and edge print, the picture (optionally inside a print-style mat), and a
/// meta row with stars, colour label and flag. The picture takes whatever
/// height the optional pieces leave, so every style fills the strip.
fn film_frame(
    ix: usize,
    asset: &Asset,
    is_selected: bool,
    theme: &Theme,
    look: &FilmLook,
    prefs: &FilmstripSettings,
    frame_width: f32,
) -> gpui::Stateful<gpui::Div> {
    let theme = *theme;
    let look = *look;
    let frame_number = ix + 1;
    let label_color = theme
        .color_label(asset.color_label)
        .filter(|_| prefs.show_color_labels);

    let image: AnyElement = match asset.thumbnail_path.as_ref() {
        Some(path) => img(path.clone())
            .object_fit(ObjectFit::Cover)
            .grayscale(look.monochrome)
            .size_full()
            .into_any_element(),
        None => div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_xs()
            .text_color(look.meta_text.opacity(0.5))
            .child("…")
            .into_any_element(),
    };

    let picture = div()
        .relative()
        .flex_1()
        .min_h(px(0.0))
        .mx(px(look.picture_inset))
        .rounded(px(look.picture_radius))
        .overflow_hidden()
        .bg(look.picture_border)
        .border_2()
        .border_color(if is_selected {
            look.selected
        } else {
            look.picture_border
        })
        .child(image)
        .when_some(asset.copy_name.clone(), |el, name| {
            el.child(crate::grid::copy_badge(name, &theme))
        })
        .when_some(label_color, |el, color| {
            el.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(4.0))
                    .bg(color),
            )
        });

    let meta = div()
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .h(px(META_ROW_HEIGHT))
        .px(px(if look.mat.is_some() { 2.0 } else { 8.0 }))
        .child(div().when(prefs.show_ratings, |el| {
            el.child(film_stars(asset.rating.value(), &look))
        }))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .when_some(label_color, |el, color| {
                    el.child(div().size(px(7.0)).rounded_full().bg(color))
                })
                .child(rustyedit_ui::flag_badge(asset.flag, &theme)),
        );

    // Instant prints wrap picture and caption in a white border with a
    // deeper "chin" at the bottom; film styles lay them out directly.
    let body: AnyElement = match look.mat {
        Some(mat) => div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .mx(px(8.0))
            .my(px(8.0))
            .pt(px(5.0))
            .px(px(5.0))
            .rounded(px(2.0))
            .bg(mat)
            .shadow_md()
            .border_2()
            .border_color(if is_selected { look.selected } else { mat })
            .child(picture)
            .child(meta)
            .into_any_element(),
        None => div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .child(picture)
            .child(meta)
            .into_any_element(),
    };

    let edge_print = look
        .edge_print
        .filter(|_| prefs.show_edge_print)
        .map(|edge| {
            let (left, right) = (edge.label)(frame_number);
            div()
                .flex()
                .flex_row()
                .flex_shrink_0()
                .justify_between()
                .h(px(EDGE_PRINT_HEIGHT))
                .px(px(8.0))
                .when(look.monospace, |el| el.font_family(EDGE_PRINT_FONT))
                .text_size(px(8.0))
                .text_color(if is_selected {
                    edge.selected_color
                } else {
                    edge.color
                })
                .child(SharedString::from(left))
                .child(SharedString::from(right))
        });

    div()
        .id(("filmstrip-cell", ix))
        .flex()
        .flex_col()
        .flex_shrink_0()
        .w(px(frame_width))
        .h_full()
        .cursor_pointer()
        .when(asset.flag == Flag::Rejected, |el| el.opacity(0.4))
        .when(is_selected, |el| el.bg(look.frame_selected_bg))
        .hover(|el| el.bg(look.frame_hover_bg))
        .when_some(look.frame_line, |el, color| {
            el.border_r_1().border_color(color)
        })
        .when(look.sprockets.is_none(), |el| el.py(px(4.0)))
        .children(look.sprockets.map(sprocket_row))
        .children(edge_print)
        .child(body)
        .children(look.sprockets.map(sprocket_row))
}

/// A row of perforations, evenly spaced across a frame.
fn sprocket_row(sprockets: Sprockets) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .justify_around()
        .h(px(SPROCKET_ROW_HEIGHT))
        .children((0..sprockets.per_frame).map(move |_| {
            div()
                .w(px(sprockets.width))
                .h(px(sprockets.height))
                .rounded(px(sprockets.radius))
                .bg(sprockets.color)
        }))
}

/// Compact five-star rating printed beneath the picture.
fn film_stars(value: u8, look: &FilmLook) -> impl IntoElement {
    let look = *look;
    div()
        .flex()
        .flex_row()
        .text_size(px(10.0))
        .children((1..=5u8).map(move |star| {
            let filled = star <= value;
            div()
                .text_color(if filled {
                    look.star_filled
                } else {
                    look.star_empty
                })
                .child(if filled { "\u{2605}" } else { "\u{2606}" })
        }))
}
