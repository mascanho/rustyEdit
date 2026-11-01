//! A zine as web pages: a folder with its pictures beside the page, one HTML
//! file with the pictures inside it (to email), or a fixed-layout EPUB.
//! Every page is laid out as the PDF's is.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use rustyedit_core::{
    zine_ink_on, zine_photo_geometry, ZineCell, ZineContent, ZineFont, ZineOptions, ZinePage,
    ZinePageLayout, ZineRect, ZineTextAlign, CAPTION_MM,
};

use crate::{
    zine::{photos_used, ZineImage},
    zip_store::ZipStore,
};

/// Millimetres in a point.
const MM_PER_PT: f32 = 25.4 / 72.0;
/// Pixels in a millimetre on a 96 dpi screen (what an EPUB page is sized in).
const PX_PER_MM: f32 = 96.0 / 25.4;

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

/// A picture on a page: where its source is (a file beside the page, or
/// the picture itself inside the file), its caption and its size in pixels.
#[derive(Debug, Clone)]
pub struct WebPhoto {
    pub src: String,
    pub caption: String,
    pub width: u32,
    pub height: u32,
}

/// How lengths are written: as shares of the page (so it scales with the
/// window), or in pixels (for a page of a fixed size).
#[derive(Debug, Clone, Copy)]
enum Units {
    Responsive { page_w: f32, page_h: f32 },
    Pixels { per_mm: f32 },
}

impl Units {
    fn x(&self, mm: f32) -> String {
        match *self {
            Units::Responsive { page_w, .. } => percent(mm, page_w),
            Units::Pixels { per_mm } => format!("{:.2}px", mm * per_mm),
        }
    }

    fn y(&self, mm: f32) -> String {
        match *self {
            Units::Responsive { page_h, .. } => percent(mm, page_h),
            Units::Pixels { per_mm } => format!("{:.2}px", mm * per_mm),
        }
    }

    /// A length that does not follow either axis: a radius, a padding.
    fn len(&self, mm: f32) -> String {
        match *self {
            Units::Responsive { page_w, .. } => format!("{:.3}cqw", mm / page_w * 100.0),
            Units::Pixels { per_mm } => format!("{:.2}px", mm * per_mm),
        }
    }

    fn font(&self, pt: f32) -> String {
        self.len(pt * MM_PER_PT)
    }
}

fn percent(part: f32, whole: f32) -> String {
    format!("{:.3}%", part / whole.max(0.001) * 100.0)
}

fn fit_inside(area: ZineRect, size: (u32, u32)) -> ZineRect {
    rustyedit_core::zine_fit_rect(area, size)
}

