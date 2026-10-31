//! Writing a zine: the PDF (pages laid out by `rustyedit_core::layout_pages`)
//! or an HTML page with its pictures, and remembering the zine between
//! runs.

use std::path::{Path, PathBuf};

use image::RgbImage;
use rustyedit_core::{
    Zine, ZineCell, ZineContent, ZineLayout, ZineOptions, ZinePageLayout, ZineRect, CAPTION_MM,
};

use crate::pdf::{self, JpegImage, PdfPage};

/// A photo ready to go into a zine: its JPEG, and the words under it.
#[derive(Debug, Clone)]
pub struct ZineImage {
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub caption: String,
}

impl ZineImage {
    pub fn encode(image: &RgbImage, quality: u8, caption: String) -> anyhow::Result<Self> {
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality.clamp(1, 100))
            .encode_image(image)?;
        Ok(Self {
            jpeg,
            width: image.width(),
            height: image.height(),
            caption,
        })
    }
}

/// The folder-safe form of a zine's title.
pub fn file_stem(title: &str) -> String {
    let cleaned: String = title
        .trim()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");
    if cleaned.is_empty() {
        "zine".to_string()
    } else {
        cleaned
    }
}

// --- PDF -----------------------------------------------------------------------

use rustyedit_core::{
    zine_ink_on, zine_photo_geometry, ZineFont, ZineTextAlign, ZineTextBox,
};

/// Millimetres in a point.
const MM_PER_PT: f32 = 25.4 / 72.0;

fn family(font: ZineFont) -> pdf::Family {
    match font {
        ZineFont::Sans => pdf::Family::Sans,
        ZineFont::Serif => pdf::Family::Serif,
        ZineFont::Mono => pdf::Family::Mono,
    }
}

/// Writes the zine as a PDF at `path`, its pages as `layout` has them.
/// `images[n]` is photo `n` of the zine (`None` for one that could not be
/// rendered).
pub fn write_pdf(
    path: &Path,
    options: &ZineOptions,
    layout: &ZinePageLayout,
    images: &[Option<ZineImage>],
) -> anyhow::Result<()> {
    // Only the pictures a page uses are embedded, each once.
    let mut slots: Vec<Option<usize>> = vec![None; images.len()];
    let mut embedded: Vec<JpegImage> = Vec::new();
    let mut slot_of = |n: usize| -> Option<usize> {
        let image = images.get(n)?.as_ref()?;
        Some(*slots[n].get_or_insert_with(|| {
            embedded.push(JpegImage {
                data: image.jpeg.clone(),
                width: image.width,
                height: image.height,
            });
            embedded.len() - 1
        }))
    };

    let paper = options.paper.rgb();
    let folded = options.layout == ZineLayout::FoldedSheet
        || layout.pages.iter().any(|p| p.cells.iter().any(|c| c.rotated));
    let mut pages = Vec::new();
    for page in &layout.pages {
        let background = page.background.unwrap_or(paper);
        let ink = zine_ink_on(background);
        let mut out = PdfPage::new(layout.size.0, layout.size.1);
        out.fill_page(background);
        for cell in &page.cells {
            let draw = |out: &mut PdfPage, slot: &mut dyn FnMut(usize) -> Option<usize>| {
                let centre = (
                    cell.rect.x + cell.rect.w / 2.0,
                    cell.rect.y + cell.rect.h / 2.0,
                );
                out.rotated_by(centre, cell.rotation, |out| {
                    draw_cell(out, cell, options, images, slot, ink, folded);
                });
            };
            if cell.rotated {
                let centre = (
                    cell.rect.x + cell.rect.w / 2.0,
                    cell.rect.y + cell.rect.h / 2.0,
                );
                out.rotated_about(centre, |out| draw(out, &mut slot_of));
            } else {
                draw(&mut out, &mut slot_of);
            }
        }
        if folded {
            fold_marks(&mut out, layout.size, ink);
        }
        pages.push(out);
    }
    if pages.is_empty() {
        let mut blank = PdfPage::new(layout.size.0, layout.size.1);
        blank.fill_page(paper);
        pages.push(blank);
    }
    std::fs::write(path, pdf::finish(pages, &embedded, &options.title))?;
    Ok(())
}

/// Where a picture of `image_size` sits, centred and as large as it can be
/// inside `area`.
fn fit_inside(area: ZineRect, image_size: (u32, u32)) -> ZineRect {
    rustyedit_core::zine_fit_rect(area, image_size)
}

