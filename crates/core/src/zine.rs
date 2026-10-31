//! Zines: a set of photos chosen from the library (right-click ▸ Add to
//! zine) and laid out on pages, written as a PDF or an HTML page.
//!
//! This module is the model and the page layout; writing the files is in
//! `rustyedit-io`. Positions are in millimetres from a page's top-left.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use crate::zine_design::{
    align_rects, bounding_box, distribute_rects, fill_rect, fit_rect, ink_on, photo_geometry,
    AlignMode, DistributeAxis, Fit, FrameStyle, PhotoGeometry,
    FreeItem, TextAlign, TextBox, ZineFont, ZineStyle, ZineTemplate, MAX_MAT_MM, MAX_RADIUS_MM,
    MAX_TEXT_PT, MIN_ITEM, MIN_TEXT_PT,
};

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

/// Who decides which photo goes where.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ZineMode {
    /// The photos flow onto pages in order.
    #[default]
    Auto,
    /// The user places each photo on a page of their choosing.
    Manual,
}

impl ZineMode {
    pub const ALL: [ZineMode; 2] = [ZineMode::Auto, ZineMode::Manual];

    pub fn label(&self) -> &'static str {
        match self {
            ZineMode::Auto => "Automatic",
            ZineMode::Manual => "Manual",
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
    pub mode: ZineMode,
    /// The type of the title, captions and new text boxes.
    pub font: ZineFont,
    /// Which style preset the zine started from.
    pub style: ZineStyle,
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
            mode: ZineMode::Auto,
            font: ZineFont::Sans,
            style: ZineStyle::Clean,
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

/// One page of a manually laid-out zine: its slots, which hold a photo or
/// are empty. One, two or four slots are an ordinary page; eight are a
/// folded sheet, whose slots are its panels in reading order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ManualPage {
    pub slots: Vec<Option<ZinePhoto>>,
    /// A free page has no slots: its photos and text are `items`, placed
    /// and sized anywhere on it, the last in front.
    pub items: Vec<FreeItem>,
    /// This page's own paper colour, instead of the zine's.
    pub background: Option<[u8; 3]>,
}

impl ManualPage {
    pub fn new(slots: usize) -> Self {
        Self {
            slots: vec![None; slots],
            items: Vec::new(),
            background: None,
        }
    }

    /// An empty free page.
    pub fn free() -> Self {
        Self::default()
    }

    pub fn is_free(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn is_sheet(&self) -> bool {
        self.slots.len() == SHEET_PANELS
    }
}

/// The pages of a manually laid-out zine.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ManualLayout {
    /// Set once the pages have been started (from the automatic layout, or
    /// emptied by hand), so they are not started again.
    pub started: bool,
    pub pages: Vec<ManualPage>,
    /// The photo on the cover (when the zine has one).
    pub cover: Option<ZinePhoto>,
}

/// Panels on a folded sheet.
pub const SHEET_PANELS: usize = 8;

/// A place a photo can be put in a manual layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    Cover,
    Slot { page: usize, slot: usize },
}

/// The photos picked for a zine, in page order, with how it is made.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Zine {
    pub photos: Vec<ZinePhoto>,
    pub options: ZineOptions,
    pub manual: ManualLayout,
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
        // A photo that is out of the zine is off its pages too.
        let gone = |slot: &mut Option<ZinePhoto>| {
            if slot.as_ref().is_some_and(|p| photos.contains(p)) {
                *slot = None;
            }
        };
        gone(&mut self.manual.cover);
        for page in &mut self.manual.pages {
            page.slots.iter_mut().for_each(gone);
            // A frame whose photo went stays, empty.
            for item in &mut page.items {
                if item.photo.as_ref().is_some_and(|p| photos.contains(p)) {
                    item.photo = None;
                }
            }
        }
        before - self.photos.len()
    }

    /// Takes everything out.
    pub fn clear(&mut self) {
        self.photos.clear();
        self.manual = ManualLayout::default();
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
    /// A box of text (the cell holds it).
    Text,
}

/// A place on a page. A `rotated` cell is drawn upside down, as the top
/// row of a folded sheet is.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub rect: Rect,
    pub content: Content,
    pub rotated: bool,
    /// A free page's photo may be turned about its middle (degrees,
    /// clockwise).
    pub rotation: f32,
    pub frame: FrameStyle,
    pub text: Option<TextBox>,
}