/// The markup of one cell of a page.
fn cell_markup<'a>(
    cell: &ZineCell,
    options: &ZineOptions,
    u: Units,
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
                format!(
                    "box-shadow:{} {} {} rgba(0,0,0,.28);",
                    u.len(0.6),
                    u.len(1.2),
                    u.len(2.2)
                )
            } else {
                String::new()
            };
            let (inner, img) = (g.inner, g.image);
            let mut out = format!(
                "<div class=\"cell\" style=\"left:{};top:{};width:{};height:{};{mat}{shadow}border-radius:{};{}\">\n",
                u.x(g.outer.x),
                u.y(g.outer.y),
                u.x(g.outer.w),
                u.y(g.outer.h),
                u.len(g.outer_radius),
                rotate(Some(origin)),
            );
            out.push_str(&format!(
                "<div class=\"clip\" style=\"left:{};top:{};width:{};height:{};border-radius:{}\">\
                 <img src=\"{}\" alt=\"{}\" style=\"left:{};top:{};width:{};height:{}\"/></div>\n</div>\n",
                percent(inner.x - g.outer.x, g.outer.w),
                percent(inner.y - g.outer.y, g.outer.h),
                percent(inner.w, g.outer.w),
                percent(inner.h, g.outer.h),
                u.len(g.inner_radius),
                escape_html(&p.src),
                escape_html(&p.caption),
                percent(img.x - inner.x, inner.w),
                percent(img.y - inner.y, inner.h),
                percent(img.w, inner.w),
                percent(img.h, inner.h),
            ));
            if options.captions && !p.caption.is_empty() {
                out.push_str(&format!(
                    "<div class=\"cap\" style=\"left:{};top:{};width:{};font-size:{}\">{}</div>\n",
                    u.x(r.x),
                    u.y(g.outer.y + g.outer.h + caption_room * 0.2),
                    u.x(r.w),
                    u.font(8.0),
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
                     <img src=\"{}\" alt=\"{}\" style=\"left:0;top:0;width:100%;height:100%\"/></div>\n",
                    u.x(at.x),
                    u.y(at.y),
                    u.x(at.w),
                    u.y(at.h),
                    escape_html(&p.src),
                    escape_html(&p.caption),
                ));
            }
            let title = escape_html(options.title.trim());
            let author = escape_html(options.author.trim());
            out.push_str(&format!(
                "<div class=\"cell text\" style=\"left:{};top:{};width:{};height:{};text-align:center;color:{};font-family:{}\">\
                 <div style=\"font-size:{};font-weight:bold\">{title}</div><div style=\"font-size:{};opacity:.8\">{author}</div></div>\n",
                u.x(r.x),
                u.y(r.y + r.h - band),
                u.x(r.w),
                u.y(band),
                css_colour(ink),
                css_font(options.font),
                u.font(30.0),
                u.font(13.0),
            ));
            out
        }
        ZineContent::Back => {
            let title = escape_html(options.title.trim());
            let author = escape_html(options.author.trim());
            format!(
                "<div class=\"cell text\" style=\"left:{};top:{};width:{};height:{};text-align:center;font-family:{}\">\
                 <div style=\"font-size:{};font-weight:bold\">{title}</div><div style=\"font-size:{}\">{author}</div></div>\n",
                u.x(r.x),
                u.y(r.y + r.h / 2.0 - 10.0),
                u.x(r.w),
                u.y(20.0),
                css_font(options.font),
                u.font(16.0),
                u.font(11.0),
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
            let _ = (page_w, page_h);
            format!(
                "<div class=\"cell text\" style=\"left:{};top:{};width:{};height:{};padding:{};{backdrop}\
                 font-family:{};font-size:{};font-weight:{};text-align:{align};color:{};{}\">{}</div>\n",
                u.x(r.x),
                u.y(r.y),
                u.x(r.w),
                u.y(r.h),
                u.len(1.5),
                css_font(text.font),
                u.font(text.size_pt),
                if text.bold { "bold" } else { "normal" },
                css_colour(text.colour.unwrap_or(ink)),
                rotate(None),
                escape_html(&text.text),
            )
        }
    }
}

/// One page's cells.
fn page_markup<'a>(
    page: &ZinePage,
    options: &ZineOptions,
    u: Units,
    size: (f32, f32),
    photo: &dyn Fn(usize) -> Option<&'a WebPhoto>,
) -> (String, [u8; 3]) {
    let background = page.background.unwrap_or(options.paper.rgb());
    let ink = zine_ink_on(background);
    let cells: String = page
        .cells
        .iter()
        .map(|cell| cell_markup(cell, options, u, size, photo, ink))
        .collect();
    (cells, background)
}

/// The CSS the cells need.
const CELL_CSS: &str = ".cell{position:absolute;box-sizing:border-box}\n\
.cell img{position:absolute;display:block;max-width:none}\n\
.clip{position:absolute;overflow:hidden}\n\
.text{overflow:hidden;white-space:pre-wrap;word-wrap:break-word;line-height:1.25}\n\
.cap{position:absolute;text-align:center;opacity:.75;white-space:nowrap}\n";

