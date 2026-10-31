//! How a zine's pages look on screen: framed photos (drawn by
//! `zine_compose`, so mat, shadow, rounded corners and tilt match the PDF),
//! text, covers, and whole pages for the preview and the thumbnails.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use gpui::{
    div, img, prelude::*, px, relative, AnyElement, Hsla, ObjectFit, RenderImage, SharedString,
    Window,
};
use image::RgbImage;
use rustyedit_core::{
    zine_ink_on, zine_photo_geometry, ZineCell, ZineContent, ZineFont, ZineFrameStyle,
    ZinePhoto, ZinePage, ZineRect, ZineTextAlign, ZineTextBox, CAPTION_MM,
};

use crate::{
    workspace::Workspace,
    zine_compose::{compose, to_bgra, ComposeSpec},
};

/// Millimetres in a point.
const MM_PER_PT: f32 = 25.4 / 72.0;

/// Decoded thumbnails and the pictures made from them, kept between
/// frames so a photo is only drawn again when it changes.
#[derive(Default)]
pub(crate) struct ComposeCache {
    thumbs: HashMap<PathBuf, Option<Arc<RgbImage>>>,
    composed: HashMap<String, Composed>,
}

struct Composed {
    image: Arc<RenderImage>,
    size: (u32, u32),
    /// Where the picture's top-left is, from the frame's top-left.
    offset_mm: (f32, f32),
    used: bool,
}

impl ComposeCache {
    /// Lets go of the pictures nobody drew this frame.
    pub(crate) fn prune(&mut self, window: &mut Window) {
        self.composed.retain(|_, c| {
            if c.used {
                c.used = false;
                true
            } else {
                let _ = window.drop_image(c.image.clone());
                false
            }
        });
    }

    fn thumb(&mut self, path: &PathBuf) -> Option<Arc<RgbImage>> {
        self.thumbs
            .entry(path.clone())
            .or_insert_with(|| image::open(path).ok().map(|i| Arc::new(i.to_rgb8())))
            .clone()
    }
}

/// A page on screen: its size in millimetres and in pixels.
#[derive(Clone, Copy)]
pub(crate) struct Stage {
    pub(crate) size_mm: (f32, f32),
    pub(crate) width_px: f32,
}

impl Stage {
    pub(crate) fn px_per_mm(&self) -> f32 {
        self.width_px / self.size_mm.0
    }

    pub(crate) fn height_px(&self) -> f32 {
        self.size_mm.1 * self.px_per_mm()
    }
}

fn hsla(rgb: [u8; 3]) -> Hsla {
    gpui::rgb(u32::from(rgb[0]) << 16 | u32::from(rgb[1]) << 8 | u32::from(rgb[2])).into()
}

fn family(font: ZineFont) -> SharedString {
    if cfg!(target_os = "windows") {
        match font {
            ZineFont::Sans => "Arial",
            ZineFont::Serif => "Times New Roman",
            ZineFont::Mono => "Courier New",
        }
    } else {
        match font {
            ZineFont::Sans => "Helvetica",
            ZineFont::Serif => "Times New Roman",
            ZineFont::Mono => "Courier New",
        }
    }
    .into()
}

impl Workspace {
    /// The decoded thumbnail of a photo of the zine.
    fn zine_thumb_image(&self, photo: &ZinePhoto) -> Option<Arc<RgbImage>> {
        let path = self.zine_thumb(photo)?;
        self.zine_ui.cache.borrow_mut().thumb(&path)
    }

    /// The shape of a photo of the zine, in pixels.
    pub(crate) fn zine_photo_size(&self, photo: &ZinePhoto) -> Option<(u32, u32)> {
        let image = self.zine_thumb_image(photo)?;
        Some((image.width(), image.height()))
    }

