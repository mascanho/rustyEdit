//! Zines: a set of photos chosen from the library (right-click ▸ Add to
//! zine) and laid out on pages, written as a PDF or an HTML page.
//!
//! This module is the model and the page layout; writing the files is in
//! `rustyedit-io`. Positions are in millimetres from a page's top-left.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// What a zine is written as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ZineFormat {
    #[default]
    Pdf,
    /// A folder holding a web page and its pictures.
    Html,
}

impl ZineFormat {
    pub const ALL: [ZineFormat; 2] = [ZineFormat::Pdf, ZineFormat::Html];

    pub fn label(&self) -> &'static str {
        match self {
            ZineFormat::Pdf => "PDF",
            ZineFormat::Html => "HTML",
        }
    }
}

/// How the photos sit on the pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ZineLayout {
    #[default]
    OnePerPage,
    TwoPerPage,
    FourPerPage,
    /// One sheet of eight panels, cut and folded into an 8-page booklet.
    FoldedSheet,
}

impl ZineLayout {
    pub const ALL: [ZineLayout; 4] = [
        ZineLayout::OnePerPage,
        ZineLayout::TwoPerPage,
        ZineLayout::FourPerPage,
        ZineLayout::FoldedSheet,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ZineLayout::OnePerPage => "1 per page",
            ZineLayout::TwoPerPage => "2 per page",
            ZineLayout::FourPerPage => "4 per page",
            ZineLayout::FoldedSheet => "Folded sheet",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ZineLayout::OnePerPage => "A full page for each photo",
            ZineLayout::TwoPerPage => "Two photos share each page",
            ZineLayout::FourPerPage => "A page of four, two by two",
            ZineLayout::FoldedSheet => "Print one sheet, fold it into an 8-page mini zine",
        }
    }

    fn per_page(&self) -> usize {
        match self {
            ZineLayout::OnePerPage => 1,
            ZineLayout::TwoPerPage => 2,
            ZineLayout::FourPerPage => 4,
            ZineLayout::FoldedSheet => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ZinePageSize {
    A5,
    #[default]
    A4,
    Letter,
    Square,
}

impl ZinePageSize {
    pub const ALL: [ZinePageSize; 4] = [
        ZinePageSize::A5,
        ZinePageSize::A4,
        ZinePageSize::Letter,
        ZinePageSize::Square,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ZinePageSize::A5 => "A5",
            ZinePageSize::A4 => "A4",
            ZinePageSize::Letter => "Letter",
            ZinePageSize::Square => "Square",
        }
    }

    /// Width and height in millimetres, upright.
    pub fn millimetres(&self) -> (f32, f32) {
        match self {
            ZinePageSize::A5 => (148.0, 210.0),
            ZinePageSize::A4 => (210.0, 297.0),
            ZinePageSize::Letter => (215.9, 279.4),
            ZinePageSize::Square => (200.0, 200.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ZinePaper {
    #[default]
    White,
    Cream,
    Black,
}

impl ZinePaper {
    pub const ALL: [ZinePaper; 3] = [ZinePaper::White, ZinePaper::Cream, ZinePaper::Black];

    pub fn label(&self) -> &'static str {
        match self {
            ZinePaper::White => "White",
            ZinePaper::Cream => "Cream",
            ZinePaper::Black => "Black",
        }
    }

    pub fn rgb(&self) -> [u8; 3] {
        match self {
            ZinePaper::White => [255, 255, 255],
            ZinePaper::Cream => [247, 241, 226],
            ZinePaper::Black => [14, 14, 14],
        }
    }

    /// The text colour that reads on this paper.
    pub fn ink(&self) -> [u8; 3] {
        match self {
            ZinePaper::Black => [235, 235, 235],
            _ => [30, 30, 30],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ZineOptions {
    pub format: ZineFormat,
    pub layout: ZineLayout,
    pub page_size: ZinePageSize,
    pub landscape: bool,
    /// Space around the pictures, in millimetres.
    pub margin_mm: u8,
    pub paper: ZinePaper,
    pub title: String,
    pub author: String,
    /// A cover (the first photo and the title) and, on a folded sheet, a
    /// back cover.
    pub cover: bool,
    /// The photo's file name under each picture.
    pub captions: bool,
    /// JPEG quality of the pictures inside the zine.
    pub quality: u8,
    pub destination: Option<PathBuf>,
}

impl Default for ZineOptions {
    fn default() -> Self {
        Self {
            format: ZineFormat::Pdf,
            layout: ZineLayout::OnePerPage,
            page_size: ZinePageSize::A4,
            landscape: false,
            margin_mm: 12,
            paper: ZinePaper::White,
            title: "Untitled zine".to_string(),
            author: String::new(),
            cover: true,
            captions: false,
            quality: 88,
            destination: None,
        }
    }
}

/// One photo in a zine: the file, and which virtual copy of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZinePhoto {
    pub path: PathBuf,
    pub copy_name: Option<String>,
}

/// The photos picked for a zine, in page order, with how it is made.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Zine {
    pub photos: Vec<ZinePhoto>,
    pub options: ZineOptions,
}

impl Zine {
    pub fn position(&self, photo: &ZinePhoto) -> Option<usize> {
        self.photos.iter().position(|p| p == photo)
    }

    pub fn contains(&self, photo: &ZinePhoto) -> bool {
        self.position(photo).is_some()
    }

    /// Adds the photos not already in; how many were new.
    pub fn add(&mut self, photos: impl IntoIterator<Item = ZinePhoto>) -> usize {
        let mut added = 0;
        for photo in photos {
            if !self.contains(&photo) {
                self.photos.push(photo);
                added += 1;
            }
        }
        added
    }

    /// Takes the photos out; how many were in.
    pub fn remove(&mut self, photos: &[ZinePhoto]) -> usize {
        let before = self.photos.len();
        self.photos.retain(|p| !photos.contains(p));
        before - self.photos.len()
    }

    /// Moves the photo at `ix` one place earlier (`-1`) or later (`1`).
    pub fn shift(&mut self, ix: usize, by: i32) -> Option<usize> {
        let to = ix as i64 + by as i64;
        if ix >= self.photos.len() || to < 0 || to as usize >= self.photos.len() {
            return None;
        }
        self.photos.swap(ix, to as usize);
        Some(to as usize)
    }
}

/// A rectangle on a page, in millimetres from its top-left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// What a cell shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Content {
    /// Photo `n` of the zine.
    Photo(usize),
    /// The cover: the first photo (when there is one) with the title.
    Cover { photo: Option<usize> },
    /// The back cover: the author.
    Back,
}

/// A place on a page. A `rotated` cell is drawn upside down, as the top
/// row of a folded sheet is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    pub rect: Rect,
    pub content: Content,
    pub rotated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub cells: Vec<Cell>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageLayout {
    /// Page width and height in millimetres.
    pub size: (f32, f32),
    pub pages: Vec<Page>,
    /// Photos that did not fit (only a folded sheet has a limit).
    pub left_out: usize,
}

/// Room kept under a picture for its caption.
pub const CAPTION_MM: f32 = 6.0;
/// The gap between pictures sharing a page.
const GUTTER_MM: f32 = 6.0;

/// The pages for `photo_count` photos.
pub fn layout_pages(photo_count: usize, options: &ZineOptions) -> PageLayout {
    let (mut w, mut h) = options.page_size.millimetres();
    if options.landscape || options.layout == ZineLayout::FoldedSheet {
        (w, h) = (w.max(h), w.min(h));
    }
    // A square page makes no folded sheet worth printing: use A4's shape.
    if options.layout == ZineLayout::FoldedSheet && options.page_size == ZinePageSize::Square {
        (w, h) = ZinePageSize::A4.millimetres();
        (w, h) = (w.max(h), w.min(h));
    }
    let margin = options.margin_mm as f32;
    if options.layout == ZineLayout::FoldedSheet {
        return folded_sheet(photo_count, options, (w, h));
    }

    let area = Rect {
        x: margin,
        y: margin,
        w: (w - 2.0 * margin).max(10.0),
        h: (h - 2.0 * margin).max(10.0),
    };
    let mut pages = Vec::new();
    let mut next = 0;
    if options.cover {
        pages.push(Page {
            cells: vec![Cell {
                rect: area,
                content: Content::Cover {
                    photo: (photo_count > 0).then_some(0),
                },
                rotated: false,
            }],
        });
        next = photo_count.min(1);
    }
    let per = options.layout.per_page();
    let mut photo = next;
    while photo < photo_count {
        let n = per.min(photo_count - photo);
        let rects = grid(area, per, w > h);
        let cells = (0..n)
            .map(|i| Cell {
                rect: rects[i],
                content: Content::Photo(photo + i),
                rotated: false,
            })
            .collect();
        pages.push(Page { cells });
        photo += n;
    }
    PageLayout {
        size: (w, h),
        pages,
        left_out: 0,
    }
}

/// `per` equal cells in `area`: stacked or side by side for two, 2x2 for four.
fn grid(area: Rect, per: usize, wide: bool) -> Vec<Rect> {
    let cell = |cols: usize, rows: usize| -> Vec<Rect> {
        let cw = (area.w - GUTTER_MM * (cols as f32 - 1.0)) / cols as f32;
        let ch = (area.h - GUTTER_MM * (rows as f32 - 1.0)) / rows as f32;
        (0..rows)
            .flat_map(|r| {
                (0..cols).map(move |c| Rect {
                    x: area.x + c as f32 * (cw + GUTTER_MM),
                    y: area.y + r as f32 * (ch + GUTTER_MM),
                    w: cw,
                    h: ch,
                })
            })
            .collect()
    };
    match per {
        1 => vec![area],
        2 if wide => cell(2, 1),
        2 => cell(1, 2),
        _ => cell(2, 2),
    }
}

/// Eight panels on one landscape sheet. With the sheet folded in half and
/// then half again, reading order is the panel numbers: the top row is
/// printed upside down.
fn folded_sheet(photo_count: usize, options: &ZineOptions, size: (f32, f32)) -> PageLayout {
    let (w, h) = size;
    let (pw, ph) = (w / 4.0, h / 2.0);
    let margin = (options.margin_mm as f32).min(pw / 4.0).min(ph / 4.0);
    // Panel number (1..=8) at each position, top row then bottom row.
    const ORDER: [[usize; 4]; 2] = [[5, 4, 3, 2], [6, 7, 8, 1]];

    let mut contents: [Option<Content>; 8] = [None; 8];
    let mut photo = 0;
    let take = |photo: &mut usize| -> Option<Content> {
        (*photo < photo_count).then(|| {
            *photo += 1;
            Content::Photo(*photo - 1)
        })
    };
    if options.cover {
        contents[0] = Some(Content::Cover {
            photo: (photo_count > 0).then_some(0),
        });
        photo = photo_count.min(1);
        for slot in contents.iter_mut().take(7).skip(1) {
            *slot = take(&mut photo);
        }
        contents[7] = Some(Content::Back);
    } else {
        for slot in contents.iter_mut() {
            *slot = take(&mut photo);
        }
    }

    let mut cells = Vec::new();
    for (row, numbers) in ORDER.iter().enumerate() {
        for (col, number) in numbers.iter().enumerate() {
            let Some(content) = contents[number - 1] else {
                continue;
            };
            cells.push(Cell {
                rect: Rect {
                    x: col as f32 * pw + margin,
                    y: row as f32 * ph + margin,
                    w: pw - 2.0 * margin,
                    h: ph - 2.0 * margin,
                },
                content,
                rotated: row == 0,
            });
        }
    }
    PageLayout {
        size,
        pages: vec![Page { cells }],
        left_out: photo_count.saturating_sub(photo),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(name: &str) -> ZinePhoto {
        ZinePhoto {
            path: PathBuf::from(name),
            copy_name: None,
        }
    }

    fn options(layout: ZineLayout, cover: bool) -> ZineOptions {
        ZineOptions {
            layout,
            cover,
            ..Default::default()
        }
    }

    fn photos_on(page: &Page) -> Vec<usize> {
        page.cells
            .iter()
            .filter_map(|c| match c.content {
                Content::Photo(n) => Some(n),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn photos_are_added_once_removed_and_reordered() {
        let mut zine = Zine::default();
        assert_eq!(zine.add([photo("a"), photo("b"), photo("a")]), 2);
        assert_eq!(zine.add([photo("b"), photo("c")]), 1);
        assert_eq!(zine.shift(0, 1), Some(1));
        assert_eq!(zine.photos[0], photo("b"));
        assert_eq!(zine.shift(0, -1), None);
        assert_eq!(zine.shift(2, 1), None);
        assert_eq!(zine.remove(&[photo("a"), photo("zz")]), 1);
        assert_eq!(zine.photos.len(), 2);
    }

    #[test]
    fn a_cover_takes_the_first_photo_and_the_rest_follow() {
        let layout = layout_pages(5, &options(ZineLayout::OnePerPage, true));
        // Cover plus the other four.
        assert_eq!(layout.pages.len(), 5);
        assert_eq!(
            layout.pages[0].cells[0].content,
            Content::Cover { photo: Some(0) }
        );
        assert_eq!(photos_on(&layout.pages[1]), vec![1]);
        assert_eq!(photos_on(&layout.pages[4]), vec![4]);
        // Without a cover every photo has its own page.
        assert_eq!(layout_pages(5, &options(ZineLayout::OnePerPage, false)).pages.len(), 5);
    }

    #[test]
    fn several_to_a_page_fill_in_order_and_stay_inside_the_margins() {
        let o = options(ZineLayout::FourPerPage, false);
        let layout = layout_pages(6, &o);
        assert_eq!(layout.pages.len(), 2);
        assert_eq!(photos_on(&layout.pages[0]), vec![0, 1, 2, 3]);
        assert_eq!(photos_on(&layout.pages[1]), vec![4, 5]);
        let (w, h) = layout.size;
        let m = o.margin_mm as f32;
        for cell in layout.pages.iter().flat_map(|p| &p.cells) {
            let r = cell.rect;
            assert!(r.x >= m - 0.01 && r.y >= m - 0.01);
            assert!(r.x + r.w <= w - m + 0.01 && r.y + r.h <= h - m + 0.01);
        }
        // Two to a portrait page stack; landscape puts them side by side.
        let stacked = layout_pages(2, &options(ZineLayout::TwoPerPage, false));
        let c = &stacked.pages[0].cells;
        assert!(c[1].rect.y > c[0].rect.y && c[1].rect.x == c[0].rect.x);
        let wide = layout_pages(
            2,
            &ZineOptions { landscape: true, ..options(ZineLayout::TwoPerPage, false) },
        );
        let c = &wide.pages[0].cells;
        assert!(c[1].rect.x > c[0].rect.x && c[1].rect.y == c[0].rect.y);
    }

    #[test]
    fn a_folded_sheet_has_eight_panels_with_the_top_row_upside_down() {
        let layout = layout_pages(20, &options(ZineLayout::FoldedSheet, true));
        assert_eq!(layout.pages.len(), 1);
        let cells = &layout.pages[0].cells;
        assert_eq!(cells.len(), 8);
        assert_eq!(cells.iter().filter(|c| c.rotated).count(), 4);
        // Cover first photo, six more, then the back; the rest do not fit.
        assert_eq!(layout.left_out, 20 - 7);
        assert!(layout.size.0 > layout.size.1);
        // Cover is bottom right, back cover beside it; panel 2 sits above
        // the cover, upside down.
        let at = |content: Content| cells.iter().find(|c| c.content == content).unwrap();
        let cover = at(Content::Cover { photo: Some(0) });
        let back = at(Content::Back);
        let second = at(Content::Photo(1));
        assert!(!cover.rotated && cover.rect.x > back.rect.x && cover.rect.y == back.rect.y);
        assert!(second.rotated && second.rect.x == cover.rect.x && second.rect.y < cover.rect.y);
    }

    #[test]
    fn a_short_folded_sheet_leaves_panels_blank() {
        let layout = layout_pages(3, &options(ZineLayout::FoldedSheet, false));
        assert_eq!(layout.pages[0].cells.len(), 3);
        assert_eq!(layout.left_out, 0);
        assert!(layout_pages(0, &options(ZineLayout::OnePerPage, true)).pages.len() == 1);
    }
}