/// A folded sheet is meant for printing: its panels read in order, one
/// picture after another, each on a page its own shape.
fn flow_photos(page: &ZinePage) -> Vec<usize> {
    let mut cells: Vec<&ZineCell> = page.cells.iter().collect();
    cells.sort_by_key(|c| match c.content {
        ZineContent::Photo(n) => n,
        _ => usize::MAX,
    });
    cells
        .iter()
        .filter_map(|c| match c.content {
            ZineContent::Photo(n) | ZineContent::Cover { photo: Some(n) } => Some(n),
            _ => None,
        })
        .collect()
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
    html.push_str(CELL_CSS);
    html.push_str("@media print{body{background:none}.page{break-after:page;box-shadow:none;margin:0}}\n");
    html.push_str("</style>\n</head>\n<body>\n<main>\n");

    let photo = |n: usize| photos.get(n).and_then(|p| p.as_ref());
    let units = Units::Responsive { page_w, page_h };
    for page in &layout.pages {
        if page.cells.iter().any(|c| c.rotated) {
            for n in flow_photos(page) {
                if let Some(p) = photo(n) {
                    html.push_str(&format!(
                        "<section class=\"page\" style=\"aspect-ratio:{}/{};background:{}\"><div class=\"cell\" style=\"inset:3%\">\
                         <img src=\"{}\" alt=\"{}\" style=\"width:100%;height:100%;object-fit:contain\"/></div></section>\n",
                        p.width.max(1),
                        p.height.max(1),
                        css_colour(page.background.unwrap_or(options.paper.rgb())),
                        escape_html(&p.src),
                        escape_html(&p.caption)
                    ));
                }
            }
            continue;
        }
        let (cells, background) = page_markup(page, options, units, layout.size, &photo);
        html.push_str(&format!(
            "<section class=\"page\" style=\"background:{};color:{}\">\n{cells}</section>\n",
            css_colour(background),
            css_colour(zine_ink_on(background)),
        ));
    }
    html.push_str("</main>\n</body>\n</html>\n");
    html
}

/// The pictures a layout shows, as `WebPhoto`s with the source `src(n,
/// image)` gives each; `None` for those not shown.
fn web_photos(
    layout: &ZinePageLayout,
    images: &[Option<ZineImage>],
    mut src: impl FnMut(usize, &ZineImage) -> anyhow::Result<String>,
) -> anyhow::Result<Vec<Option<WebPhoto>>> {
    let mut photos: Vec<Option<WebPhoto>> = vec![None; images.len()];
    for n in photos_used(layout) {
        let Some(Some(image)) = images.get(n) else { continue };
        photos[n] = Some(WebPhoto {
            src: src(n, image)?,
            caption: image.caption.clone(),
            width: image.width,
            height: image.height,
        });
    }
    Ok(photos)
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
    let mut written = 0;
    let photos = web_photos(layout, images, |_, image| {
        written += 1;
        let name = format!("{written:03}.jpg");
        std::fs::write(pictures.join(&name), &image.jpeg)?;
        Ok(format!("images/{name}"))
    })?;
    let index = folder.join("index.html");
    std::fs::write(&index, html_document(options, layout, &photos))?;
    Ok(index)
}

/// Writes the zine as one HTML file, its pictures inside it.
pub fn write_html_single(
    path: &Path,
    options: &ZineOptions,
    layout: &ZinePageLayout,
    images: &[Option<ZineImage>],
) -> anyhow::Result<()> {
    let photos = web_photos(layout, images, |_, image| {
        Ok(format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&image.jpeg)
        ))
    })?;
    std::fs::write(path, html_document(options, layout, &photos))?;
    Ok(())
}

// --- EPUB ------------------------------------------------------------------------------------

/// A moment as an ISO 8601 UTC time, as an EPUB records when it was made.
fn iso_now() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let (y, m, d) = rustyedit_core::civil_date(seconds);
    let of_day = seconds.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60
    )
}