#[allow(clippy::too_many_arguments)]
fn draw_cell(
    out: &mut PdfPage,
    cell: &ZineCell,
    options: &ZineOptions,
    images: &[Option<ZineImage>],
    slot: &mut dyn FnMut(usize) -> Option<usize>,
    ink: [u8; 3],
    small_panel: bool,
) {
    let r = cell.rect;
    // Type is set smaller on a folded sheet's panels.
    let scale: f32 = if small_panel { 0.5 } else { 1.0 };
    let body = pdf::Font::new(family(options.font), false);
    let bold = pdf::Font::new(family(options.font), true);
    match cell.content {
        ZineContent::Photo(n) => {
            let caption_room = if options.captions { CAPTION_MM * scale.max(0.7) } else { 0.0 };
            if let (Some(index), Some(image)) = (slot(n), images.get(n).and_then(|i| i.as_ref())) {
                let geometry = zine_photo_geometry(r, &cell.frame, (image.width, image.height), caption_room);
                draw_framed(out, index, &geometry, &cell.frame);
                if options.captions && !image.caption.is_empty() {
                    let size = pdf::fit_text_size(&image.caption, r.w, 8.0 * scale.max(0.8), 5.0, body);
                    out.text(
                        &image.caption,
                        r.x + r.w / 2.0,
                        geometry.outer.y + geometry.outer.h + caption_room * 0.75,
                        size,
                        body,
                        ink,
                        true,
                    );
                }
            }
        }
        ZineContent::Cover { photo } => {
            let band = (r.h * 0.25).min(34.0 * scale.max(0.6));
            let area = ZineRect { h: (r.h - band).max(1.0), ..r };
            if let Some(n) = photo {
                if let (Some(index), Some(image)) = (slot(n), images.get(n).and_then(|i| i.as_ref())) {
                    let at = fit_inside(area, (image.width, image.height));
                    out.image(index, at.x, at.y, at.w, at.h);
                }
            }
            let title = options.title.trim();
            let title_size = pdf::fit_text_size(title, r.w, 30.0 * scale, 8.0, bold);
            let centre = r.x + r.w / 2.0;
            let base = r.y + r.h - band + band * 0.45;
            out.text(title, centre, base, title_size, bold, ink, true);
            let author = options.author.trim();
            if !author.is_empty() {
                let size = pdf::fit_text_size(author, r.w, 13.0 * scale, 6.0, body);
                out.text(author, centre, base + band * 0.38, size, body, ink, true);
            }
        }
        ZineContent::Back => {
            let centre_x = r.x + r.w / 2.0;
            let middle = r.y + r.h / 2.0;
            let title = options.title.trim();
            let size = pdf::fit_text_size(title, r.w, 16.0 * scale, 7.0, bold);
            out.text(title, centre_x, middle, size, bold, ink, true);
            let author = options.author.trim();
            if !author.is_empty() {
                let size = pdf::fit_text_size(author, r.w, 11.0 * scale, 6.0, body);
                out.text(author, centre_x, middle + 7.0 * scale.max(0.7), size, body, ink, true);
            }
            out.text(
                "Made with RustyEdit",
                centre_x,
                r.y + r.h - 2.0,
                6.0,
                body,
                [150, 150, 150],
                true,
            );
        }
        ZineContent::Text => {
            if let Some(text) = &cell.text {
                draw_text_box(out, r, text, ink);
            }
        }
    }
}

/// A photo with its shadow, mat and rounded corners.
fn draw_framed(out: &mut PdfPage, index: usize, g: &rustyedit_core::ZinePhotoGeometry, frame: &rustyedit_core::ZineFrameStyle) {
    if frame.shadow {
        // A soft shadow as a few see-through layers, growing outwards.
        for k in 1..=5 {
            let grow = k as f32 * 0.55;
            let o = g.outer.grown(grow);
            out.fill_rounded_rect(o.x + 0.6, o.y + 1.2, o.w, o.h, g.outer_radius + grow, [0, 0, 0], 0.055);
        }
    }
    if frame.mat_mm > 0.0 {
        let o = g.outer;
        out.fill_rounded_rect(o.x, o.y, o.w, o.h, g.outer_radius, frame.mat, 1.0);
    }
    let clip = g.inner;
    out.clipped_to(clip.x, clip.y, clip.w, clip.h, g.inner_radius, |out| {
        out.image(index, g.image.x, g.image.y, g.image.w, g.image.h);
    });
}

