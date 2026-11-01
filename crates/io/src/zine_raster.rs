//! A picture of each page of a zine, as PNG or JPEG. Pages are drawn as the
//! PDF's are: the same positions, mats, shadows, corners, tilt and text, with
//! the type taken from the computer's own fonts.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use image::RgbImage;
use rustyedit_core::{
    zine_ink_on, zine_photo_geometry, ZineCell, ZineContent, ZineFont, ZineOptions, ZinePageLayout,
    ZineRect, ZineTextAlign, ZineTextBox, CAPTION_MM,
};
use tiny_skia::{
    Color, FillRule, FilterQuality, IntSize, Mask, Paint, Path as SkPath, PathBuilder, Pattern,
    Pixmap, SpreadMode, Stroke, StrokeDash, Transform,
};

use crate::{
    pdf::{self, Family},
    zine::{file_stem, ZineImage},
};

/// Millimetres in a point.
const MM_PER_PT: f32 = 25.4 / 72.0;

/// What a page picture is written as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageImage {
    Png,
    /// With this quality (1..=100).
    Jpeg(u8),
}

// --- Fonts -----------------------------------------------------------------------------------

/// A font file read in, with which face of it to use.
struct LoadedFont {
    data: Vec<u8>,
    index: u32,
}

/// The system fonts the zine's three typefaces map to, found when first
/// needed.
struct Fonts {
    db: fontdb::Database,
    loaded: HashMap<(usize, bool), Option<LoadedFont>>,
    glyphs: HashMap<(usize, bool, u16), Option<SkPath>>,
}

fn family_index(family: Family) -> usize {
    match family {
        Family::Sans => 0,
        Family::Serif => 1,
        Family::Mono => 2,
    }
}

impl Fonts {
    fn new() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        Self {
            db,
            loaded: HashMap::new(),
            glyphs: HashMap::new(),
        }
    }

    fn font(&mut self, family: Family, bold: bool) -> Option<&LoadedFont> {
        let key = (family_index(family), bold);
        if !self.loaded.contains_key(&key) {
            let names: &[&str] = match family {
                Family::Sans => &["Helvetica", "Arial", "Liberation Sans", "DejaVu Sans", "Noto Sans"],
                Family::Serif => &["Times New Roman", "Times", "Liberation Serif", "DejaVu Serif", "Noto Serif"],
                Family::Mono => &["Courier New", "Courier", "Liberation Mono", "DejaVu Sans Mono", "Noto Sans Mono"],
            };
            let families: Vec<fontdb::Family> = names.iter().map(|n| fontdb::Family::Name(n)).collect();
            let query = fontdb::Query {
                families: &families,
                weight: if bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL },
                ..Default::default()
            };
            let found = self.db.query(&query).and_then(|id| {
                let (source, index) = self.db.face_source(id)?;
                let data = match source {
                    fontdb::Source::File(path) => std::fs::read(path).ok()?,
                    fontdb::Source::Binary(bytes) => bytes.as_ref().as_ref().to_vec(),
                    fontdb::Source::SharedFile(path, _) => std::fs::read(path).ok()?,
                };
                Some(LoadedFont { data, index })
            });
            self.loaded.insert(key, found);
        }
        self.loaded.get(&key).and_then(Option::as_ref)
    }

    /// The width of `text` in font units per em: `(advance, upem)`.
    fn advance(&mut self, family: Family, bold: bool, text: &str) -> Option<(f32, f32)> {
        let font = self.font(family, bold)?;
        let face = ttf_parser::Face::parse(&font.data, font.index).ok()?;
        let upem = f32::from(face.units_per_em());
        let total: f32 = text
            .chars()
            .map(|c| {
                face.glyph_index(c)
                    .and_then(|g| face.glyph_hor_advance(g))
                    .map_or(upem * 0.5, f32::from)
            })
            .sum();
        Some((total, upem))
    }
}

/// Collects a glyph's outline as a path.
struct Outline(PathBuilder);

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.cubic_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.0.close();
    }
}

// --- The page ---------------------------------------------------------------------------------