/// One page of an EPUB: a document of the page's own size.
fn epub_page(title: &str, body_style: &str, size_px: (f32, f32), cells: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE html>\n\
<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" lang=\"en\">\n\
<head>\n<meta charset=\"utf-8\"/>\n<title>{}</title>\n\
<meta name=\"viewport\" content=\"width={:.0},height={:.0}\"/>\n\
<link rel=\"stylesheet\" type=\"text/css\" href=\"style.css\"/>\n</head>\n\
<body style=\"{body_style}width:{:.0}px;height:{:.0}px\">\n{cells}</body>\n</html>\n",
        escape_html(title),
        size_px.0,
        size_px.1,
        size_px.0,
        size_px.1,
    )
}

/// Writes the zine as a fixed-layout EPUB: each page keeps its design.
pub fn write_epub(
    path: &Path,
    options: &ZineOptions,
    layout: &ZinePageLayout,
    images: &[Option<ZineImage>],
) -> anyhow::Result<()> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let photos = web_photos(layout, images, |_, image| {
        let name = format!("images/img{:03}.jpg", files.len() + 1);
        files.push((name.clone(), image.jpeg.clone()));
        Ok(name)
    })?;
    let photo = |n: usize| photos.get(n).and_then(|p| p.as_ref());

    // The pages: (file name, document, size in pixels).
    let u = Units::Pixels { per_mm: PX_PER_MM };
    let page_px = (layout.size.0 * PX_PER_MM, layout.size.1 * PX_PER_MM);
    let mut pages: Vec<(String, String)> = Vec::new();
    for page in &layout.pages {
        if page.cells.iter().any(|c| c.rotated) {
            for n in flow_photos(page) {
                let Some(p) = photo(n) else { continue };
                let w = page_px.0.max(page_px.1);
                let h = w * p.height.max(1) as f32 / p.width.max(1) as f32;
                let cell = format!(
                    "<div class=\"cell\" style=\"left:3%;top:3%;width:94%;height:94%\">\
                     <img src=\"{}\" alt=\"{}\" style=\"width:100%;height:100%;object-fit:contain\"/></div>\n",
                    escape_html(&p.src),
                    escape_html(&p.caption)
                );
                let name = format!("p{:03}.xhtml", pages.len() + 1);
                let background = css_colour(page.background.unwrap_or(options.paper.rgb()));
                pages.push((
                    name,
                    epub_page(&options.title, &format!("background:{background};"), (w, h), &cell),
                ));
            }
            continue;
        }
        let (cells, background) = page_markup(page, options, u, layout.size, &photo);
        let name = format!("p{:03}.xhtml", pages.len() + 1);
        pages.push((
            name,
            epub_page(
                &options.title,
                &format!("background:{};color:{};", css_colour(background), css_colour(zine_ink_on(background))),
                page_px,
                &cells,
            ),
        ));
    }
    if pages.is_empty() {
        pages.push((
            "p001.xhtml".to_string(),
            epub_page(&options.title, "", page_px, ""),
        ));
    }

    let title = escape_html(options.title.trim());
    let author = escape_html(options.author.trim());
    let id = format!("urn:uuid:{:016x}-{:04x}", crc_of(&options.title), pages.len());
    let mut manifest = String::from(
        "<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n\
         <item id=\"css\" href=\"style.css\" media-type=\"text/css\"/>\n",
    );
    let mut spine = String::new();
    for (n, (name, _)) in pages.iter().enumerate() {
        manifest.push_str(&format!(
            "<item id=\"p{n}\" href=\"{name}\" media-type=\"application/xhtml+xml\"/>\n"
        ));
        spine.push_str(&format!("<itemref idref=\"p{n}\"/>\n"));
    }
    for (n, (name, _)) in files.iter().enumerate() {
        manifest.push_str(&format!(
            "<item id=\"img{n}\" href=\"{name}\" media-type=\"image/jpeg\"{}/>\n",
            if n == 0 { " properties=\"cover-image\"" } else { "" }
        ));
    }
    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"book-id\">\n\
<metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n\
<dc:identifier id=\"book-id\">{id}</dc:identifier>\n\
<dc:title>{title}</dc:title>\n\
{}\
<dc:language>en</dc:language>\n\
<meta property=\"dcterms:modified\">{}</meta>\n\
<meta property=\"rendition:layout\">pre-paginated</meta>\n\
<meta property=\"rendition:orientation\">auto</meta>\n\
<meta property=\"rendition:spread\">none</meta>\n\
</metadata>\n<manifest>\n{manifest}</manifest>\n<spine>\n{spine}</spine>\n</package>\n",
        if author.is_empty() { String::new() } else { format!("<dc:creator>{author}</dc:creator>\n") },
        iso_now(),
    );
    let mut toc = String::new();
    for (n, (name, _)) in pages.iter().enumerate() {
        toc.push_str(&format!("<li><a href=\"{name}\">Page {}</a></li>\n", n + 1));
    }
    let nav = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html>\n\