impl Cell {
    /// A cell with the plain look.
    pub fn plain(rect: Rect, content: Content, rotated: bool) -> Self {
        Self {
            rect,
            content,
            rotated,
            rotation: 0.0,
            frame: FrameStyle::default(),
            text: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub cells: Vec<Cell>,
    /// A paper colour of this page's own.
    pub background: Option<[u8; 3]>,
}

impl Page {
    pub fn new(cells: Vec<Cell>) -> Self {
        Self {
            cells,
            background: None,
        }
    }
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
    let folded = options.layout == ZineLayout::FoldedSheet;
    let (w, h) = page_size(options, folded);
    let margin = options.margin_mm as f32;
    if folded {
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
        pages.push(Page::new(vec![Cell::plain(
            area,
            Content::Cover {
                photo: (photo_count > 0).then_some(0),
            },
            false,
        )]));
        next = photo_count.min(1);
    }
    let per = options.layout.per_page();
    let mut photo = next;
    while photo < photo_count {
        let n = per.min(photo_count - photo);
        let rects = grid(area, per, w > h);
        let cells = (0..n)
            .map(|i| Cell::plain(rects[i], Content::Photo(photo + i), false))
            .collect();
        pages.push(Page::new(cells));
        photo += n;
    }
    PageLayout {
        size: (w, h),
        pages,
        left_out: 0,
    }
}

/// A page's width and height in millimetres: a folded sheet is always
/// landscape.
fn page_size(options: &ZineOptions, folded: bool) -> (f32, f32) {
    let (mut w, mut h) = options.page_size.millimetres();
    if options.landscape || folded {
        (w, h) = (w.max(h), w.min(h));
    }
    // A square page makes no folded sheet worth printing: use A4's shape.
    if folded && options.page_size == ZinePageSize::Square {
        (w, h) = ZinePageSize::A4.millimetres();
        (w, h) = (w.max(h), w.min(h));
    }
    (w, h)
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

    let cells = sheet_cells(&contents, size, margin);
    PageLayout {
        size,
        pages: vec![Page::new(cells)],
        left_out: photo_count.saturating_sub(photo),
    }
}

/// The cells of a folded sheet whose panel `n` (1..=8) shows
/// `contents[n - 1]`: the top row is upside down, and with the sheet
/// folded the panels read in order.
fn sheet_cells(contents: &[Option<Content>; 8], (w, h): (f32, f32), margin: f32) -> Vec<Cell> {
    let (pw, ph) = (w / 4.0, h / 2.0);
    // Panel number (1..=8) at each position, top row then bottom row.
    const ORDER: [[usize; 4]; 2] = [[5, 4, 3, 2], [6, 7, 8, 1]];
    let mut cells = Vec::new();
    for (row, numbers) in ORDER.iter().enumerate() {
        for (col, number) in numbers.iter().enumerate() {
            let Some(content) = contents[number - 1] else {
                continue;
            };
            cells.push(Cell::plain(
                Rect {
                    x: col as f32 * pw + margin,
                    y: row as f32 * ph + margin,
                    w: pw - 2.0 * margin,
                    h: ph - 2.0 * margin,
                },
                content,
                row == 0,
            ));
        }
    }
    cells
}

// --- Snapping on free pages ---------------------------------------------------------

/// A line a dragged photo has snapped to, as a page fraction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Guide {
    pub vertical: bool,
    pub at: f32,
}

/// Lines a photo's edges and middle snap to: the page's margins and middle.
fn snap_lines(margin: (f32, f32)) -> ([f32; 3], [f32; 3]) {
    ([margin.0, 0.5, 1.0 - margin.0], [margin.1, 0.5, 1.0 - margin.1])
}

/// The closest shift (within `threshold`) that puts one of `edges` on one of
/// `lines`.
fn nearest_shift(edges: &[f32], lines: &[f32; 3], threshold: f32) -> Option<(f32, f32)> {
    let mut best: Option<(f32, f32)> = None;
    for &edge in edges {
        for &line in lines {
            let shift = line - edge;
            if shift.abs() <= threshold && best.is_none_or(|(s, _)| shift.abs() < s.abs()) {
                best = Some((shift, line));
            }
        }
    }
    best
}

/// A photo `rect` (x, y, w, h as page fractions) being moved, snapped so its
/// edges or middle meet the margins and the page's middle. `margin` is the
/// page margin as fractions of the width and height.
pub fn snap_move(
    rect: (f32, f32, f32, f32),
    margin: (f32, f32),
    threshold: f32,
) -> ((f32, f32, f32, f32), Vec<Guide>) {
    let (xs, ys) = snap_lines(margin);
    let (x, y, w, h) = rect;
    let mut guides = Vec::new();
    let mut out = rect;
    if let Some((shift, at)) = nearest_shift(&[x, x + w / 2.0, x + w], &xs, threshold) {
        out.0 += shift;
        guides.push(Guide { vertical: true, at });
    }
    if let Some((shift, at)) = nearest_shift(&[y, y + h / 2.0, y + h], &ys, threshold) {
        out.1 += shift;
        guides.push(Guide { vertical: false, at });
    }
    (out, guides)
}

/// A photo being resized from its bottom-right corner: that corner snaps.
pub fn snap_corner(
    rect: (f32, f32, f32, f32),
    margin: (f32, f32),
    threshold: f32,
) -> ((f32, f32, f32, f32), Vec<Guide>) {
    let (xs, ys) = snap_lines(margin);
    let (x, y, w, h) = rect;
    let mut guides = Vec::new();
    let mut out = rect;
    if let Some((shift, at)) = nearest_shift(&[x + w], &xs, threshold) {
        out.2 += shift;
        guides.push(Guide { vertical: true, at });
    }
    if let Some((shift, at)) = nearest_shift(&[y + h], &ys, threshold) {
        out.3 += shift;
        guides.push(Guide { vertical: false, at });
    }
    (out, guides)
}

// --- Manual layout ----------------------------------------------------------------

impl Zine {
    /// The pages, by `options.mode`.
    pub fn layout(&self) -> PageLayout {
        match self.options.mode {
            ZineMode::Auto => layout_pages(self.photos.len(), &self.options),
            ZineMode::Manual => self.manual_layout(),
        }
    }