fn colour(rgb: [u8; 3], alpha: f32) -> Color {
    Color::from_rgba8(rgb[0], rgb[1], rgb[2], (alpha.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// A rounded rectangle, in pixels.
fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<SkPath> {
    let r = radius.clamp(0.0, w.min(h) / 2.0);
    let mut b = PathBuilder::new();
    if r < 0.05 {
        b.push_rect(tiny_skia::Rect::from_xywh(x, y, w.max(0.01), h.max(0.01))?);
        return b.finish();
    }
    let k = r * 0.5523;
    b.move_to(x + r, y);
    b.line_to(x + w - r, y);
    b.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    b.line_to(x + w, y + h - r);
    b.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    b.line_to(x + r, y + h);
    b.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    b.line_to(x, y + r);
    b.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    b.close();
    b.finish()
}

struct Painter<'a> {
    options: &'a ZineOptions,
    images: &'a [Option<ZineImage>],
    /// The pictures, read in once.
    pictures: HashMap<usize, Option<Pixmap>>,
    fonts: Fonts,
    /// Pixels in a millimetre.
    s: f32,
}

impl<'a> Painter<'a> {
    fn picture(&mut self, n: usize) -> Option<&Pixmap> {
        if !self.pictures.contains_key(&n) {
            let made = self
                .images
                .get(n)
                .and_then(|i| i.as_ref())
                .and_then(|i| image::load_from_memory(&i.jpeg).ok())
                .and_then(|i| {
                    let rgba = i.to_rgba8();
                    let size = IntSize::from_wh(rgba.width(), rgba.height())?;
                    Pixmap::from_vec(rgba.into_raw(), size)
                });
            self.pictures.insert(n, made);
        }
        self.pictures.get(&n).and_then(Option::as_ref)
    }

    fn fill(&self, pm: &mut Pixmap, path: &SkPath, rgb: [u8; 3], alpha: f32, xf: Transform, mask: Option<&Mask>) {
        let mut paint = Paint::default();
        paint.set_color(colour(rgb, alpha));
        paint.anti_alias = true;
        pm.fill_path(path, &paint, FillRule::Winding, xf, mask);
    }

    /// Picture `n` stretched over `at` (millimetres), cut to `clip`.
    fn draw_picture(&mut self, pm: &mut Pixmap, n: usize, at: ZineRect, clip: &SkPath, xf: Transform) {
        let s = self.s;
        let Some(picture) = self.picture(n) else { return };
        let (w, h) = (picture.width() as f32, picture.height() as f32);
        let fit = Transform::from_row(at.w * s / w, 0.0, 0.0, at.h * s / h, at.x * s, at.y * s);
        let mut paint = Paint::default();
        paint.shader = Pattern::new(picture.as_ref(), SpreadMode::Pad, FilterQuality::Bicubic, 1.0, fit);
        paint.anti_alias = true;
        pm.fill_path(clip, &paint, FillRule::Winding, xf, None);
    }

    /// Text starting at `x` (or centred on it, or ending at it) with its
    /// baseline at `y`, all in millimetres.
    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        pm: &mut Pixmap,
        text: &str,
        x: f32,
        y: f32,
        size_pt: f32,
        font: pdf::Font,
        rgb: [u8; 3],
        align: ZineTextAlign,
        xf: Transform,
        mask: Option<&Mask>,
    ) {
        let s = self.s;
        let px = size_pt * MM_PER_PT * s;
        let (family, bold) = (font.family, font.bold);
        let Some((total, upem)) = self.fonts.advance(family, bold, text) else {
            return;
        };
        let width_mm = total / upem * size_pt * MM_PER_PT;
        let left = match align {
            ZineTextAlign::Left => x,
            ZineTextAlign::Centre => x - width_mm / 2.0,
            ZineTextAlign::Right => x - width_mm,
        };
        let key = (family_index(family), bold);
        let Some(loaded) = self.fonts.loaded.get(&key).and_then(Option::as_ref) else {
            return;
        };
        let Ok(face) = ttf_parser::Face::parse(&loaded.data, loaded.index) else {
            return;
        };
        let scale = px / upem;
        let mut pen = left * s;
        let baseline = y * s;
        let mut paint = Paint::default();
        paint.set_color(colour(rgb, 1.0));
        paint.anti_alias = true;
        for c in text.chars() {
            let Some(id) = face.glyph_index(c) else {
                pen += upem * 0.5 * scale;
                continue;
            };
            let cached = self.fonts.glyphs.contains_key(&(key.0, key.1, id.0));
            if !cached {
                let mut outline = Outline(PathBuilder::new());
                let path = face.outline_glyph(id, &mut outline).and_then(|_| outline.0.finish());
                self.fonts.glyphs.insert((key.0, key.1, id.0), path);
            }
            if let Some(Some(path)) = self.fonts.glyphs.get(&(key.0, key.1, id.0)) {
                // Glyphs point upwards: flip them onto the page.
                let at = Transform::from_row(scale, 0.0, 0.0, -scale, pen, baseline);
                pm.fill_path(path, &paint, FillRule::Winding, at.post_concat(xf), mask);
            }
            pen += f32::from(face.glyph_hor_advance(id).unwrap_or((upem * 0.5) as u16)) * scale;
        }
    }

    fn draw_cell(&mut self, pm: &mut Pixmap, cell: &ZineCell, ink: [u8; 3], folded: bool) {
        let s = self.s;
        let r = cell.rect;
        let angle = cell.rotation + if cell.rotated { 180.0 } else { 0.0 };
        let xf = Transform::from_rotate_at(angle, (r.x + r.w / 2.0) * s, (r.y + r.h / 2.0) * s);
        let scale: f32 = if folded { 0.5 } else { 1.0 };
        let family = |font: ZineFont| match font {
            ZineFont::Sans => Family::Sans,
            ZineFont::Serif => Family::Serif,
            ZineFont::Mono => Family::Mono,
        };
        let body = pdf::Font::new(family(self.options.font), false);
        let bold = pdf::Font::new(family(self.options.font), true);
        let options = self.options;
        match cell.content {
            ZineContent::Photo(n) => {
                let caption_room = if options.captions { CAPTION_MM * scale.max(0.7) } else { 0.0 };
                let Some(image) = self.images.get(n).and_then(|i| i.as_ref()) else {
                    return;
                };
                let (iw, ih, caption) = (image.width, image.height, image.caption.clone());
                let g = zine_photo_geometry(r, &cell.frame, (iw, ih), caption_room);
                if cell.frame.shadow {
                    for k in 1..=5 {
                        let grow = k as f32 * 0.55;
                        let o = g.outer.grown(grow);
                        if let Some(path) = rounded_rect(
                            (o.x + 0.6) * s,
                            (o.y + 1.2) * s,
                            o.w * s,
                            o.h * s,
                            (g.outer_radius + grow) * s,
                        ) {
                            self.fill(pm, &path, [0, 0, 0], 0.055, xf, None);
                        }
                    }
                }
                if cell.frame.mat_mm > 0.0 {
                    if let Some(path) = rounded_rect(g.outer.x * s, g.outer.y * s, g.outer.w * s, g.outer.h * s, g.outer_radius * s) {
                        self.fill(pm, &path, cell.frame.mat, 1.0, xf, None);
                    }
                }
                if let Some(clip) = rounded_rect(g.inner.x * s, g.inner.y * s, g.inner.w * s, g.inner.h * s, g.inner_radius * s) {
                    self.draw_picture(pm, n, g.image, &clip, xf);
                }
                if options.captions && !caption.is_empty() {
                    let size = pdf::fit_text_size(&caption, r.w, 8.0 * scale.max(0.8), 5.0, body);
                    self.text(
                        pm,
                        &caption,
                        r.x + r.w / 2.0,
                        g.outer.y + g.outer.h + caption_room * 0.75,
                        size,
                        body,
                        ink,
                        ZineTextAlign::Centre,
                        xf,
                        None,
                    );
                }
            }
            ZineContent::Cover { photo } => {
                let band = (r.h * 0.25).min(34.0 * scale.max(0.6));
                let area = ZineRect { h: (r.h - band).max(1.0), ..r };
                if let Some(n) = photo {
                    if let Some(image) = self.images.get(n).and_then(|i| i.as_ref()) {
                        let at = rustyedit_core::zine_fit_rect(area, (image.width, image.height));
                        if let Some(clip) = rounded_rect(at.x * s, at.y * s, at.w * s, at.h * s, 0.0) {
                            self.draw_picture(pm, n, at, &clip, xf);
                        }
                    }
                }
                let title = options.title.trim().to_string();
                let size = pdf::fit_text_size(&title, r.w, 30.0 * scale, 8.0, bold);
                let centre = r.x + r.w / 2.0;
                let base = r.y + r.h - band + band * 0.45;
                self.text(pm, &title, centre, base, size, bold, ink, ZineTextAlign::Centre, xf, None);
                let author = options.author.trim().to_string();
                if !author.is_empty() {
                    let size = pdf::fit_text_size(&author, r.w, 13.0 * scale, 6.0, body);
                    self.text(pm, &author, centre, base + band * 0.38, size, body, ink, ZineTextAlign::Centre, xf, None);
                }
            }
            ZineContent::Back => {
                let centre = r.x + r.w / 2.0;
                let middle = r.y + r.h / 2.0;
                let title = options.title.trim().to_string();
                let size = pdf::fit_text_size(&title, r.w, 16.0 * scale, 7.0, bold);
                self.text(pm, &title, centre, middle, size, bold, ink, ZineTextAlign::Centre, xf, None);
                let author = options.author.trim().to_string();
                if !author.is_empty() {
                    let size = pdf::fit_text_size(&author, r.w, 11.0 * scale, 6.0, body);
                    self.text(pm, &author, centre, middle + 7.0 * scale.max(0.7), size, body, ink, ZineTextAlign::Centre, xf, None);
                }
                self.text(pm, "Made with RustyEdit", centre, r.y + r.h - 2.0, 6.0, body, [150, 150, 150], ZineTextAlign::Centre, xf, None);
            }
            ZineContent::Text => {
                if let Some(text) = &cell.text {
                    self.text_box(pm, r, text, ink, xf);
                }
            }
        }
    }

    /// A box of text: wrapped to the box, set as it says, cut off at its edge.
    fn text_box(&mut self, pm: &mut Pixmap, r: ZineRect, text: &ZineTextBox, ink: [u8; 3], xf: Transform) {
        let s = self.s;
        let Some(box_path) = rounded_rect(r.x * s, r.y * s, r.w * s, r.h * s, 0.0) else {
            return;
        };
        if let Some(backdrop) = text.backdrop {
            self.fill(pm, &box_path, backdrop, 0.5, xf, None);
        }
        let mut mask = Mask::new(pm.width(), pm.height());
        let Some(mask) = mask.as_mut().map(|m| {
            m.fill_path(&box_path, FillRule::Winding, true, xf);
            &*m
        }) else {
            return;
        };
        let family = match text.font {
            ZineFont::Sans => Family::Sans,
            ZineFont::Serif => Family::Serif,
            ZineFont::Mono => Family::Mono,
        };
        let font = pdf::Font::new(family, text.bold);
        let pad = 1.5;
        let lines = pdf::wrap_lines(&text.text, (r.w - 2.0 * pad).max(1.0), text.size_pt, font);
        let line_height = text.size_pt * 1.25 * MM_PER_PT;
        let rgb = text.colour.unwrap_or(ink);
        for (i, line) in lines.iter().enumerate() {
            let baseline = r.y + pad + text.size_pt * 0.9 * MM_PER_PT + i as f32 * line_height;
            let x = match text.align {
                ZineTextAlign::Left => r.x + pad,
                ZineTextAlign::Centre => r.x + r.w / 2.0,
                ZineTextAlign::Right => r.x + r.w - pad,
            };
            self.text(pm, line, x, baseline, text.size_pt, font, rgb, text.align, xf, Some(mask));
        }
    }
}

/// A page of the layout as a picture `edge` pixels along its longer side.
fn render_page(
    options: &ZineOptions,
    layout: &ZinePageLayout,
    page: &rustyedit_core::ZinePage,
    images: &[Option<ZineImage>],
    fonts: Fonts,
    edge: u32,
) -> (RgbImage, Fonts) {
    let (w_mm, h_mm) = layout.size;
    let s = edge as f32 / w_mm.max(h_mm);
    let (w, h) = ((w_mm * s).round().max(1.0) as u32, (h_mm * s).round().max(1.0) as u32);
    let mut pm = Pixmap::new(w, h).expect("a page picture has a size");
    let background = page.background.unwrap_or(options.paper.rgb());
    pm.fill(colour(background, 1.0));
    let ink = zine_ink_on(background);
    let mut painter = Painter {
        options,
        images,
        pictures: HashMap::new(),
        fonts,
        s,
    };
    let folded = page.cells.iter().any(|c| c.rotated);
    for cell in &page.cells {
        painter.draw_cell(&mut pm, cell, ink, folded);
    }
    if folded {
        // Dashed lines where a folded sheet folds.
        let grey = [ink[0] / 2 + 64, ink[1] / 2 + 64, ink[2] / 2 + 64];
        let mut paint = Paint::default();
        paint.set_color(colour(grey, 1.0));
        let stroke = Stroke {
            width: (0.3 * s).max(1.0),
            dash: StrokeDash::new(vec![3.0 * s, 3.0 * s], 0.0),
            ..Default::default()
        };
        let mut lines = PathBuilder::new();
        for i in 1..4 {
            let x = w as f32 / 4.0 * i as f32;
            lines.move_to(x, 0.0);
            lines.line_to(x, h as f32);
        }
        lines.move_to(0.0, h as f32 / 2.0);
        lines.line_to(w as f32, h as f32 / 2.0);
        if let Some(path) = lines.finish() {
            pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
    let rgb = RgbImage::from_fn(w, h, |x, y| {
        let p = pm.pixel(x, y).map_or([0, 0, 0, 255], |p| [p.red(), p.green(), p.blue(), p.alpha()]);
        image::Rgb([p[0], p[1], p[2]])
    });
    (rgb, painter.fonts)
}

/// Writes a picture of every page into `folder`, named after the zine and
/// numbered. Returns the files.
pub fn write_page_images(
    folder: &Path,
    options: &ZineOptions,
    layout: &ZinePageLayout,
    images: &[Option<ZineImage>],
    format: PageImage,
) -> anyhow::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(folder)?;
    let stem = file_stem(&options.title);
    let edge = options.image_edge.clamp(400, 6000);
    let digits = layout.pages.len().to_string().len().max(2);
    let mut fonts = Fonts::new();
    let mut written = Vec::new();
    for (n, page) in layout.pages.iter().enumerate() {
        let (picture, back) = render_page(options, layout, page, images, fonts, edge);
        fonts = back;
        let extension = match format {
            PageImage::Png => "png",
            PageImage::Jpeg(_) => "jpg",
        };
        let path = folder.join(format!("{stem}-{:0digits$}.{extension}", n + 1));
        match format {
            PageImage::Png => picture.save(&path)?,
            PageImage::Jpeg(quality) => {
                let file = std::fs::File::create(&path)?;
                let mut out = std::io::BufWriter::new(file);
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100))
                    .encode_image(&picture)?;
            }
        }
        written.push(path);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyedit_core::{
        layout_pages, Zine, ZineFit, ZineFont, ZineLayout, ZineMode, ZinePhoto, ZineTextBox,
    };

    fn solid(w: u32, h: u32, rgb: [u8; 3]) -> Option<ZineImage> {
        let image = RgbImage::from_pixel(w, h, image::Rgb(rgb));
        Some(ZineImage::encode(&image, 95, "a.jpg".to_string()).unwrap())
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rustyedit-raster-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn close(a: image::Rgb<u8>, b: [u8; 3], tolerance: i32) -> bool {
        (0..3).all(|i| (i32::from(a.0[i]) - i32::from(b[i])).abs() <= tolerance)
    }

    #[test]
    fn every_page_is_a_picture_with_the_photo_where_the_layout_puts_it() {
        let dir = temp("pages");
        let options = ZineOptions {
            cover: false,
            layout: ZineLayout::OnePerPage,
            image_edge: 800,
            ..Default::default()
        };
        let images = vec![solid(300, 200, [200, 30, 30]), solid(200, 300, [30, 30, 200])];
        let layout = layout_pages(2, &options);
        let files = write_page_images(&dir, &options, &layout, &images, PageImage::Png).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files[0].file_name().unwrap().to_string_lossy().ends_with("-01.png"));
        let first = image::open(&files[0]).unwrap().to_rgb8();
        // A4 portrait, 800 pixels on the long side.
        assert_eq!(first.height(), 800);
        assert!((first.width() as i32 - 566).abs() <= 1);
        // Paper in the corner (the margin), the red photo in the middle.
        assert!(close(*first.get_pixel(3, 3), [255, 255, 255], 2));
        assert!(close(*first.get_pixel(first.width() / 2, first.height() / 2), [200, 30, 30], 6));
        let second = image::open(&files[1]).unwrap().to_rgb8();
        assert!(close(*second.get_pixel(second.width() / 2, second.height() / 2), [30, 30, 200], 6));
        // As JPEGs too.
        let jpegs = write_page_images(&dir.join("j"), &options, &layout, &images, PageImage::Jpeg(85)).unwrap();
        assert!(jpegs[0].extension().unwrap() == "jpg");
        assert_eq!(image::open(&jpegs[0]).unwrap().to_rgb8().height(), 800);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mats_shadows_corners_tilt_fill_and_text_are_drawn() {
        let dir = temp("styled");
        let photo = |n: usize| ZinePhoto { path: PathBuf::from(format!("p{n}.jpg")), copy_name: None };
        let mut zine = Zine {
            photos: vec![photo(0), photo(1)],
            options: ZineOptions {
                mode: ZineMode::Manual,
                cover: false,
                image_edge: 900,
                ..Default::default()
            },
            ..Default::default()
        };
        zine.add_free_page();
        // A filled, matted, rounded, shadowed photo; a tilted one; text.
        zine.free_place(0, photo(0), (0.1, 0.1, 0.5, 0.3));
        zine.free_edit(0, 0, |i| {
            i.frame.fit = ZineFit::Fill;
            i.frame.mat_mm = 5.0;
            i.frame.radius_mm = 6.0;
            i.frame.shadow = true;
        });
        zine.free_place(0, photo(1), (0.3, 0.5, 0.4, 0.25));
        zine.free_edit(0, 1, |i| i.rotation = 45.0);
        let mut text = ZineTextBox::new("Hello zine", ZineFont::Sans);
        text.size_pt = 40.0;
        text.bold = true;
        zine.free_add_text(0, text, (0.1, 0.85, 0.8, 0.1));
        let images = vec![solid(300, 200, [200, 30, 30]), solid(300, 200, [30, 160, 30])];
        let files = write_page_images(&dir, &zine.options, &zine.layout(), &images, PageImage::Png).unwrap();
        let page = image::open(&files[0]).unwrap().to_rgb8();
        let at = |fx: f32, fy: f32| *page.get_pixel((fx * page.width() as f32) as u32, (fy * page.height() as f32) as u32);
        // The filled photo's middle is red; just inside its edge is the white mat.
        assert!(close(at(0.35, 0.25), [200, 30, 30], 8), "{:?}", at(0.35, 0.25));
        assert!(close(at(0.103, 0.25), [255, 255, 255], 8), "the mat {:?}", at(0.103, 0.25));
        // Beneath it the shadow darkens the paper a little.
        let shadow = at(0.35, 0.405);
        assert!(shadow.0[0] < 252 && shadow.0[0] > 150, "shadow {shadow:?}");
        // The tilted green photo is turned: its middle is green and the
        // corner of its unturned box is paper.
        assert!(close(at(0.5, 0.625), [30, 160, 30], 8));
        assert!(close(at(0.305, 0.505), [255, 255, 255], 8), "corner {:?}", at(0.305, 0.505));
        // The words put dark pixels in their box (when a font was found).
        let ink = (0..page.width())
            .flat_map(|x| (0..page.height()).map(move |y| (x, y)))
            .filter(|&(x, y)| {
                let (fx, fy) = (x as f32 / page.width() as f32, y as f32 / page.height() as f32);
                (0.1..0.9).contains(&fx) && (0.85..0.95).contains(&fy) && page.get_pixel(x, y).0[0] < 100
            })
            .count();
        if ink == 0 {
            eprintln!("no system font found: text not checked");
        } else {
            assert!(ink > 200, "{ink} dark pixels");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