/// A box of text: wrapped to the box, set as it says, cut off at its edge.
fn draw_text_box(out: &mut PdfPage, r: ZineRect, text: &ZineTextBox, ink: [u8; 3]) {
    if let Some(backdrop) = text.backdrop {
        out.fill_rounded_rect(r.x, r.y, r.w, r.h, 0.0, backdrop, 0.5);
    }
    let pad = 1.5;
    let font = pdf::Font::new(family(text.font), text.bold);
    let size = text.size_pt;
    let colour = text.colour.unwrap_or(ink);
    let lines = pdf::wrap_lines(&text.text, (r.w - 2.0 * pad).max(1.0), size, font);
    let line_height = size * 1.25 * MM_PER_PT;
    out.clipped_to(r.x, r.y, r.w, r.h, 0.0, |out| {
        for (i, line) in lines.iter().enumerate() {
            let baseline = r.y + pad + size * 0.9 * MM_PER_PT + i as f32 * line_height;
            let width = pdf::text_width_mm(line, size, font);
            let left = match text.align {
                ZineTextAlign::Left => r.x + pad,
                ZineTextAlign::Centre => r.x + (r.w - width) / 2.0,
                ZineTextAlign::Right => r.x + r.w - pad - width,
            };
            out.text(line, left, baseline, size, font, colour, false);
        }
    });
}

/// Dashed lines where a folded sheet folds (and where it is cut).
fn fold_marks(out: &mut PdfPage, (w, h): (f32, f32), ink: [u8; 3]) {
    let grey = [
        ink[0] / 2 + 64,
        ink[1] / 2 + 64,
        ink[2] / 2 + 64,
    ];
    for i in 1..4 {
        let x = w / 4.0 * i as f32;
        out.dashed_line((x, 0.0), (x, h), grey, 0.3);
    }
    out.dashed_line((0.0, h / 2.0), (w, h / 2.0), grey, 0.3);
}

// --- HTML ----------------------------------------------------------------------

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn css_colour(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

fn css_font(font: ZineFont) -> &'static str {
    match font {
        ZineFont::Sans => "Helvetica,Arial,sans-serif",
        ZineFont::Serif => "'Times New Roman',Times,serif",
        ZineFont::Mono => "'Courier New',Courier,monospace",
    }
}

/// The photo numbers a layout shows, lowest first.
pub fn photos_used(layout: &ZinePageLayout) -> Vec<usize> {
    let mut used: Vec<usize> = layout
        .pages
        .iter()
        .flat_map(|p| &p.cells)
        .filter_map(|c| match c.content {
            ZineContent::Photo(n) => Some(n),
            ZineContent::Cover { photo } => photo,
            ZineContent::Back | ZineContent::Text => None,
        })
        .collect();
    used.sort_unstable();
    used.dedup();
    used
}

/// A picture on the web page: its file (in `images/` beside the page), its
/// caption and its size in pixels.
#[derive(Debug, Clone)]
pub struct WebPhoto {
    pub file: String,
    pub caption: String,
    pub width: u32,
    pub height: u32,
}

/// `value` millimetres as a share of the page's width, in the unit that
/// follows the page's own width (`cqw`).
fn cqw(mm: f32, page_w: f32) -> String {
    format!("{:.3}cqw", mm / page_w * 100.0)
}

fn percent(part: f32, whole: f32) -> String {
    format!("{:.3}%", part / whole.max(0.001) * 100.0)
}