<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\n\
<head><meta charset=\"utf-8\"/><title>{title}</title></head>\n\
<body><nav epub:type=\"toc\" id=\"toc\"><h1>{title}</h1><ol>\n{toc}</ol></nav></body>\n</html>\n"
    );
    let css = format!("body{{margin:0;position:relative;overflow:hidden}}\n{CELL_CSS}");

    let mut zip = ZipStore::new();
    zip.add("mimetype", b"application/epub+zip");
    zip.add(
        "META-INF/container.xml",
        b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\n\
<rootfiles><rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/></rootfiles>\n\
</container>\n",
    );
    zip.add("OEBPS/content.opf", opf.as_bytes());
    zip.add("OEBPS/nav.xhtml", nav.as_bytes());
    zip.add("OEBPS/style.css", css.as_bytes());
    for (name, document) in &pages {
        zip.add(&format!("OEBPS/{name}"), document.as_bytes());
    }
    for (name, data) in &files {
        zip.add(&format!("OEBPS/{name}"), data);
    }
    std::fs::write(path, zip.finish())?;
    Ok(())
}

fn crc_of(text: &str) -> u64 {
    u64::from(crc32fast::hash(text.as_bytes())) << 16 | u64::from(crc32fast::hash(iso_now().as_bytes()) & 0xffff)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zip_store::read_back;
    use rustyedit_core::{layout_pages, Zine, ZineFit, ZineFont, ZineLayout, ZineMode, ZinePhoto, ZineTextBox};

    fn picture(w: u32, h: u32, caption: &str) -> Option<ZineImage> {
        let image = image::RgbImage::from_pixel(w, h, image::Rgb([90, 120, 200]));
        Some(ZineImage::encode(&image, 80, caption.to_string()).unwrap())
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rustyedit-web-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample() -> (ZineOptions, ZinePageLayout, Vec<Option<ZineImage>>) {
        let photo = |n: usize| ZinePhoto { path: PathBuf::from(format!("p{n}.jpg")), copy_name: None };
        let mut zine = Zine {
            photos: (0..3).map(photo).collect(),
            options: ZineOptions {
                mode: ZineMode::Manual,
                title: "Fish & <Chips>".into(),
                author: "Me".into(),
                cover: true,
                captions: true,
                ..Default::default()
            },
            ..Default::default()
        };
        zine.manual.cover = Some(photo(0));
        zine.add_free_page();
        zine.free_place(0, photo(1), (0.1, 0.1, 0.5, 0.3));
        zine.free_edit(0, 0, |i| {
            i.frame.fit = ZineFit::Fill;
            i.frame.mat_mm = 3.0;
            i.rotation = 10.0;
        });
        zine.free_add_text(0, ZineTextBox::new("Caught \"fresh\" & fried", ZineFont::Serif), (0.1, 0.6, 0.6, 0.1));
        let images = vec![picture(300, 200, "a.jpg"), picture(200, 300, "b.jpg"), picture(200, 200, "c.jpg")];
        (zine.options.clone(), zine.layout(), images)
    }

    #[test]
    fn one_html_file_carries_its_pictures_inside_it() {
        let dir = temp("single");
        let (options, layout, images) = sample();
        let path = dir.join("zine.html");
        write_html_single(&path, &options, &layout, &images).unwrap();
        let html = std::fs::read_to_string(&path).unwrap();
        assert!(html.contains("src=\"data:image/jpeg;base64,"));
        assert!(!html.contains("src=\"images/"));
        assert!(html.contains("<title>Fish &amp; &lt;Chips&gt;</title>"));
        assert!(html.contains("Caught &quot;fresh&quot; &amp; fried"));
        // The data is the picture: decode it and it is a JPEG.
        let data = html.split("base64,").nth(1).unwrap().split('"').next().unwrap();
        let bytes = base64::engine::general_purpose::STANDARD.decode(data).unwrap();
        assert!(image::load_from_memory(&bytes).is_ok());
        // Nothing else was written beside it.
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_epub_has_the_pieces_a_reader_needs() {
        let dir = temp("epub");
        let (options, layout, images) = sample();
        let path = dir.join("zine.epub");
        write_epub(&path, &options, &layout, &images).unwrap();
        let files = read_back(&std::fs::read(&path).unwrap());
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        // mimetype first, uncompressed; then the container, the package, the
        // contents, the style, a document for each page and the pictures.
        assert_eq!(names[0], "mimetype");
        for wanted in ["META-INF/container.xml", "OEBPS/content.opf", "OEBPS/nav.xhtml", "OEBPS/style.css", "OEBPS/p001.xhtml", "OEBPS/p002.xhtml", "OEBPS/images/img001.jpg"] {
            assert!(names.contains(&wanted), "{wanted} in {names:?}");
        }
        let get = |name: &str| String::from_utf8(files.iter().find(|(n, _)| n == name).unwrap().1.clone()).unwrap();
        let opf = get("OEBPS/content.opf");
        assert!(opf.contains("pre-paginated") && opf.contains("<dc:title>Fish &amp; &lt;Chips&gt;</dc:title>"));
        assert!(opf.contains("cover-image") && opf.contains("<dc:creator>Me</dc:creator>"));
        assert_eq!(opf.matches("<itemref").count(), 2, "a cover and a free page");
        // Every picture and page the spine names is in the file.
        for href in opf.split("href=\"").skip(1).map(|r| r.split('"').next().unwrap()) {
            assert!(names.contains(&format!("OEBPS/{href}").as_str()), "{href}");
        }
        // Pages are fixed in size, with their pictures by relative path.
        let page = get("OEBPS/p002.xhtml");
        assert!(page.contains("name=\"viewport\" content=\"width=794,height=1123\""), "{}", &page[..400.min(page.len())]);
        assert!(page.contains("src=\"images/img002.jpg\"") || page.contains("src=\"images/img001.jpg\""));
        assert!(page.contains("rotate(10.00deg)") && page.contains("px"));
        assert!(!page.contains("cqw"), "no container units in an e-book");
        // The XHTML is well formed enough: every image tag closes itself.
        assert_eq!(page.matches("<img").count(), page.matches("/>").count() - page.matches("<meta").count() - page.matches("<link").count());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_folded_sheet_reads_picture_by_picture_on_the_web() {
        let (mut options, _, images) = sample();
        options.layout = ZineLayout::FoldedSheet;
        let layout = layout_pages(3, &options);
        let dir = temp("fold");
        write_html_single(&dir.join("f.html"), &options, &layout, &images).unwrap();
        let html = std::fs::read_to_string(dir.join("f.html")).unwrap();
        assert!(html.matches("<section").count() >= 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