    fn manual_layout(&self) -> PageLayout {
        let options = &self.options;
        let index = |photo: &Option<ZinePhoto>| -> Option<usize> {
            photo.as_ref().and_then(|p| self.position(p))
        };
        let sheet = self.manual.pages.iter().find(|p| p.is_sheet());
        if let Some(sheet) = sheet {
            let size = page_size(options, true);
            let (pw, ph) = (size.0 / 4.0, size.1 / 2.0);
            let margin = (options.margin_mm as f32).min(pw / 4.0).min(ph / 4.0);
            let mut contents: [Option<Content>; 8] = [None; 8];
            for (n, slot) in sheet.slots.iter().enumerate() {
                contents[n] = index(slot).map(Content::Photo);
            }
            if options.cover {
                contents[0] = Some(Content::Cover {
                    photo: index(&self.manual.cover),
                });
                contents[7] = Some(Content::Back);
            }
            return PageLayout {
                size,
                pages: vec![Page {
                    background: sheet.background,
                    ..Page::new(sheet_cells(&contents, size, margin))
                }],
                left_out: 0,
            };
        }

        let (w, h) = page_size(options, false);
        let margin = options.margin_mm as f32;
        let area = Rect {
            x: margin,
            y: margin,
            w: (w - 2.0 * margin).max(10.0),
            h: (h - 2.0 * margin).max(10.0),
        };
        let mut pages = Vec::new();
        if options.cover {
            pages.push(Page::new(vec![Cell::plain(
                area,
                Content::Cover {
                    photo: index(&self.manual.cover),
                },
                false,
            )]));
        }
        for page in &self.manual.pages {
            if page.is_free() {
                let cells = page
                    .items
                    .iter()
                    .filter_map(|item| {
                        let rect = Rect {
                            x: item.x * w,
                            y: item.y * h,
                            w: item.w * w,
                            h: item.h * h,
                        };
                        // A text box is its own cell; a frame with no photo
                        // in it has nothing to print.
                        let content = match (&item.text, &item.photo) {
                            (Some(_), _) => Content::Text,
                            (None, Some(photo)) => Content::Photo(self.position(photo)?),
                            (None, None) => return None,
                        };
                        Some(Cell {
                            rotation: item.rotation,
                            frame: item.frame,
                            text: item.text.clone(),
                            ..Cell::plain(rect, content, false)
                        })
                    })
                    .collect();
                pages.push(Page {
                    background: page.background,
                    ..Page::new(cells)
                });
                continue;
            }
            let rects = grid(area, page.slots.len().clamp(1, 4), w > h);
            let cells = page
                .slots
                .iter()
                .enumerate()
                .filter_map(|(n, slot)| {
                    Some(Cell::plain(*rects.get(n)?, Content::Photo(index(slot)?), false))
                })
                .collect();
            pages.push(Page {
                background: page.background,
                ..Page::new(cells)
            });
        }
        PageLayout {
            size: (w, h),
            pages,
            left_out: 0,
        }
    }

    /// Fills the manual pages as the automatic layout would, as somewhere
    /// to start from.
    pub fn seed_manual(&mut self) {
        let photos = self.photos.clone();
        let take = |i: usize| photos.get(i).cloned();
        let mut manual = ManualLayout {
            started: true,
            ..Default::default()
        };
        let mut next = 0;
        if self.options.cover {
            manual.cover = take(0);
            next = photos.len().min(1);
        }
        if self.options.layout == ZineLayout::FoldedSheet {
            let mut page = ManualPage::new(SHEET_PANELS);
            // With a cover the first and last panels are the cover and back.
            let (first, last) = if self.options.cover { (1, SHEET_PANELS - 1) } else { (0, SHEET_PANELS) };
            for slot in first..last {
                page.slots[slot] = take(next);
                next += 1;
            }
            manual.pages.push(page);
        } else {
            let per = self.options.layout.per_page();
            while next < photos.len() {
                let mut page = ManualPage::new(per);
                for slot in page.slots.iter_mut() {
                    *slot = take(next);
                    next += 1;
                }
                manual.pages.push(page);
            }
        }
        self.manual = manual;
    }

    /// Where each slot of manual page `page` sits (and whether it prints
    /// upside down), with the page's size: for drawing the page.
    pub fn slot_places(&self, page: usize) -> Option<((f32, f32), Vec<(Rect, bool)>)> {
        let options = &self.options;
        let manual = self.manual.pages.get(page)?;
        if manual.is_free() {
            return None;
        }
        if manual.is_sheet() {
            let size = page_size(options, true);
            let (pw, ph) = (size.0 / 4.0, size.1 / 2.0);
            let margin = (options.margin_mm as f32).min(pw / 4.0).min(ph / 4.0);
            let contents: [Option<Content>; 8] = std::array::from_fn(|n| Some(Content::Photo(n)));
            let mut places = vec![(Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }, false); SHEET_PANELS];
            for cell in sheet_cells(&contents, size, margin) {
                if let Content::Photo(n) = cell.content {
                    places[n] = (cell.rect, cell.rotated);
                }
            }
            return Some((size, places));
        }
        let (w, h) = page_size(options, false);
        let margin = options.margin_mm as f32;
        let area = Rect {
            x: margin,
            y: margin,
            w: (w - 2.0 * margin).max(10.0),
            h: (h - 2.0 * margin).max(10.0),
        };
        let rects = grid(area, manual.slots.len().clamp(1, 4), w > h);
        Some(((w, h), rects.into_iter().map(|r| (r, false)).collect()))
    }

    /// The page size as manual pages have it (and the cover's place on it).
    pub fn manual_page_size(&self) -> (f32, f32) {
        let sheet = self.manual.pages.iter().any(|p| p.is_sheet());
        page_size(&self.options, sheet)
    }

    /// The photo at `spot`.
    pub fn at(&self, spot: Spot) -> Option<&ZinePhoto> {
        match spot {
            Spot::Cover => self.manual.cover.as_ref(),
            Spot::Slot { page, slot } => self.manual.pages.get(page)?.slots.get(slot)?.as_ref(),
        }
    }

    fn at_mut(&mut self, spot: Spot) -> Option<&mut Option<ZinePhoto>> {
        match spot {
            Spot::Cover => Some(&mut self.manual.cover),
            Spot::Slot { page, slot } => self.manual.pages.get_mut(page)?.slots.get_mut(slot),
        }
    }