/// The web page for a zine: each page is a sheet laid out as the PDF's is,
/// positioned in percentages so it scales with the window.
pub fn html_document(
    options: &ZineOptions,
    layout: &ZinePageLayout,
    photos: &[Option<WebPhoto>],
) -> String {
    let title = escape_html(options.title.trim());
    let (page_w, page_h) = layout.size;
    let body_font = css_font(options.font);
    let mut html = String::new();
    html.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str(&format!("<title>{title}</title>\n<style>\n"));
    html.push_str(&format!(
        "body{{margin:0;background:#3a3a3a;color:#222;font-family:{body_font}}}\n"
    ));
    html.push_str("main{max-width:1000px;margin:0 auto;padding:24px 12px}\n");
    html.push_str(&format!(
        ".page{{position:relative;width:100%;aspect-ratio:{page_w:.2}/{page_h:.2};container-type:inline-size;\
         margin:0 auto 28px;overflow:hidden;box-shadow:0 4px 24px rgba(0,0,0,.45)}}\n"
    ));
    html.push_str(".cell{position:absolute;box-sizing:border-box}\n");
    html.push_str(".cell img{position:absolute;display:block;max-width:none}\n");
    html.push_str(".clip{position:absolute;overflow:hidden}\n");
    html.push_str(".text{overflow:hidden;white-space:pre-wrap;word-wrap:break-word;line-height:1.25}\n");
    html.push_str(".cap{position:absolute;text-align:center;opacity:.75;white-space:nowrap}\n");
    html.push_str("@media print{body{background:none}.page{break-after:page;box-shadow:none;margin:0}}\n");
    html.push_str("</style>\n</head>\n<body>\n<main>\n");

    let photo = |n: usize| photos.get(n).and_then(|p| p.as_ref());
    for page in &layout.pages {
        // A folded sheet is meant for printing: on the web its panels read
        // in order, one picture after another.
        if page.cells.iter().any(|c| c.rotated) {
            let mut cells: Vec<&ZineCell> = page.cells.iter().collect();
            cells.sort_by_key(|c| match c.content {
                ZineContent::Photo(n) => n,
                _ => usize::MAX,
            });
            for cell in cells {
                if let ZineContent::Photo(n) | ZineContent::Cover { photo: Some(n) } = cell.content {
                    if let Some(p) = photo(n) {
                        html.push_str(&format!(
                            "<section class=\"page\" style=\"aspect-ratio:{}/{};background:{}\"><div class=\"cell\" style=\"inset:3%\">\
                             <img src=\"images/{}\" alt=\"{}\" style=\"width:100%;height:100%;object-fit:contain\"></div></section>\n",
                            p.width.max(1),
                            p.height.max(1),
                            css_colour(page.background.unwrap_or(options.paper.rgb())),
                            escape_html(&p.file),
                            escape_html(&p.caption)
                        ));
                    }
                }
            }
            continue;
        }
        let background = page.background.unwrap_or(options.paper.rgb());
        let ink = zine_ink_on(background);
        html.push_str(&format!(
            "<section class=\"page\" style=\"background:{};color:{}\">\n",
            css_colour(background),
            css_colour(ink)
        ));
        for cell in &page.cells {
            html.push_str(&html_cell(cell, options, layout.size, &photo, ink));
        }
        html.push_str("</section>\n");
    }
    html.push_str("</main>\n</body>\n</html>\n");
    html
}