    /// A framed photo, drawn at `rect` (millimetres) with a turn of
    /// `rotation` degrees. `None` while its thumbnail is not ready.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn framed_photo(
        &self,
        photo: &ZinePhoto,
        rect: ZineRect,
        frame: &ZineFrameStyle,
        rotation: f32,
        caption_room: f32,
        stage: Stage,
    ) -> Option<(AnyElement, ZineRect)> {
        let thumb = self.zine_thumb_image(photo)?;
        let geometry = zine_photo_geometry(rect, frame, (thumb.width(), thumb.height()), caption_room);
        let s = stage.px_per_mm();
        let spec = ComposeSpec {
            outer: geometry.outer,
            inner: geometry.inner,
            image: geometry.image,
            outer_radius: geometry.outer_radius,
            inner_radius: geometry.inner_radius,
            mat: (frame.mat_mm > 0.0).then_some(frame.mat),
            shadow: frame.shadow,
            rotation,
            centre: (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0),
            px_per_mm: s,
        };
        let key = format!(
            "{:?}|{}x{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            photo,
            thumb.width(),
            thumb.height(),
            spec.outer.w.to_bits(),
            spec.outer.h.to_bits(),
            frame.focus,
            (frame.fit, frame.mat_mm.to_bits(), frame.radius_mm.to_bits()),
            (frame.mat, frame.shadow),
            rotation.to_bits(),
            s.to_bits(),
            (spec.image.x - spec.inner.x, spec.image.y - spec.inner.y, spec.image.w, spec.image.h)
                .map_bits(),
            (spec.inner.w.to_bits(), spec.inner.h.to_bits()),
        );
        let (image, size, origin) = {
            let mut cache = self.zine_ui.cache.borrow_mut();
            if let Some(hit) = cache.composed.get_mut(&key) {
                hit.used = true;
                (hit.image.clone(), hit.size, hit.offset_mm)
            } else {
                let made = compose(&thumb, &spec);
                let size = (made.image.width(), made.image.height());
                let image = Arc::new(RenderImage::new(vec![image::Frame::new(to_bgra(made.image))]));
                cache.composed.insert(
                    key,
                    Composed {
                        image: image.clone(),
                        size,
                        // Kept relative to the frame, so the picture can be
                        // reused wherever the frame is moved to.
                        offset_mm: (made.origin_mm.0 - rect.x, made.origin_mm.1 - rect.y),
                        used: true,
                    },
                );
                (image, size, (made.origin_mm.0 - rect.x, made.origin_mm.1 - rect.y))
            }
        };
        let element = div()
            .absolute()
            .left(px((rect.x + origin.0) * s))
            .top(px((rect.y + origin.1) * s))
            .w(px(size.0 as f32))
            .h(px(size.1 as f32))
            .child(img(image).size_full())
            .into_any_element();
        Some((element, geometry.outer))
    }

    /// A box of text.
    pub(crate) fn text_visual(&self, text: &ZineTextBox, rect: ZineRect, stage: Stage, ink: [u8; 3]) -> AnyElement {
        let s = stage.px_per_mm();
        let colour = text.colour.unwrap_or(ink);
        div()
            .absolute()
            .left(px(rect.x * s))
            .top(px(rect.y * s))
            .w(px(rect.w * s))
            .h(px(rect.h * s))
            .overflow_hidden()
            .p(px(1.5 * s))
            .when_some(text.backdrop, |el, b| el.bg(hsla(b).opacity(0.5)))
            .font_family(family(text.font))
            .text_size(px(text.size_pt * MM_PER_PT * s))
            .line_height(relative(1.25))
            .when(text.bold, |el| el.font_weight(gpui::FontWeight::BOLD))
            .text_color(hsla(colour))
            .map(|el| match text.align {
                ZineTextAlign::Left => el.text_left(),
                ZineTextAlign::Centre => el.text_center(),
                ZineTextAlign::Right => el.text_right(),
            })
            .child(text.text.clone())
            .into_any_element()
    }

    /// One cell of a page as it prints.
    pub(crate) fn cell_visual(&self, cell: &ZineCell, stage: Stage, ink: [u8; 3]) -> Vec<AnyElement> {
        let options = &self.zine.options;
        let r = cell.rect;
        let s = stage.px_per_mm();
        let folded = cell.rotated;
        let scale = if folded { 0.5 } else { 1.0 };
        let caption = |text: String, outer: ZineRect, room: f32| -> AnyElement {
            div()
                .absolute()
                .left(px(r.x * s))
                .top(px((outer.y + outer.h) * s))
                .w(px(r.w * s))
                .h(px(room * s))
                .flex()
                .justify_center()
                .font_family(family(options.font))
                .text_size(px(8.0 * scale * MM_PER_PT * s))
                .text_color(hsla(ink).opacity(0.75))
                .child(text)
                .into_any_element()
        };
        match cell.content {
            ZineContent::Photo(n) => {
                let Some(photo) = self.zine.photos.get(n) else {
                    return Vec::new();
                };
                let room = if options.captions { CAPTION_MM * scale } else { 0.0 };
                let rotation = cell.rotation + if folded { 180.0 } else { 0.0 };
                let Some((element, outer)) = self.framed_photo(photo, r, &cell.frame, rotation, room, stage) else {
                    return vec![self.photo_loading(r, stage)];
                };
                let mut out = vec![element];
                if options.captions && !folded {
                    let name = photo
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    out.push(caption(name, outer, room));
                }
                out
            }
            ZineContent::Cover { photo } => {
                let band = (r.h * 0.25).min(34.0);
                let area = ZineRect { h: (r.h - band).max(1.0), ..r };
                let mut out = Vec::new();
                if let Some(photo) = photo.and_then(|n| self.zine.photos.get(n)) {
                    if let Some((element, _)) =
                        self.framed_photo(photo, area, &ZineFrameStyle::default(), 0.0, 0.0, stage)
                    {
                        out.push(element);
                    }
                }
                let mut title = ZineTextBox::new(options.title.trim(), options.font);
                title.size_pt = 30.0;
                title.bold = true;
                title.align = ZineTextAlign::Centre;
                out.push(self.text_visual(&title, ZineRect { y: r.y + r.h - band, h: band * 0.6, ..r }, stage, ink));
                let author = options.author.trim();
                if !author.is_empty() {
                    let mut by = ZineTextBox::new(author, options.font);
                    by.size_pt = 13.0;
                    by.align = ZineTextAlign::Centre;
                    out.push(self.text_visual(&by, ZineRect { y: r.y + r.h - band * 0.4, h: band * 0.4, ..r }, stage, ink));
                }
                out
            }
            ZineContent::Back => {
                let mut title = ZineTextBox::new(options.title.trim(), options.font);
                title.size_pt = 16.0 * scale;
                title.bold = true;
                title.align = ZineTextAlign::Centre;
                let mut out = vec![self.text_visual(&title, ZineRect { y: r.y + r.h / 2.0 - 8.0, h: 10.0, ..r }, stage, ink)];
                let author = options.author.trim();
                if !author.is_empty() {
                    let mut by = ZineTextBox::new(author, options.font);
                    by.size_pt = 11.0 * scale;
                    by.align = ZineTextAlign::Centre;
                    out.push(self.text_visual(&by, ZineRect { y: r.y + r.h / 2.0 + 2.0, h: 8.0, ..r }, stage, ink));
                }
                out
            }
            ZineContent::Text => cell
                .text
                .as_ref()
                .map(|t| vec![self.text_visual(t, r, stage, ink)])
                .unwrap_or_default(),
        }
    }

    /// A grey box standing in for a photo whose thumbnail is on its way.
    fn photo_loading(&self, r: ZineRect, stage: Stage) -> AnyElement {
        let s = stage.px_per_mm();
        div()
            .absolute()
            .left(px(r.x * s))
            .top(px(r.y * s))
            .w(px(r.w * s))
            .h(px(r.h * s))
            .bg(gpui::black().opacity(0.12))
            .into_any_element()
    }

    /// A page of the finished layout, as it will print.
    pub(crate) fn layout_page(&self, page: &ZinePage, stage: Stage) -> AnyElement {
        let background = page.background.unwrap_or(self.zine.options.paper.rgb());
        let ink = zine_ink_on(background);
        let cells = page.cells.iter().flat_map(|cell| self.cell_visual(cell, stage, ink));
        div()
            .relative()
            .flex_shrink_0()
            .w(px(stage.width_px))
            .h(px(stage.height_px()))
            .overflow_hidden()
            .bg(hsla(background))
            .shadow_md()
            .children(cells)
            .into_any_element()
    }

    /// A page drawn small, for the strip of thumbnails: boxes, with the
    /// photos in them, and bars for text.
    pub(crate) fn page_mini(&self, page: &ZinePage, size_mm: (f32, f32), height: f32) -> AnyElement {
        let background = page.background.unwrap_or(self.zine.options.paper.rgb());
        let (w, h) = size_mm;
        let width = height * w / h;
        let cells = page.cells.iter().map(|cell| {
            let r = cell.rect;
            let base = div()
                .absolute()
                .left(relative(r.x / w))
                .top(relative(r.y / h))
                .w(relative(r.w / w))
                .h(relative(r.h / h));
            match cell.content {
                ZineContent::Photo(n) | ZineContent::Cover { photo: Some(n) } => {
                    match self.zine.photos.get(n).and_then(|p| self.zine_thumb(p)) {
                        Some(path) => base
                            .child(img(path).object_fit(ObjectFit::Cover).size_full())
                            .into_any_element(),
                        None => base.bg(gpui::black().opacity(0.2)).into_any_element(),
                    }
                }
                _ => base.bg(gpui::black().opacity(0.25)).h(relative(r.h.min(8.0) / h)).into_any_element(),
            }
        });
        div()
            .relative()
            .flex_shrink_0()
            .w(px(width))
            .h(px(height))
            .overflow_hidden()
            .bg(hsla(background))
            .children(cells)
            .into_any_element()
    }
}

trait MapBits {
    fn map_bits(self) -> [u32; 4];
}

impl MapBits for (f32, f32, f32, f32) {
    fn map_bits(self) -> [u32; 4] {
        [self.0.to_bits(), self.1.to_bits(), self.2.to_bits(), self.3.to_bits()]
    }
}