    /// Whether a spot is part of the design rather than a place for a photo:
    /// a folded sheet's cover and back panels (with a cover).
    pub fn is_fixed(&self, spot: Spot) -> bool {
        match spot {
            Spot::Cover => !self.options.cover,
            Spot::Slot { page, slot } => {
                self.options.cover
                    && self.manual.pages.get(page).is_some_and(|p| p.is_sheet())
                    && (slot == 0 || slot == SHEET_PANELS - 1)
            }
        }
    }

    /// Puts `photo` at `to`. When it was dragged from `from`, the two spots
    /// swap what they hold.
    pub fn place(&mut self, photo: ZinePhoto, to: Spot, from: Option<Spot>) {
        if self.is_fixed(to) || !self.contains(&photo) || from == Some(to) {
            return;
        }
        let displaced = self.at(to).cloned();
        if let Some(slot) = self.at_mut(to) {
            *slot = Some(photo);
        } else {
            return;
        }
        if let Some(from) = from {
            if let Some(slot) = self.at_mut(from) {
                *slot = displaced;
            }
        }
    }

    /// Empties `spot`.
    pub fn clear_spot(&mut self, spot: Spot) {
        if let Some(slot) = self.at_mut(spot) {
            *slot = None;
        }
    }

    /// Adds a page of `slots` slots (1, 2 or 4) at the end.
    pub fn add_page(&mut self, slots: usize) {
        self.manual.pages.push(ManualPage::new(slots.clamp(1, 4)));
    }

    pub fn remove_page(&mut self, page: usize) {
        if page < self.manual.pages.len() {
            self.manual.pages.remove(page);
        }
    }

    /// Takes page `from` out and puts it where page `to` was. Returns where
    /// it ended up.
    pub fn move_page(&mut self, from: usize, to: usize) -> Option<usize> {
        let len = self.manual.pages.len();
        if from >= len || to >= len {
            return None;
        }
        let page = self.manual.pages.remove(from);
        self.manual.pages.insert(to, page);
        Some(to)
    }

    /// Moves a page one place earlier (`-1`) or later (`1`).
    pub fn shift_page(&mut self, page: usize, by: i32) -> Option<usize> {
        let to = page as i64 + by as i64;
        if page >= self.manual.pages.len() || to < 0 || to as usize >= self.manual.pages.len() {
            return None;
        }
        self.manual.pages.swap(page, to as usize);
        Some(to as usize)
    }

    /// Gives a page 1, 2 or 4 slots, keeping the photos that still fit. A
    /// free page's photos go into the slots in their stacking order.
    pub fn set_page_slots(&mut self, page: usize, slots: usize) {
        if let Some(page) = self.manual.pages.get_mut(page) {
            if page.is_sheet() {
                return;
            }
            let slots = slots.clamp(1, 4);
            if page.is_free() {
                let mut taken = std::mem::take(&mut page.items)
                    .into_iter()
                    .filter_map(|i| i.photo);
                page.slots = (0..slots).map(|_| taken.next()).collect();
            } else {
                page.slots.resize(slots, None);
            }
        }
    }

    /// Adds an empty free page at the end.
    pub fn add_free_page(&mut self) {
        self.manual.pages.push(ManualPage::free());
    }

    /// The boxes of a page as free items: a free page's own, or a grid
    /// page's slots where they sit.
    pub fn page_items(&self, page: usize) -> Vec<FreeItem> {
        let Some(manual) = self.manual.pages.get(page) else {
            return Vec::new();
        };
        if manual.is_free() {
            return manual.items.clone();
        }
        let Some((size, rects)) = self.slot_places(page) else {
            return Vec::new();
        };
        manual
            .slots
            .iter()
            .zip(rects)
            .map(|(slot, (rect, _))| {
                FreeItem::photo_frame(
                    slot.clone(),
                    (rect.x / size.0, rect.y / size.1, rect.w / size.0, rect.h / size.1),
                    FrameStyle::default(),
                )
            })
            .collect()
    }

    /// Turns a page into a free one, its photos where its slots were.
    pub fn make_page_free(&mut self, page: usize) {
        let Some(manual) = self.manual.pages.get(page) else {
            return;
        };
        if manual.is_free() || manual.is_sheet() {
            return;
        }
        let items: Vec<FreeItem> = self
            .page_items(page)
            .into_iter()
            .filter(|i| i.photo.is_some())
            .collect();
        let manual = &mut self.manual.pages[page];
        manual.slots.clear();
        manual.items = items;
    }

    fn free_page_mut(&mut self, page: usize) -> Option<&mut ManualPage> {
        self.manual.pages.get_mut(page).filter(|p| p.is_free())
    }

    /// Puts `photo` on free page `page` in `rect` (page fractions), in front
    /// of the rest. Returns its index there.
    pub fn free_place(&mut self, page: usize, photo: ZinePhoto, rect: (f32, f32, f32, f32)) -> Option<usize> {
        if !self.contains(&photo) {
            return None;
        }
        let page = self.free_page_mut(page)?;
        page.items
            .push(FreeItem::photo_frame(Some(photo), rect, FrameStyle::default()).clamped());
        Some(page.items.len() - 1)
    }

    /// Puts a box of text on free page `page`. Returns its index there.
    pub fn free_add_text(&mut self, page: usize, text: TextBox, rect: (f32, f32, f32, f32)) -> Option<usize> {
        let page = self.free_page_mut(page)?;
        page.items.push(FreeItem::text_box(text, rect).clamped());
        Some(page.items.len() - 1)
    }