fn html_cell<'a>(
    cell: &ZineCell,
    options: &ZineOptions,
    (page_w, page_h): (f32, f32),
    photo: &dyn Fn(usize) -> Option<&'a WebPhoto>,
    ink: [u8; 3],
) -> String {
    let r = cell.rect;
    let rotate = |origin: Option<(f32, f32)>| -> String {
        if cell.rotation.abs() < 0.01 {
            return String::new();
        }
        let (ox, oy) = origin.unwrap_or((50.0, 50.0));
        format!("transform:rotate({:.2}deg);transform-origin:{ox:.2}% {oy:.2}%;", cell.rotation)
    };
    let fonts = |size_pt: f32| format!("{:.3}cqw", size_pt * MM_PER_PT / page_w * 100.0);
    match cell.content {
        ZineContent::Photo(n) => {
            let Some(p) = photo(n) else {
                return String::new();
            };
            let caption_room = if options.captions { CAPTION_MM } else { 0.0 };
            let g = zine_photo_geometry(r, &cell.frame, (p.width, p.height), caption_room);
            // Turning about the cell's middle, whichever box is turned.
            let origin = (
                (r.x + r.w / 2.0 - g.outer.x) / g.outer.w.max(0.001) * 100.0,
                (r.y + r.h / 2.0 - g.outer.y) / g.outer.h.max(0.001) * 100.0,
            );
            let mat = if cell.frame.mat_mm > 0.0 {
                format!("background:{};", css_colour(cell.frame.mat))
            } else {
                String::new()
            };
            let shadow = if cell.frame.shadow {
                "box-shadow:.6cqw 1.2cqw 2.2cqw rgba(0,0,0,.28);"
            } else {
                ""
            };
            let inner = g.inner;
            let img = g.image;
            let mut out = format!(
                "<div class=\"cell\" style=\"left:{};top:{};width:{};height:{};{mat}{shadow}border-radius:{};{}\">\n",
                percent(g.outer.x, page_w),
                percent(g.outer.y, page_h),
                percent(g.outer.w, page_w),
                percent(g.outer.h, page_h),
                cqw(g.outer_radius, page_w),
                rotate(Some(origin)),
            );
            out.push_str(&format!(
                "<div class=\"clip\" style=\"left:{};top:{};width:{};height:{};border-radius:{}\">\
                 <img src=\"images/{}\" alt=\"{}\" style=\"left:{};top:{};width:{};height:{}\"></div>\n</div>\n",
                percent(inner.x - g.outer.x, g.outer.w),
                percent(inner.y - g.outer.y, g.outer.h),
                percent(inner.w, g.outer.w),
                percent(inner.h, g.outer.h),
                cqw(g.inner_radius, page_w),
                escape_html(&p.file),
                escape_html(&p.caption),
                percent(img.x - inner.x, inner.w),
                percent(img.y - inner.y, inner.h),
                percent(img.w, inner.w),
                percent(img.h, inner.h),
            ));
            if options.captions && !p.caption.is_empty() {
                out.push_str(&format!(
                    "<div class=\"cap\" style=\"left:{};top:{};width:{};font-size:{}\">{}</div>\n",
                    percent(r.x, page_w),
                    percent(g.outer.y + g.outer.h + caption_room * 0.2, page_h),
                    percent(r.w, page_w),
                    fonts(8.0),
                    escape_html(&p.caption)
                ));
            }
            out
        }
        ZineContent::Cover { photo: hero } => {
            let band = (r.h * 0.25).min(34.0);
            let area = ZineRect { h: (r.h - band).max(1.0), ..r };
            let mut out = String::new();
            if let Some(p) = hero.and_then(photo) {
                let at = fit_inside(area, (p.width, p.height));
                out.push_str(&format!(
                    "<div class=\"cell\" style=\"left:{};top:{};width:{};height:{}\">\
                     <img src=\"images/{}\" alt=\"{}\" style=\"left:0;top:0;width:100%;height:100%\"></div>\n",
                    percent(at.x, page_w),
                    percent(at.y, page_h),
                    percent(at.w, page_w),
                    percent(at.h, page_h),
                    escape_html(&p.file),
                    escape_html(&p.caption),
                ));
            }
            let title = escape_html(options.title.trim());
            let author = escape_html(options.author.trim());
            out.push_str(&format!(
                "<div class=\"cell text\" style=\"left:{};top:{};width:{};height:{};text-align:center;color:{}\">\
                 <div style=\"font-size:{};font-weight:bold\">{title}</div><div style=\"font-size:{};opacity:.8\">{author}</div></div>\n",
                percent(r.x, page_w),
                percent(r.y + r.h - band, page_h),
                percent(r.w, page_w),
                percent(band, page_h),
                css_colour(ink),
                fonts(30.0),
                fonts(13.0),
            ));
            out
        }
        ZineContent::Back => {
            let title = escape_html(options.title.trim());
            let author = escape_html(options.author.trim());
            format!(
                "<div class=\"cell text\" style=\"left:{};top:{};width:{};height:{};text-align:center\">\
                 <div style=\"font-size:{};font-weight:bold\">{title}</div><div style=\"font-size:{}\">{author}</div></div>\n",
                percent(r.x, page_w),
                percent(r.y + r.h / 2.0 - 10.0, page_h),
                percent(r.w, page_w),
                percent(20.0, page_h),
                fonts(16.0),
                fonts(11.0),
            )
        }
        ZineContent::Text => {
            let Some(text) = &cell.text else {
                return String::new();
            };
            let align = match text.align {
                ZineTextAlign::Left => "left",
                ZineTextAlign::Centre => "center",
                ZineTextAlign::Right => "right",
            };
            let backdrop = text
                .backdrop
                .map(|c| format!("background:rgba({},{},{},.5);", c[0], c[1], c[2]))
                .unwrap_or_default();
            format!(
                "<div class=\"cell text\" style=\"left:{};top:{};width:{};height:{};padding:{};{backdrop}\
                 font-family:{};font-size:{};font-weight:{};text-align:{align};color:{};{}\">{}</div>\n",
                percent(r.x, page_w),
                percent(r.y, page_h),
                percent(r.w, page_w),
                percent(r.h, page_h),
                cqw(1.5, page_w),
                css_font(text.font),
                fonts(text.size_pt),
                if text.bold { "bold" } else { "normal" },
                css_colour(text.colour.unwrap_or(ink)),
                rotate(None),
                escape_html(&text.text),
            )
        }
    }
}

