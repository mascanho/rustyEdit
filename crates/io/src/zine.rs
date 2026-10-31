//! Writing a zine: the PDF (pages laid out by `rustyedit_core::layout_pages`)
//! or an HTML page with its pictures, and remembering the zine between
//! runs.

use std::path::{Path, PathBuf};

use image::RgbImage;
use rustyedit_core::{
    layout_pages, Zine, ZineCell, ZineContent, ZineLayout, ZineOptions, ZineRect,
    CAPTION_MM,
};

use crate::pdf::{self, Font, JpegImage, PdfPage};

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

/// Writes the zine as a PDF at `path`. `images[n]` is photo `n` of the zine
/// (`None` for one that could not be rendered).
pub fn write_pdf(
    path: &Path,
    options: &ZineOptions,
    images: &[Option<ZineImage>],
) -> anyhow::Result<()> {
    let layout = layout_pages(images.len(), options);
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
    let ink = options.paper.ink();
    let folded = options.layout == ZineLayout::FoldedSheet;
    let mut pages = Vec::new();
    for page in &layout.pages {
        let mut out = PdfPage::new(layout.size.0, layout.size.1);
        out.fill_page(paper);
        for cell in &page.cells {
            let draw = |out: &mut PdfPage, slot: &mut dyn FnMut(usize) -> Option<usize>| {
                draw_cell(out, cell, options, images, slot, ink, folded);
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
    let aspect = image_size.0 as f32 / image_size.1.max(1) as f32;
    let (mut w, mut h) = (area.w, area.w / aspect);
    if h > area.h {
        (w, h) = (area.h * aspect, area.h);
    }
    ZineRect {
        x: area.x + (area.w - w) / 2.0,
        y: area.y + (area.h - h) / 2.0,
        w,
        h,
    }
}

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
    match cell.content {
        ZineContent::Photo(n) => {
            let caption_room = if options.captions { CAPTION_MM * scale.max(0.7) } else { 0.0 };
            let area = ZineRect { h: (r.h - caption_room).max(1.0), ..r };
            if let (Some(index), Some(image)) = (slot(n), images.get(n).and_then(|i| i.as_ref())) {
                let at = fit_inside(area, (image.width, image.height));
                out.image(index, at.x, at.y, at.w, at.h);
                if options.captions && !image.caption.is_empty() {
                    let size = pdf::fit_text_size(&image.caption, r.w, 8.0 * scale.max(0.8), 5.0, Font::Regular);
                    out.text(
                        &image.caption,
                        r.x + r.w / 2.0,
                        at.y + at.h + caption_room * 0.75,
                        size,
                        Font::Regular,
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
            let title_size = pdf::fit_text_size(title, r.w, 30.0 * scale, 8.0, Font::Bold);
            let centre = r.x + r.w / 2.0;
            let base = r.y + r.h - band + band * 0.45;
            out.text(title, centre, base, title_size, Font::Bold, ink, true);
            let author = options.author.trim();
            if !author.is_empty() {
                let size = pdf::fit_text_size(author, r.w, 13.0 * scale, 6.0, Font::Regular);
                out.text(author, centre, base + band * 0.38, size, Font::Regular, ink, true);
            }
        }
        ZineContent::Back => {
            let centre_x = r.x + r.w / 2.0;
            let middle = r.y + r.h / 2.0;
            let title = options.title.trim();
            let size = pdf::fit_text_size(title, r.w, 16.0 * scale, 7.0, Font::Bold);
            out.text(title, centre_x, middle, size, Font::Bold, ink, true);
            let author = options.author.trim();
            if !author.is_empty() {
                let size = pdf::fit_text_size(author, r.w, 11.0 * scale, 6.0, Font::Regular);
                out.text(author, centre_x, middle + 7.0 * scale.max(0.7), size, Font::Regular, ink, true);
            }
            out.text(
                "Made with RustyEdit",
                centre_x,
                r.y + r.h - 2.0,
                6.0,
                Font::Regular,
                [150, 150, 150],
                true,
            );
        }
    }
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

/// The web page for a zine whose pictures are `files` (name and caption),
/// in `images/` beside it.
pub fn html_document(options: &ZineOptions, files: &[(String, String)]) -> String {
    let per = match options.layout {
        ZineLayout::OnePerPage => 1,
        ZineLayout::TwoPerPage => 2,
        ZineLayout::FourPerPage | ZineLayout::FoldedSheet => 4,
    };
    let title = escape_html(options.title.trim());
    let author = escape_html(options.author.trim());
    let mut html = String::new();
    html.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str(&format!("<title>{title}</title>\n<style>\n"));
    html.push_str(&format!(
        "body{{margin:0;background:{};color:{};font-family:Helvetica,Arial,sans-serif}}\n",
        css_colour(options.paper.rgb()),
        css_colour(options.paper.ink())
    ));
    html.push_str(&format!(
        "main{{max-width:1100px;margin:0 auto;padding:{}px}}\n",
        (options.margin_mm as u32 * 2).max(12)
    ));
    html.push_str("header{text-align:center;padding:48px 0 32px}\n");
    html.push_str("h1{margin:0 0 8px;font-size:2.4rem}\nheader p{margin:0;opacity:.7}\n");
    html.push_str(&format!(
        ".page{{display:grid;grid-template-columns:repeat({},1fr);gap:24px;margin:0 0 48px}}\n",
        if per == 1 { 1 } else { 2 }
    ));
    html.push_str("figure{margin:0}\nimg{width:100%;height:auto;display:block}\n");
    html.push_str("figcaption{font-size:.8rem;text-align:center;padding-top:6px;opacity:.7}\n");
    html.push_str("@media print{.page{break-after:page}}\n</style>\n</head>\n<body>\n<main>\n");
    if options.cover {
        html.push_str("<header>\n");
        html.push_str(&format!("<h1>{title}</h1>\n"));
        if !author.is_empty() {
            html.push_str(&format!("<p>{author}</p>\n"));
        }
        html.push_str("</header>\n");
    }
    for page in files.chunks(per) {
        html.push_str("<section class=\"page\">\n");
        for (file, caption) in page {
            html.push_str(&format!(
                "<figure><img src=\"images/{}\" alt=\"{}\">",
                escape_html(file),
                escape_html(caption)
            ));
            if options.captions && !caption.is_empty() {
                html.push_str(&format!("<figcaption>{}</figcaption>", escape_html(caption)));
            }
            html.push_str("</figure>\n");
        }
        html.push_str("</section>\n");
    }
    html.push_str("</main>\n</body>\n</html>\n");
    html
}

/// Writes the zine as `<folder>/index.html` with its pictures in
/// `<folder>/images`. Returns the page's path.
pub fn write_html(
    folder: &Path,
    options: &ZineOptions,
    images: &[Option<ZineImage>],
) -> anyhow::Result<PathBuf> {
    let pictures = folder.join("images");
    std::fs::create_dir_all(&pictures)?;
    let mut files = Vec::new();
    for image in images.iter().flatten() {
        let name = format!("{:03}.jpg", files.len() + 1);
        std::fs::write(pictures.join(&name), &image.jpeg)?;
        files.push((name, image.caption.clone()));
    }
    let index = folder.join("index.html");
    std::fs::write(&index, html_document(options, &files))?;
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
            write_pdf(&path, &options, &images).unwrap();
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
        let index = write_html(&dir, &options, &images).unwrap();
        let html = std::fs::read_to_string(index).unwrap();
        assert!(html.contains("<title>Fish &lt;&amp;&gt; Chips</title>"));
        assert!(html.contains("src=\"images/001.jpg\""));
        assert!(html.contains("src=\"images/002.jpg\""));
        assert!(html.contains("<figcaption>a &amp; b.jpg</figcaption>"));
        assert_eq!(html.matches("<section").count(), 1);
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
            write_pdf(&dir.join(format!("{layout:?}.pdf")), &options, &images).unwrap();
        }
        let options = ZineOptions { captions: true, author: "Marco".into(), ..Default::default() };
        write_html(&dir.join("web"), &options, &images).unwrap();
    }
}