    /// Puts `photo` in the frame `item` of free page `page` (swapping with
    /// the photo that was there is the caller's business).
    pub fn free_fill_frame(&mut self, page: usize, item: usize, photo: ZinePhoto) -> bool {
        if !self.contains(&photo) {
            return false;
        }
        match self
            .free_page_mut(page)
            .and_then(|p| p.items.get_mut(item))
            .filter(|i| !i.is_text())
        {
            Some(item) => {
                item.photo = Some(photo);
                // A new photo starts centred in its frame.
                item.frame.focus = (0.5, 0.5);
                true
            }
            None => false,
        }
    }

    /// Moves or resizes a box on a free page.
    pub fn free_set_rect(&mut self, page: usize, item: usize, rect: (f32, f32, f32, f32)) {
        if let Some(item) = self.free_page_mut(page).and_then(|p| p.items.get_mut(item)) {
            *item = item.clone().with_rect(rect);
        }
    }

    /// Changes a box on a free page.
    pub fn free_edit(&mut self, page: usize, item: usize, change: impl FnOnce(&mut FreeItem)) {
        if let Some(item) = self.free_page_mut(page).and_then(|p| p.items.get_mut(item)) {
            change(item);
            *item = item.clone().clamped();
        }
    }

    pub fn free_remove(&mut self, page: usize, item: usize) {
        if let Some(page) = self.manual.pages.get_mut(page) {
            if item < page.items.len() {
                page.items.remove(item);
            }
        }
    }

    /// Brings a box to the front of its page. Returns its new index.
    pub fn free_raise(&mut self, page: usize, item: usize) -> Option<usize> {
        let page = self.manual.pages.get_mut(page)?;
        if item >= page.items.len() {
            return None;
        }
        let it = page.items.remove(item);
        page.items.push(it);
        Some(page.items.len() - 1)
    }

    /// A copy of a box, a little way down and across, in front. Returns its
    /// index.
    pub fn free_duplicate(&mut self, page: usize, item: usize) -> Option<usize> {
        let page = self.free_page_mut(page)?;
        let mut copy = page.items.get(item)?.clone();
        copy.x += 0.03;
        copy.y += 0.03;
        page.items.push(copy.clamped());
        Some(page.items.len() - 1)
    }

    /// Moves several boxes of a free page by the same amount, keeping each
    /// on the page.
    pub fn free_nudge(&mut self, page: usize, items: &[usize], dx: f32, dy: f32) {
        for &item in items {
            self.free_edit(page, item, |i| {
                i.x += dx;
                i.y += dy;
            });
        }
    }

    /// Lines up boxes of a free page (two or more).
    pub fn free_align(&mut self, page: usize, items: &[usize], mode: AlignMode) {
        self.rewrite_rects(page, items, |rects| align_rects(rects, mode));
    }

    /// Spaces boxes of a free page evenly (three or more).
    pub fn free_distribute(&mut self, page: usize, items: &[usize], axis: DistributeAxis) {
        self.rewrite_rects(page, items, |rects| distribute_rects(rects, axis));
    }

    fn rewrite_rects(&mut self, page: usize, items: &[usize], change: impl FnOnce(&mut [(f32, f32, f32, f32)])) {
        let Some(p) = self.manual.pages.get(page) else {
            return;
        };
        let items: Vec<usize> = items.iter().copied().filter(|&i| i < p.items.len()).collect();
        let mut rects: Vec<_> = items.iter().map(|&i| p.items[i].rect()).collect();
        if rects.len() < 2 {
            return;
        }
        change(&mut rects);
        for (&i, rect) in items.iter().zip(rects) {
            self.free_set_rect(page, i, rect);
        }
    }

    /// Gives a page its own paper colour (`None` goes back to the zine's).
    pub fn set_page_background(&mut self, page: usize, colour: Option<[u8; 3]>) {
        if let Some(page) = self.manual.pages.get_mut(page) {
            page.background = colour;
        }
    }

    /// A copy of a page right after it. Returns its index.
    pub fn duplicate_page(&mut self, page: usize) -> Option<usize> {
        let copy = self.manual.pages.get(page)?.clone();
        self.manual.pages.insert(page + 1, copy);
        Some(page + 1)
    }

    /// Makes the page `template`: its boxes replaced by the design's, the
    /// frames filled from the photos the page held, then from photos on no
    /// page yet. Returns how many frames got a photo.
    pub fn apply_template(&mut self, page: usize, template: ZineTemplate) -> usize {
        let Some(manual) = self.manual.pages.get(page) else {
            return 0;
        };
        if manual.is_sheet() {
            return 0;
        }
        let (w, h) = self.manual_page_size();
        let m = self.options.margin_mm as f32;
        let mut items = template.items((m / w, m / h), self.options.font);
        // The page's own photos first, then ones not yet placed.
        let mut supply: Vec<ZinePhoto> = self.page_items(page).into_iter().filter_map(|i| i.photo).collect();
        for photo in &self.photos {
            if self.placements(photo) == 0 && !supply.contains(photo) {
                supply.push(photo.clone());
            }
        }
        let mut supply = supply.into_iter();
        let mut filled = 0;
        for item in items.iter_mut().filter(|i| !i.is_text()) {
            if let Some(photo) = supply.next() {
                item.photo = Some(photo);
                filled += 1;
            }
        }
        let manual = &mut self.manual.pages[page];
        manual.slots.clear();
        manual.items = items;
        filled
    }