/// Writes the zine as `<folder>/index.html` with its pictures in
/// `<folder>/images`. Returns the page's path.
pub fn write_html(
    folder: &Path,
    options: &ZineOptions,
    layout: &ZinePageLayout,
    images: &[Option<ZineImage>],
) -> anyhow::Result<PathBuf> {
    let pictures = folder.join("images");
    std::fs::create_dir_all(&pictures)?;
    let mut photos: Vec<Option<WebPhoto>> = vec![None; images.len()];
    let mut written = 0;
    for n in photos_used(layout) {
        let Some(Some(image)) = images.get(n) else { continue };
        written += 1;
        let name = format!("{written:03}.jpg");
        std::fs::write(pictures.join(&name), &image.jpeg)?;
        photos[n] = Some(WebPhoto {
            file: name,
            caption: image.caption.clone(),
            width: image.width,
            height: image.height,
        });
    }
    let index = folder.join("index.html");
    std::fs::write(&index, html_document(options, layout, &photos))?;
    Ok(index)
}

// --- Remembering ---------------------------------------------------------------

fn store_path() -> Option<PathBuf> {
    crate::settings_path().map(|path| path.with_file_name("zine.json"))
}

/// The zine saved last time (or an empty one).
pub fn load() -> Zine {
    store_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

pub fn save(zine: &Zine) -> anyhow::Result<()> {
    let path = store_path().ok_or_else(|| anyhow::anyhow!("no config directory"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(zine)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyedit_core::layout_pages;

    fn picture(w: u32, h: u32, caption: &str) -> Option<ZineImage> {
        let image = RgbImage::from_pixel(w, h, image::Rgb([90, 120, 200]));
        Some(ZineImage::encode(&image, 80, caption.to_string()).unwrap())
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rustyedit-zine-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn titles_become_safe_file_names() {
        assert_eq!(file_stem("My Zine: #1 / Summer"), "My-Zine-1-Summer");
        assert_eq!(file_stem("  "), "zine");
    }

    #[test]
    fn a_pdf_zine_is_written_for_every_layout() {
        let dir = temp("pdf");
        let images = vec![
            picture(300, 200, "a.jpg"),
            picture(200, 300, "b.jpg"),
            None,
            picture(300, 300, "d.jpg"),
            picture(320, 200, "e.jpg"),
        ];
        for layout in ZineLayout::ALL {
            let options = ZineOptions {
                layout,
                captions: true,
                author: "Someone".into(),
                title: "Summer (1)".into(),
                ..Default::default()
            };
            let path = dir.join(format!("{layout:?}.pdf"));
            let layout = layout_pages(images.len(), &options);
            write_pdf(&path, &options, &layout, &images).unwrap();
            let bytes = std::fs::read(&path).unwrap();
            assert!(bytes.starts_with(b"%PDF-1.4"), "{layout:?}");
            assert!(bytes.windows(5).any(|w| w == b"%%EOF"), "{layout:?}");
            assert!(bytes.windows(9).any(|w| w == b"/DCTDecod"), "{layout:?}");
        }
        // A folded sheet is a single page.
        let folded = std::fs::read(dir.join("FoldedSheet.pdf")).unwrap();
        assert!(String::from_utf8_lossy(&folded).contains("/Count 1 "));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_manual_zine_is_written_with_the_photos_where_they_were_put() {
        use rustyedit_core::{ZineMode, ZinePhoto, ZineSpot};
        let dir = temp("manual");
        let photo = |n: usize| ZinePhoto { path: PathBuf::from(format!("p{n}.jpg")), copy_name: None };
        let mut zine = Zine {
            photos: (0..4).map(photo).collect(),
            options: ZineOptions {
                mode: ZineMode::Manual,
                cover: false,
                captions: true,
                ..Default::default()
            },
            ..Default::default()
        };
        zine.add_page(2);
        zine.add_page(1);
        // Photo 3 alone on the second page, 1 on the first; 0 and 2 unused.
        zine.place(photo(3), ZineSpot::Slot { page: 1, slot: 0 }, None);
        zine.place(photo(1), ZineSpot::Slot { page: 0, slot: 1 }, None);
        let images: Vec<_> = (0..4).map(|n| picture(300, 200, &format!("p{n}.jpg"))).collect();
        let layout = zine.layout();
        assert_eq!(photos_used(&layout), vec![1, 3]);
        write_pdf(&dir.join("m.pdf"), &zine.options, &layout, &images).unwrap();
        let pdf = String::from_utf8_lossy(&std::fs::read(dir.join("m.pdf")).unwrap()).into_owned();
        assert!(pdf.contains("/Count 2 "));
        // Only the two placed pictures are embedded.
        assert_eq!(pdf.matches("/Subtype /Image").count(), 2);

        let index = write_html(&dir.join("web"), &zine.options, &layout, &images).unwrap();
        let html = std::fs::read_to_string(index).unwrap();
        assert_eq!(html.matches("<section").count(), 2);
        assert!(html.contains("p1.jpg") && html.contains("p3.jpg") && !html.contains("p0.jpg"));
        assert!(dir.join("web/images/002.jpg").exists() && !dir.join("web/images/003.jpg").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn photos_placed_freely_are_drawn_where_they_were_put() {
        use rustyedit_core::{ZineMode, ZinePhoto};
        let dir = temp("free");
        let photo = |n: usize| ZinePhoto { path: PathBuf::from(format!("p{n}.jpg")), copy_name: None };
        let mut zine = Zine {
            photos: (0..2).map(photo).collect(),
            options: ZineOptions { mode: ZineMode::Manual, cover: false, ..Default::default() },
            ..Default::default()
        };
        zine.add_free_page();
        zine.free_place(0, photo(0), (0.1, 0.1, 0.5, 0.3));
        zine.free_place(0, photo(1), (0.4, 0.5, 0.5, 0.4));
        let images = vec![picture(300, 200, "a.jpg"), picture(200, 300, "b.jpg")];
        let layout = zine.layout();
        write_pdf(&dir.join("f.pdf"), &zine.options, &layout, &images).unwrap();
        let pdf = String::from_utf8_lossy(&std::fs::read(dir.join("f.pdf")).unwrap()).into_owned();
        assert!(pdf.contains("/Count 1 ") && pdf.matches("/Subtype /Image").count() == 2);
        // The first photo's box starts a tenth in from the left of an A4 page.
        let (w, _) = layout.size;
        assert!((layout.pages[0].cells[0].rect.x - 0.1 * w).abs() < 0.01);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_pictures_fit_keeps_its_shape_and_centres() {
        let area = ZineRect { x: 10.0, y: 10.0, w: 100.0, h: 100.0 };
        let wide = fit_inside(area, (400, 200));
        assert_eq!((wide.w, wide.h), (100.0, 50.0));
        assert_eq!(wide.y, 35.0);
        let tall = fit_inside(area, (200, 400));
        assert_eq!((tall.w, tall.h), (50.0, 100.0));
        assert_eq!(tall.x, 35.0);
    }

    #[test]
    fn an_html_zine_has_a_page_a_picture_folder_and_escaped_text() {
        let dir = temp("html");
        let images = vec![picture(120, 80, "a & b.jpg"), None, picture(80, 120, "c.jpg")];
        let options = ZineOptions {
            layout: ZineLayout::TwoPerPage,
            captions: true,
            title: "Fish <&> Chips".into(),
            ..Default::default()
        };
        let layout = layout_pages(images.len(), &options);
        let index = write_html(&dir, &options, &layout, &images).unwrap();
        let html = std::fs::read_to_string(index).unwrap();
        assert!(html.contains("<title>Fish &lt;&amp;&gt; Chips</title>"));
        assert!(html.contains("src=\"images/001.jpg\""));
        assert!(html.contains("src=\"images/002.jpg\""));
        assert!(html.contains("<div class=\"cap\""));
        assert!(html.contains("alt=\"a &amp; b.jpg\""));
        // The cover is a page, and the two photos share one.
        assert_eq!(html.matches("<section").count(), 2);
        assert!(dir.join("images").join("002.jpg").exists());
        assert!(!dir.join("images").join("003.jpg").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Writes sample zines to /tmp/zine-demo for looking at by eye.
    #[test]
    #[ignore]
    fn demo_zines() {
        let dir = std::path::PathBuf::from("/tmp/zine-demo");
        std::fs::create_dir_all(&dir).unwrap();
        let make = |w: u32, h: u32, hue: u8, name: &str| {
            let image = RgbImage::from_fn(w, h, |x, y| {
                image::Rgb([
                    (x * 255 / w) as u8,
                    (y * 255 / h) as u8,
                    hue.wrapping_add(((x / 40 + y / 40) % 2 * 60) as u8),
                ])
            });
            Some(ZineImage::encode(&image, 85, name.to_string()).unwrap())
        };
        let images: Vec<_> = (0..9)
            .map(|i| {
                let (w, h) = if i % 3 == 0 { (600, 400) } else if i % 3 == 1 { (400, 600) } else { (500, 500) };
                make(w, h, (i * 28) as u8, &format!("DSCF{:04}.RAF", 200 + i))
            })
            .collect();
        for layout in ZineLayout::ALL {
            let options = ZineOptions {
                layout,
                captions: true,
                author: "Marco Guerreiro".into(),
                title: "Summer in Lisbon".into(),
                ..Default::default()
            };
            let pages = layout_pages(images.len(), &options);
            write_pdf(&dir.join(format!("{layout:?}.pdf")), &options, &pages, &images).unwrap();
        }
        let options = ZineOptions { captions: true, author: "Marco".into(), ..Default::default() };
        write_html(&dir.join("web"), &options, &layout_pages(images.len(), &options), &images).unwrap();

        // A hand-made zine with free pages: text, fills, mats, shadows,
        // rounded corners, tilt, a page colour, and a template.
        use rustyedit_core::{ZineFit, ZineFont, ZineMode, ZinePhoto, ZineStyle, ZineTemplate, ZineTextBox};
        let photo = |n: usize| ZinePhoto { path: PathBuf::from(format!("p{n}.jpg")), copy_name: None };
        let mut zine = Zine {
            photos: (0..9).map(photo).collect(),
            options: ZineOptions {
                mode: ZineMode::Manual,
                title: "Summer in Lisbon".into(),
                author: "Marco Guerreiro".into(),
                captions: false,
                ..Default::default()
            },
            ..Default::default()
        };
        zine.apply_style(ZineStyle::Magazine);
        zine.add_free_page();
        zine.free_place(0, photo(0), (0.08, 0.06, 0.55, 0.38));
        zine.free_edit(0, 0, |i| {
            i.frame.fit = ZineFit::Fill;
            i.frame.focus = (0.2, 0.5);
            i.frame.mat_mm = 3.0;
            i.frame.radius_mm = 4.0;
            i.frame.shadow = true;
            i.rotation = -4.0;
        });
        zine.free_place(0, photo(1), (0.55, 0.30, 0.38, 0.40));
        zine.free_edit(0, 1, |i| {
            i.rotation = 9.0;
            i.frame.mat_mm = 5.0;
            i.frame.shadow = true;
        });
        let mut note = ZineTextBox::new("A long afternoon by the river, with nowhere to be and the light doing all the work.", ZineFont::Serif);
        note.size_pt = 16.0;
        zine.free_add_text(0, note, (0.08, 0.56, 0.42, 0.2));
        let mut head = ZineTextBox::new("GOLDEN HOUR", ZineFont::Mono);
        head.size_pt = 26.0;
        head.bold = true;
        head.align = rustyedit_core::ZineTextAlign::Right;
        zine.free_add_text(0, head, (0.4, 0.82, 0.52, 0.1));
        zine.set_page_background(0, Some([238, 226, 200]));
        zine.add_free_page();
        zine.apply_template(1, ZineTemplate::GridQuote);
        zine.add_free_page();
        zine.apply_template(2, ZineTemplate::FullBleed);
        zine.add_free_page();
        zine.apply_template(3, ZineTemplate::HeroCaption);
        let layout = zine.layout();
        for (k, page) in layout.pages.iter().enumerate() {
            let one = ZinePageLayout { pages: vec![page.clone()], ..layout.clone() };
            write_pdf(&dir.join(format!("Rich{k}.pdf")), &zine.options, &one, &images).unwrap();
        }
        write_html(&dir.join("rich-web"), &zine.options, &layout, &images).unwrap();
    }
}
