//! Writing a zine: the PDF (pages laid out by `rustyedit_core::layout_pages`)
//! or an HTML page with its pictures, and remembering the zine between
//! runs.

use std::path::{Path, PathBuf};

use image::RgbImage;
use rustyedit_core::{
    Zine, ZineCell, ZineContent, ZineDesigns, ZineLayout, ZineLibrary, ZineOptions,
    ZinePageLayout, ZineRect, CAPTION_MM,
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

// --- Which photos a layout shows ----------------------------------------------

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

// --- Remembering ---------------------------------------------------------------

fn store_path(name: &str) -> Option<PathBuf> {
    crate::settings_path().map(|path| path.with_file_name(name))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn write_json<T: serde::Serialize>(path: Option<PathBuf>, value: &T) -> anyhow::Result<()> {
    let path = path.ok_or_else(|| anyhow::anyhow!("no config directory"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}

/// The zines saved last time. A single zine saved by an earlier version
/// becomes the first of the library.
pub fn load_library() -> ZineLibrary {
    if let Some(library) = store_path("zines.json").and_then(|p| read_json::<ZineLibrary>(&p)) {
        return library.normalised();
    }
    store_path("zine.json")
        .and_then(|p| read_json::<Zine>(&p))
        .map(ZineLibrary::of)
        .unwrap_or_default()
}

pub fn save_library(library: &ZineLibrary) -> anyhow::Result<()> {
    write_json(store_path("zines.json"), library)
}

/// The designs saved to start zines from.
pub fn load_designs() -> ZineDesigns {
    store_path("zine_designs.json")
        .and_then(|p| read_json(&p))
        .unwrap_or_default()
}

pub fn save_designs(designs: &ZineDesigns) -> anyhow::Result<()> {
    write_json(store_path("zine_designs.json"), designs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zine_web::write_html;

    #[test]
    fn a_library_and_designs_round_trip_and_an_old_zine_loads() {
        let mut library = ZineLibrary::default();
        library.zines[0].options.title = "Kept".into();
        let json = serde_json::to_string(&library).unwrap();
        let back: ZineLibrary = serde_json::from_str(&json).unwrap();
        assert_eq!(back.zines[0].options.title, "Kept");
        // A library file with nothing in it still has a zine to work on.
        let empty: ZineLibrary = serde_json::from_str(r#"{"zines":[],"active":3}"#).unwrap();
        let empty = empty.normalised();
        assert_eq!((empty.zines.len(), empty.active), (1, 0));
        // What the earlier version wrote is a bare zine.
        let old: Zine = serde_json::from_str(r#"{"photos":[],"options":{"title":"Old"}}"#).unwrap();
        assert_eq!(ZineLibrary::of(old).zines[0].options.title, "Old");
        let designs: ZineDesigns = serde_json::from_str("{}").unwrap();
        assert!(designs.designs.is_empty());
    }
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
        crate::zine_raster::write_page_images(&dir.join("rich-png"), &zine.options, &layout, &images, crate::zine_raster::PageImage::Png).unwrap();
        crate::zine_web::write_epub(&dir.join("rich.epub"), &zine.options, &layout, &images).unwrap();
        crate::zine_web::write_html_single(&dir.join("rich.html"), &zine.options, &layout, &images).unwrap();
    }
}