    /// Sets the zine's look: type and paper, and the type of the text
    /// already on its pages.
    pub fn apply_style(&mut self, style: ZineStyle) {
        self.options.style = style;
        self.options.font = style.font();
        self.options.paper = style.paper();
        for page in &mut self.manual.pages {
            for item in &mut page.items {
                if let Some(text) = item.text.as_mut() {
                    text.font = style.font();
                }
            }
        }
    }

    /// How many places `photo` has on the pages.
    pub fn placements(&self, photo: &ZinePhoto) -> usize {
        let on_pages = self
            .manual
            .pages
            .iter()
            .map(|p| {
                p.slots.iter().filter(|s| s.as_ref() == Some(photo)).count()
                    + p.items.iter().filter(|i| i.photo.as_ref() == Some(photo)).count()
            })
            .sum::<usize>();
        on_pages + usize::from(self.manual.cover.as_ref() == Some(photo))
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

    fn zine_of(n: usize, options: ZineOptions) -> Zine {
        Zine {
            photos: (0..n).map(|i| photo(&format!("p{i}"))).collect(),
            options: ZineOptions { mode: ZineMode::Manual, ..options },
            manual: ManualLayout::default(),
        }
    }

    #[test]
    fn seeding_matches_the_automatic_layout() {
        let mut zine = zine_of(6, options(ZineLayout::TwoPerPage, true));
        zine.seed_manual();
        assert_eq!(zine.manual.cover, Some(photo("p0")));
        assert_eq!(zine.manual.pages.len(), 3);
        assert_eq!(zine.manual.pages[0].slots, vec![Some(photo("p1")), Some(photo("p2"))]);
        assert_eq!(zine.manual.pages[2].slots, vec![Some(photo("p5")), None]);
        let manual = zine.layout();
        let auto = layout_pages(6, &ZineOptions { mode: ZineMode::Auto, ..zine.options.clone() });
        assert_eq!(manual.pages.len(), auto.pages.len());
        assert_eq!(manual, auto);
    }

    #[test]
    fn photos_are_placed_swapped_and_cleared() {
        let mut zine = zine_of(4, options(ZineLayout::OnePerPage, false));
        zine.add_page(2);
        zine.add_page(1);
        let slot = |page, slot| Spot::Slot { page, slot };
        zine.place(photo("p2"), slot(0, 1), None);
        zine.place(photo("p0"), slot(1, 0), None);
        assert_eq!(zine.at(slot(0, 1)), Some(&photo("p2")));
        // Dragging a placed photo onto another swaps them.
        zine.place(photo("p2"), slot(1, 0), Some(slot(0, 1)));
        assert_eq!(zine.at(slot(1, 0)), Some(&photo("p2")));
        assert_eq!(zine.at(slot(0, 1)), Some(&photo("p0")));
        // Onto itself, or a photo not in the zine, does nothing.
        zine.place(photo("p2"), slot(1, 0), Some(slot(1, 0)));
        zine.place(photo("zz"), slot(0, 0), None);
        assert_eq!(zine.at(slot(0, 0)), None);
        assert_eq!(zine.placements(&photo("p2")), 1);
        zine.clear_spot(slot(1, 0));
        assert_eq!(zine.placements(&photo("p2")), 0);
        // Taking a photo out of the zine takes it off its page.
        zine.remove(&[photo("p0")]);
        assert_eq!(zine.at(slot(0, 1)), None);
    }

    #[test]
    fn manual_pages_become_cells_only_for_filled_slots() {
        let mut zine = zine_of(3, options(ZineLayout::OnePerPage, false));
        zine.add_page(4);
        zine.place(photo("p2"), Spot::Slot { page: 0, slot: 3 }, None);
        zine.place(photo("p0"), Spot::Slot { page: 0, slot: 0 }, None);
        let layout = zine.layout();
        assert_eq!(layout.pages.len(), 1);
        let cells = &layout.pages[0].cells;
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].content, Content::Photo(0));
        assert_eq!(cells[1].content, Content::Photo(2));
        // Slot 3 of four is the bottom right.
        assert!(cells[1].rect.x > cells[0].rect.x && cells[1].rect.y > cells[0].rect.y);
        // Pages are reordered and resized.
        zine.add_page(1);
        assert_eq!(zine.shift_page(1, -1), Some(0));
        assert_eq!(zine.manual.pages[1].slots.len(), 4);
        zine.set_page_slots(1, 2);
        assert_eq!(zine.manual.pages[1].slots.len(), 2);
        assert_eq!(zine.at(Spot::Slot { page: 1, slot: 0 }), Some(&photo("p0")));
    }

    #[test]
    fn a_manual_folded_sheet_keeps_its_cover_and_back_fixed() {
        let mut zine = zine_of(9, options(ZineLayout::FoldedSheet, true));
        zine.seed_manual();
        assert_eq!(zine.manual.pages.len(), 1);
        assert!(zine.manual.pages[0].is_sheet());
        let first = Spot::Slot { page: 0, slot: 0 };
        let last = Spot::Slot { page: 0, slot: 7 };
        assert!(zine.is_fixed(first) && zine.is_fixed(last));
        assert!(!zine.is_fixed(Spot::Slot { page: 0, slot: 3 }));
        zine.place(photo("p8"), first, None);
        assert_eq!(zine.at(first), None);
        zine.place(photo("p8"), Spot::Cover, None);
        let layout = zine.layout();
        let cells = &layout.pages[0].cells;
        assert_eq!(cells.len(), 8);
        assert!(cells.iter().any(|c| c.content == Content::Cover { photo: Some(8) }));
        assert!(cells.iter().any(|c| c.content == Content::Back));
        // With no cover, every panel takes a photo.
        let mut plain = zine_of(9, options(ZineLayout::FoldedSheet, false));
        plain.seed_manual();
        assert!(!plain.is_fixed(Spot::Slot { page: 0, slot: 0 }));
        assert_eq!(plain.layout().pages[0].cells.len(), 8);
    }

    #[test]
    fn photos_are_placed_moved_and_stacked_on_a_free_page() {
        let mut zine = zine_of(3, options(ZineLayout::OnePerPage, false));
        zine.add_free_page();
        assert!(zine.manual.pages[0].is_free());
        let a = zine.free_place(0, photo("p0"), (0.1, 0.1, 0.4, 0.3)).unwrap();
        let b = zine.free_place(0, photo("p1"), (0.2, 0.2, 0.4, 0.3)).unwrap();
        assert_eq!((a, b), (0, 1));
        // A photo that is not in the zine is refused; a box is kept on the page.
        assert_eq!(zine.free_place(0, photo("zz"), (0.0, 0.0, 0.2, 0.2)), None);
        zine.free_set_rect(0, 0, (0.9, 0.9, 0.5, 0.5));
        let item = &zine.manual.pages[0].items[0];
        assert!((item.x + item.w - 1.0).abs() < 1e-5 && (item.y + item.h - 1.0).abs() < 1e-5);
        zine.free_set_rect(0, 1, (0.5, 0.5, 0.0, 0.0));
        assert_eq!(zine.manual.pages[0].items[1].w, MIN_ITEM);
        // Raising puts it in front.
        assert_eq!(zine.free_raise(0, 0), Some(1));
        assert_eq!(zine.manual.pages[0].items[1].photo, Some(photo("p0")));
        assert_eq!(zine.placements(&photo("p0")), 1);
        // The layout has a cell for each, in page millimetres.
        let layout = zine.layout();
        assert_eq!(layout.pages[0].cells.len(), 2);
        let last = layout.pages[0].cells[1].rect;
        assert!(last.x + last.w <= layout.size.0 + 0.01);
        zine.remove(&[photo("p1")]);
        // Its frame stays, empty; the layout has nothing to print for it.
        assert_eq!(zine.manual.pages[0].items.len(), 2);
        assert_eq!(zine.layout().pages[0].cells.len(), 1);
        zine.free_remove(0, 0);
        zine.free_remove(0, 0);
        assert!(zine.manual.pages[0].items.is_empty());
    }

    #[test]
    fn a_page_changes_between_slots_and_free_keeping_its_photos() {
        let mut zine = zine_of(3, options(ZineLayout::OnePerPage, false));
        zine.add_page(2);
        zine.place(photo("p0"), Spot::Slot { page: 0, slot: 0 }, None);
        zine.place(photo("p2"), Spot::Slot { page: 0, slot: 1 }, None);
        zine.make_page_free(0);
        let page = &zine.manual.pages[0];
        assert!(page.is_free() && page.items.len() == 2);
        // Slots two to a portrait page are stacked: the second is lower.
        assert!(page.items[1].y > page.items[0].y);
        zine.set_page_slots(0, 4);
        let page = &zine.manual.pages[0];
        assert!(!page.is_free() && page.slots.len() == 4);
        assert_eq!(page.slots[0], Some(photo("p0")));
        assert_eq!(page.slots[1], Some(photo("p2")));
    }

    #[test]
    fn dragged_photos_snap_to_the_margins_and_the_middle() {
        let margin = (0.05, 0.05);
        // Near the left margin: snaps to it, with a guide.
        let (rect, guides) = snap_move((0.062, 0.3, 0.3, 0.2), margin, 0.015);
        assert!((rect.0 - 0.05).abs() < 1e-5);
        assert!(guides.contains(&Guide { vertical: true, at: 0.05 }));
        // Centred on the page's middle.
        let (rect, _) = snap_move((0.36, 0.3, 0.3, 0.2), margin, 0.02);
        assert!((rect.0 + rect.2 / 2.0 - 0.5).abs() < 1e-5);
        // Far from every line: untouched.
        assert_eq!(snap_move((0.2, 0.2, 0.1, 0.1), margin, 0.01), ((0.2, 0.2, 0.1, 0.1), vec![]));
        // A corner dragged near the right margin snaps its edge.
        let (rect, guides) = snap_corner((0.4, 0.4, 0.54, 0.2), margin, 0.015);
        assert!((rect.0 + rect.2 - 0.95).abs() < 1e-5 && guides.len() == 1);
    }

    #[test]
    fn text_boxes_and_styled_frames_reach_the_layout() {
        let mut zine = zine_of(2, options(ZineLayout::OnePerPage, false));
        zine.add_free_page();
        let rect = (0.1, 0.1, 0.5, 0.4);
        zine.free_place(0, photo("p0"), rect);
        zine.free_edit(0, 0, |i| {
            i.rotation = 12.0;
            i.frame.fit = Fit::Fill;
            i.frame.mat_mm = 4.0;
            i.frame.shadow = true;
        });
        let text = zine.free_add_text(0, TextBox::new("Hello", ZineFont::Serif), (0.1, 0.6, 0.5, 0.1));
        assert_eq!(text, Some(1));
        // An empty frame has nothing to print.
        zine.manual.pages[0]
            .items
            .push(FreeItem::photo_frame(None, (0.7, 0.1, 0.2, 0.2), FrameStyle::default()));
        zine.set_page_background(0, Some([10, 20, 30]));
        let layout = zine.layout();
        let page = &layout.pages[0];
        assert_eq!(page.background, Some([10, 20, 30]));
        assert_eq!(page.cells.len(), 2);
        assert_eq!(page.cells[0].rotation, 12.0);
        assert_eq!(page.cells[0].frame.fit, Fit::Fill);
        assert_eq!(page.cells[1].content, Content::Text);
        assert_eq!(page.cells[1].text.as_ref().unwrap().text, "Hello");
        // Taking the photo out of the zine empties its frame but keeps it.
        zine.remove(&[photo("p0")]);
        assert_eq!(zine.manual.pages[0].items.len(), 3);
        assert!(zine.manual.pages[0].items[0].photo.is_none());
        assert_eq!(zine.free_fill_frame(0, 0, photo("p1")), true);
        assert_eq!(zine.free_fill_frame(0, 1, photo("p1")), false, "text is not a frame");
    }

    #[test]
    fn a_template_fills_its_frames_from_the_photos_not_yet_placed() {
        let mut zine = zine_of(6, options(ZineLayout::OnePerPage, false));
        zine.add_free_page();
        // p0 is already on this page, p1 on another.
        zine.free_place(0, photo("p0"), (0.1, 0.1, 0.3, 0.3));
        zine.add_page(1);
        zine.place(photo("p1"), Spot::Slot { page: 1, slot: 0 }, None);
        let filled = zine.apply_template(0, ZineTemplate::GridQuote);
        assert_eq!(filled, 4);
        let items = &zine.manual.pages[0].items;
        let photos: Vec<_> = items.iter().filter_map(|i| i.photo.clone()).collect();
        // The page's own photo first, then the unplaced ones in order (not p1).
        assert_eq!(photos, vec![photo("p0"), photo("p2"), photo("p3"), photo("p4")]);
        assert!(items.iter().any(|i| i.is_text()));
        // Fewer photos than frames leaves frames empty.
        let mut few = zine_of(1, options(ZineLayout::OnePerPage, false));
        few.add_free_page();
        assert_eq!(few.apply_template(0, ZineTemplate::Triptych), 1);
        assert_eq!(few.manual.pages[0].items.iter().filter(|i| i.photo.is_none() && !i.is_text()).count(), 2);
        // A template's text uses the zine's font.
        let mut serif = zine_of(1, ZineOptions { font: ZineFont::Serif, ..options(ZineLayout::OnePerPage, false) });
        serif.add_free_page();
        serif.apply_template(0, ZineTemplate::HeroCaption);
        assert!(serif.manual.pages[0].items.iter().filter_map(|i| i.text.as_ref()).all(|t| t.font == ZineFont::Serif));
    }

    #[test]
    fn a_style_restyles_the_paper_and_every_text_box() {
        let mut zine = zine_of(2, options(ZineLayout::OnePerPage, false));
        zine.add_free_page();
        zine.free_add_text(0, TextBox::new("x", ZineFont::Sans), (0.1, 0.1, 0.3, 0.1));
        zine.apply_style(ZineStyle::Typewriter);
        assert_eq!(zine.options.font, ZineFont::Mono);
        assert_eq!(zine.options.paper, ZinePaper::Cream);
        assert_eq!(zine.manual.pages[0].items[0].text.as_ref().unwrap().font, ZineFont::Mono);
    }

    #[test]
    fn several_boxes_are_nudged_aligned_distributed_and_pages_duplicated() {
        let mut zine = zine_of(3, options(ZineLayout::OnePerPage, false));
        zine.add_free_page();
        zine.free_place(0, photo("p0"), (0.05, 0.1, 0.2, 0.2));
        zine.free_place(0, photo("p1"), (0.40, 0.3, 0.2, 0.2));
        zine.free_place(0, photo("p2"), (0.70, 0.6, 0.2, 0.2));
        zine.free_nudge(0, &[0, 1, 2], 0.0, 0.05);
        assert!((zine.manual.pages[0].items[0].y - 0.15).abs() < 1e-5);
        // Nudging past the edge stops at it.
        zine.free_nudge(0, &[2], 1.0, 0.0);
        assert!((zine.manual.pages[0].items[2].x - 0.8).abs() < 1e-5);
        zine.free_align(0, &[0, 1, 2], AlignMode::Top);
        let tops: Vec<f32> = zine.manual.pages[0].items.iter().map(|i| i.y).collect();
        assert!(tops.iter().all(|&y| (y - tops[0]).abs() < 1e-5));
        zine.free_distribute(0, &[0, 1, 2], DistributeAxis::Horizontally);
        let items = &zine.manual.pages[0].items;
        let gap_a = items[1].x - (items[0].x + items[0].w);
        let gap_b = items[2].x - (items[1].x + items[1].w);
        assert!((gap_a - gap_b).abs() < 1e-4);
        // Duplicating a page puts the copy after it, photos and all.
        assert_eq!(zine.duplicate_page(0), Some(1));
        assert_eq!(zine.manual.pages[1], zine.manual.pages[0]);
        assert_eq!(zine.placements(&photo("p1")), 2);
        // And a box can be copied in front.
        assert_eq!(zine.free_duplicate(0, 0), Some(3));
    }

    #[test]
    fn a_page_is_moved_to_where_another_was() {
        let mut zine = zine_of(3, options(ZineLayout::OnePerPage, false));
        for n in 0..4 {
            zine.add_page(1);
            zine.place(photo("p0"), Spot::Slot { page: n, slot: 0 }, None);
            zine.manual.pages[n].background = Some([n as u8, 0, 0]);
        }
        assert_eq!(zine.move_page(0, 2), Some(2));
        let order: Vec<u8> = zine.manual.pages.iter().map(|p| p.background.unwrap()[0]).collect();
        assert_eq!(order, vec![1, 2, 0, 3]);
        assert_eq!(zine.move_page(3, 9), None);
    }
}
