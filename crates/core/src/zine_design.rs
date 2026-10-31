//! The design side of a zine: fonts and text boxes, how a photo sits in its
//! frame (fit, crop, mat, shadow, corners, tilt), style presets, page
//! templates, and aligning or distributing several boxes at once.
//!
//! Boxes are `(x, y, w, h)` fractions of the page.

use serde::{Deserialize, Serialize};

use crate::zine::{Rect, ZinePhoto};

/// A type family. Each maps to a standard PDF font, so nothing is embedded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ZineFont {
    #[default]
    Sans,
    Serif,
    /// A typewriter.
    Mono,
}

impl ZineFont {
    pub const ALL: [ZineFont; 3] = [ZineFont::Sans, ZineFont::Serif, ZineFont::Mono];

    pub fn label(&self) -> &'static str {
        match self {
            ZineFont::Sans => "Sans",
            ZineFont::Serif => "Serif",
            ZineFont::Mono => "Typewriter",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TextAlign {
    #[default]
    Left,
    Centre,
    Right,
}

impl TextAlign {
    pub const ALL: [TextAlign; 3] = [TextAlign::Left, TextAlign::Centre, TextAlign::Right];

    pub fn label(&self) -> &'static str {
        match self {
            TextAlign::Left => "Left",
            TextAlign::Centre => "Centre",
            TextAlign::Right => "Right",
        }
    }
}

/// How a photo meets its frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Fit {
    /// All of the photo, with room round it when the shapes differ.
    #[default]
    Contain,
    /// The frame filled, the photo cropped to do it.
    Fill,
}

impl Fit {
    pub const ALL: [Fit; 2] = [Fit::Contain, Fit::Fill];

    pub fn label(&self) -> &'static str {
        match self {
            Fit::Contain => "Fit",
            Fit::Fill => "Fill",
        }
    }
}

/// The look of a photo's frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FrameStyle {
    pub fit: Fit,
    /// With `Fill`, which part of the photo stays in frame: 0..=1 across
    /// and down (0.5, 0.5 is the middle).
    pub focus: (f32, f32),
    /// A mat (a border) round the photo, in millimetres.
    pub mat_mm: f32,
    pub mat: [u8; 3],
    pub shadow: bool,
    /// Rounded corners, in millimetres.
    pub radius_mm: f32,
}

impl Default for FrameStyle {
    fn default() -> Self {
        Self {
            fit: Fit::Contain,
            focus: (0.5, 0.5),
            mat_mm: 0.0,
            mat: [255, 255, 255],
            shadow: false,
            radius_mm: 0.0,
        }
    }
}

/// The most mat and corner radius the controls offer, in millimetres.
pub const MAX_MAT_MM: f32 = 12.0;
pub const MAX_RADIUS_MM: f32 = 15.0;

/// A box of text on a free page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextBox {
    pub text: String,
    pub font: ZineFont,
    pub size_pt: f32,
    pub bold: bool,
    /// `None` is the paper's own ink colour.
    pub colour: Option<[u8; 3]>,
    pub align: TextAlign,
    /// A tint laid behind the text (half see-through), for text over a
    /// photo.
    pub backdrop: Option<[u8; 3]>,
}

impl Default for TextBox {
    fn default() -> Self {
        Self {
            text: String::new(),
            font: ZineFont::Sans,
            size_pt: 14.0,
            bold: false,
            colour: None,
            align: TextAlign::Left,
            backdrop: None,
        }
    }
}

pub const MIN_TEXT_PT: f32 = 6.0;
pub const MAX_TEXT_PT: f32 = 96.0;

impl TextBox {
    pub fn new(text: &str, font: ZineFont) -> Self {
        Self {
            text: text.to_string(),
            font,
            ..Default::default()
        }
    }
}

/// Something placed on a free page: a photo in a frame (or an empty frame
/// waiting for one), or a box of text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FreeItem {
    pub photo: Option<ZinePhoto>,
    pub text: Option<TextBox>,
    /// Position and size, as page fractions.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Degrees, clockwise, about the box's middle.
    pub rotation: f32,
    pub frame: FrameStyle,
}

impl Default for FreeItem {
    fn default() -> Self {
        Self {
            photo: None,
            text: None,
            x: 0.0,
            y: 0.0,
            w: 0.3,
            h: 0.3,
            rotation: 0.0,
            frame: FrameStyle::default(),
        }
    }
}

/// The smallest a box on a free page can be made, as a page fraction.
pub const MIN_ITEM: f32 = 0.06;

impl FreeItem {
    pub fn photo_frame(photo: Option<ZinePhoto>, rect: (f32, f32, f32, f32), frame: FrameStyle) -> Self {
        Self {
            photo,
            x: rect.0,
            y: rect.1,
            w: rect.2,
            h: rect.3,
            frame,
            ..Default::default()
        }
    }

    pub fn text_box(text: TextBox, rect: (f32, f32, f32, f32)) -> Self {
        Self {
            text: Some(text),
            x: rect.0,
            y: rect.1,
            w: rect.2,
            h: rect.3,
            ..Default::default()
        }
    }

    pub fn rect(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.w, self.h)
    }

    pub fn is_text(&self) -> bool {
        self.text.is_some()
    }

    /// The same item with its box kept on the page and no smaller than
    /// [`MIN_ITEM`].
    pub fn clamped(mut self) -> Self {
        self.w = self.w.clamp(MIN_ITEM, 1.0);
        self.h = self.h.clamp(MIN_ITEM, 1.0);
        self.x = self.x.clamp(0.0, 1.0 - self.w);
        self.y = self.y.clamp(0.0, 1.0 - self.h);
        self
    }

    pub fn with_rect(mut self, rect: (f32, f32, f32, f32)) -> Self {
        (self.x, self.y, self.w, self.h) = rect;
        self.clamped()
    }
}

// --- Style presets --------------------------------------------------------------

/// A look for the whole zine: its type and its paper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ZineStyle {
    #[default]
    Clean,
    Magazine,
    Typewriter,
    Newsprint,
    Darkroom,
}

impl ZineStyle {
    pub const ALL: [ZineStyle; 5] = [
        ZineStyle::Clean,
        ZineStyle::Magazine,
        ZineStyle::Typewriter,
        ZineStyle::Newsprint,
        ZineStyle::Darkroom,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ZineStyle::Clean => "Clean",
            ZineStyle::Magazine => "Magazine",
            ZineStyle::Typewriter => "Typewriter",
            ZineStyle::Newsprint => "Newsprint",
            ZineStyle::Darkroom => "Darkroom",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ZineStyle::Clean => "Sans-serif type on white",
            ZineStyle::Magazine => "A serif on white, like a printed feature",
            ZineStyle::Typewriter => "Typewriter type on cream paper",
            ZineStyle::Newsprint => "A serif on cream, like a newspaper",
            ZineStyle::Darkroom => "Sans-serif type on black",
        }
    }

    pub fn font(&self) -> ZineFont {
        match self {
            ZineStyle::Clean | ZineStyle::Darkroom => ZineFont::Sans,
            ZineStyle::Magazine | ZineStyle::Newsprint => ZineFont::Serif,
            ZineStyle::Typewriter => ZineFont::Mono,
        }
    }

    pub fn paper(&self) -> crate::ZinePaper {
        match self {
            ZineStyle::Clean | ZineStyle::Magazine => crate::ZinePaper::White,
            ZineStyle::Typewriter | ZineStyle::Newsprint => crate::ZinePaper::Cream,
            ZineStyle::Darkroom => crate::ZinePaper::Black,
        }
    }
}

// --- Templates --------------------------------------------------------------------

/// A page design: frames to drop photos into, and text to write over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZineTemplate {
    /// One big photo with a caption strip under it.
    HeroCaption,
    /// Three tall photos side by side, with a caption.
    Triptych,
    /// Four photos in a grid, with a pull quote under them.
    GridQuote,
    /// A photo over the whole page, with text laid on it.
    FullBleed,
}

impl ZineTemplate {
    pub const ALL: [ZineTemplate; 4] = [
        ZineTemplate::HeroCaption,
        ZineTemplate::Triptych,
        ZineTemplate::GridQuote,
        ZineTemplate::FullBleed,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ZineTemplate::HeroCaption => "Hero + caption",
            ZineTemplate::Triptych => "Triptych",
            ZineTemplate::GridQuote => "Grid + quote",
            ZineTemplate::FullBleed => "Full bleed",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ZineTemplate::HeroCaption => "One big photo with a caption strip",
            ZineTemplate::Triptych => "Three tall photos in a row",
            ZineTemplate::GridQuote => "Four photos and a pull quote",
            ZineTemplate::FullBleed => "A photo over the whole page, text on it",
        }
    }

    /// The boxes of the design on a page whose margins are `margin` (page
    /// fractions across and down), its text in `font`. Photo frames are
    /// empty.
    pub fn items(&self, margin: (f32, f32), font: ZineFont) -> Vec<FreeItem> {
        let (mx, my) = (margin.0.clamp(0.0, 0.2), margin.1.clamp(0.0, 0.2));
        let fill = FrameStyle {
            fit: Fit::Fill,
            ..Default::default()
        };
        let width = 1.0 - 2.0 * mx;
        let frame = |rect| FreeItem::photo_frame(None, rect, fill);
        let text = |s: &str, size: f32, bold: bool, align: TextAlign, rect| {
            FreeItem::text_box(
                TextBox {
                    text: s.to_string(),
                    font,
                    size_pt: size,
                    bold,
                    align,
                    ..Default::default()
                },
                rect,
            )
        };
        match self {
            ZineTemplate::HeroCaption => vec![
                frame((mx, my, width, 0.66 - my)),
                text("A caption for this photo", 22.0, true, TextAlign::Left, (mx, 0.70, width, 0.09)),
                text(
                    "A few words about where it was taken, and what was going on.",
                    11.0,
                    false,
                    TextAlign::Left,
                    (mx, 0.80, width, 0.12),
                ),
            ],
            ZineTemplate::Triptych => {
                let gap = 0.02;
                let each = (width - 2.0 * gap) / 3.0;
                let mut items: Vec<FreeItem> = (0..3)
                    .map(|i| frame((mx + i as f32 * (each + gap), my, each, 0.72 - my)))
                    .collect();
                items.push(text("Three moments", 16.0, true, TextAlign::Centre, (mx, 0.77, width, 0.08)));
                items
            }
            ZineTemplate::GridQuote => {
                let gap = 0.02;
                let top = 0.60;
                let cw = (width - gap) / 2.0;
                let ch = (top - my - gap) / 2.0;
                let mut items: Vec<FreeItem> = (0..4)
                    .map(|i| {
                        let (col, row) = ((i % 2) as f32, (i / 2) as f32);
                        frame((mx + col * (cw + gap), my + row * (ch + gap), cw, ch))
                    })
                    .collect();
                items.push(text(
                    "\u{201C}A pull quote worth reading.\u{201D}",
                    24.0,
                    false,
                    TextAlign::Centre,
                    (mx, 0.66, width, 0.24),
                ));
                items
            }
            ZineTemplate::FullBleed => {
                let mut title = text("Title", 40.0, true, TextAlign::Left, (mx.max(0.06), 0.72, 1.0 - 2.0 * mx.max(0.06), 0.2));
                if let Some(t) = title.text.as_mut() {
                    t.colour = Some([255, 255, 255]);
                    t.backdrop = Some([0, 0, 0]);
                }
                vec![frame((0.0, 0.0, 1.0, 1.0)), title]
            }
        }
    }
}

// --- Aligning boxes ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignMode {
    Left,
    CentreHorizontally,
    Right,
    Top,
    CentreVertically,
    Bottom,
}

impl AlignMode {
    pub const ALL: [AlignMode; 6] = [
        AlignMode::Left,
        AlignMode::CentreHorizontally,
        AlignMode::Right,
        AlignMode::Top,
        AlignMode::CentreVertically,
        AlignMode::Bottom,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            AlignMode::Left => "Align left edges",
            AlignMode::CentreHorizontally => "Centre across",
            AlignMode::Right => "Align right edges",
            AlignMode::Top => "Align top edges",
            AlignMode::CentreVertically => "Centre down",
            AlignMode::Bottom => "Align bottom edges",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistributeAxis {
    Horizontally,
    Vertically,
}

/// The box round all of `rects`.
pub fn bounding_box(rects: &[(f32, f32, f32, f32)]) -> Option<(f32, f32, f32, f32)> {
    let first = rects.first()?;
    let (mut x0, mut y0, mut x1, mut y1) = (first.0, first.1, first.0 + first.2, first.1 + first.3);
    for r in rects {
        x0 = x0.min(r.0);
        y0 = y0.min(r.1);
        x1 = x1.max(r.0 + r.2);
        y1 = y1.max(r.1 + r.3);
    }
    Some((x0, y0, x1 - x0, y1 - y0))
}

/// Lines `rects` up on the edge or middle of the box round them all.
pub fn align_rects(rects: &mut [(f32, f32, f32, f32)], mode: AlignMode) {
    let Some((bx, by, bw, bh)) = bounding_box(rects) else {
        return;
    };
    for r in rects.iter_mut() {
        match mode {
            AlignMode::Left => r.0 = bx,
            AlignMode::CentreHorizontally => r.0 = bx + (bw - r.2) / 2.0,
            AlignMode::Right => r.0 = bx + bw - r.2,
            AlignMode::Top => r.1 = by,
            AlignMode::CentreVertically => r.1 = by + (bh - r.3) / 2.0,
            AlignMode::Bottom => r.1 = by + bh - r.3,
        }
    }
}

/// Spaces `rects` evenly along `axis`, the outermost two staying put.
/// Needs three or more.
pub fn distribute_rects(rects: &mut [(f32, f32, f32, f32)], axis: DistributeAxis) {
    if rects.len() < 3 {
        return;
    }
    let (start, size): (fn(&(f32, f32, f32, f32)) -> f32, fn(&(f32, f32, f32, f32)) -> f32) = match axis {
        DistributeAxis::Horizontally => (|r| r.0, |r| r.2),
        DistributeAxis::Vertically => (|r| r.1, |r| r.3),
    };
    let mut order: Vec<usize> = (0..rects.len()).collect();
    order.sort_by(|&a, &b| start(&rects[a]).total_cmp(&start(&rects[b])));
    let first = start(&rects[order[0]]);
    let last = *order.last().unwrap();
    let end = start(&rects[last]) + size(&rects[last]);
    let total: f32 = order.iter().map(|&i| size(&rects[i])).sum();
    let gap = (end - first - total) / (rects.len() - 1) as f32;
    let mut at = first;
    for &i in &order {
        match axis {
            DistributeAxis::Horizontally => rects[i].0 = at,
            DistributeAxis::Vertically => rects[i].1 = at,
        }
        at += size(&rects[i]) + gap;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligning_lines_boxes_up_on_the_group() {
        let mut rects = [(0.1, 0.1, 0.2, 0.2), (0.5, 0.4, 0.3, 0.1), (0.3, 0.7, 0.1, 0.2)];
        align_rects(&mut rects, AlignMode::Left);
        assert!(rects.iter().all(|r| (r.0 - 0.1).abs() < 1e-6));
        align_rects(&mut rects, AlignMode::Bottom);
        // The lowest edge was 0.9.
        assert!(rects.iter().all(|r| (r.1 + r.3 - 0.9).abs() < 1e-6));
        let mut rects = [(0.0, 0.0, 0.2, 0.1), (0.8, 0.5, 0.2, 0.1)];
        align_rects(&mut rects, AlignMode::CentreHorizontally);
        assert!((rects[0].0 - 0.4).abs() < 1e-6 && (rects[1].0 - 0.4).abs() < 1e-6);
    }

    #[test]
    fn distributing_leaves_equal_gaps_and_the_ends_in_place() {
        let mut rects = [(0.0, 0.0, 0.1, 0.1), (0.15, 0.0, 0.2, 0.1), (0.8, 0.0, 0.2, 0.1)];
        distribute_rects(&mut rects, DistributeAxis::Horizontally);
        assert_eq!(rects[0].0, 0.0);
        assert!((rects[2].0 - 0.8).abs() < 1e-6);
        let gap_a = rects[1].0 - (rects[0].0 + rects[0].2);
        let gap_b = rects[2].0 - (rects[1].0 + rects[1].2);
        assert!((gap_a - gap_b).abs() < 1e-5, "{gap_a} vs {gap_b}");
        // Two boxes have nothing to distribute.
        let mut two = [(0.0, 0.0, 0.1, 0.1), (0.5, 0.0, 0.1, 0.1)];
        let before = two;
        distribute_rects(&mut two, DistributeAxis::Horizontally);
        assert_eq!(two, before);
        assert_eq!(bounding_box(&[]), None);
    }

    #[test]
    fn every_template_stays_on_the_page_and_has_frames_and_text() {
        for template in ZineTemplate::ALL {
            let items = template.items((0.06, 0.05), ZineFont::Serif);
            assert!(items.iter().any(|i| i.photo.is_none() && !i.is_text()), "{template:?} has frames");
            assert!(items.iter().any(|i| i.is_text()), "{template:?} has text");
            for item in &items {
                assert!(item.x >= -1e-4 && item.y >= -1e-4);
                assert!(item.x + item.w <= 1.0 + 1e-4 && item.y + item.h <= 1.0 + 1e-4, "{template:?}");
            }
            // No two photo frames overlap.
            let frames: Vec<_> = items.iter().filter(|i| !i.is_text()).collect();
            for (a, fa) in frames.iter().enumerate() {
                for fb in frames.iter().skip(a + 1) {
                    let apart = fa.x + fa.w <= fb.x + 1e-4
                        || fb.x + fb.w <= fa.x + 1e-4
                        || fa.y + fa.h <= fb.y + 1e-4
                        || fb.y + fb.h <= fa.y + 1e-4;
                    assert!(apart, "{template:?} overlaps");
                }
            }
        }
        assert_eq!(ZineTemplate::Triptych.items((0.05, 0.05), ZineFont::Sans).len(), 4);
        assert_eq!(ZineTemplate::GridQuote.items((0.05, 0.05), ZineFont::Sans).len(), 5);
        // The full-bleed photo covers the page; its text is white on a tint.
        let bleed = ZineTemplate::FullBleed.items((0.05, 0.05), ZineFont::Sans);
        assert_eq!(bleed[0].rect(), (0.0, 0.0, 1.0, 1.0));
        assert_eq!(bleed[1].text.as_ref().unwrap().colour, Some([255, 255, 255]));
    }

    #[test]
    fn a_box_is_kept_on_the_page_and_a_style_has_a_font_and_paper() {
        let item = FreeItem::photo_frame(None, (0.9, 0.9, 0.5, 0.5), FrameStyle::default()).clamped();
        assert!(item.x + item.w <= 1.0 + 1e-6 && item.y + item.h <= 1.0 + 1e-6);
        assert_eq!(ZineStyle::Typewriter.font(), ZineFont::Mono);
        assert_eq!(ZineStyle::Darkroom.paper(), crate::ZinePaper::Black);
    }

    #[test]
    fn an_older_saved_photo_item_still_loads() {
        let old = r#"{"photo":{"path":"/a.jpg","copy_name":null},"x":0.1,"y":0.2,"w":0.3,"h":0.4}"#;
        let item: FreeItem = serde_json::from_str(old).unwrap();
        assert_eq!(item.photo.unwrap().path, std::path::PathBuf::from("/a.jpg"));
        assert_eq!(item.frame, FrameStyle::default());
        assert!(item.text.is_none() && item.rotation == 0.0);
    }
}

// --- Where a photo goes in its frame --------------------------------------------

/// Where the parts of a framed photo sit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhotoGeometry {
    /// The frame with its mat: what a shadow and the mat colour cover.
    pub outer: Rect,
    /// What is left inside the mat: the picture shows through this.
    pub inner: Rect,
    /// The picture itself, scaled to fit or fill. With `Fill` it reaches
    /// past `inner` and is cut off there.
    pub image: Rect,
    /// The corner radius of `outer` and of `inner`.
    pub outer_radius: f32,
    pub inner_radius: f32,
}

impl Rect {
    pub fn grown(&self, by: f32) -> Rect {
        Rect {
            x: self.x - by,
            y: self.y - by,
            w: self.w + 2.0 * by,
            h: self.h + 2.0 * by,
        }
    }

    pub fn shrunk(&self, by: f32) -> Rect {
        let by = by.min(self.w / 2.0 - 0.01).min(self.h / 2.0 - 0.01).max(0.0);
        self.grown(-by)
    }
}

/// A picture of `size` pixels centred and as large as it can be inside
/// `area`, keeping its shape.
pub fn fit_rect(area: Rect, size: (u32, u32)) -> Rect {
    let aspect = size.0.max(1) as f32 / size.1.max(1) as f32;
    let (mut w, mut h) = (area.w, area.w / aspect);
    if h > area.h {
        (w, h) = (area.h * aspect, area.h);
    }
    Rect {
        x: area.x + (area.w - w) / 2.0,
        y: area.y + (area.h - h) / 2.0,
        w,
        h,
    }
}

/// A picture of `size` pixels scaled to cover `area`, shifted so `focus`
/// (0..=1 across and down) of it is what stays in view.
pub fn fill_rect(area: Rect, size: (u32, u32), focus: (f32, f32)) -> Rect {
    let aspect = size.0.max(1) as f32 / size.1.max(1) as f32;
    let (mut w, mut h) = (area.w, area.w / aspect);
    if h < area.h {
        (w, h) = (area.h * aspect, area.h);
    }
    Rect {
        x: area.x - (w - area.w) * focus.0.clamp(0.0, 1.0),
        y: area.y - (h - area.h) * focus.1.clamp(0.0, 1.0),
        w,
        h,
    }
}

/// Lays a photo of `size` pixels in `frame_box` (less `caption_room` kept
/// under it), as `frame` says.
pub fn photo_geometry(
    frame_box: Rect,
    frame: &FrameStyle,
    size: (u32, u32),
    caption_room: f32,
) -> PhotoGeometry {
    let pic = Rect {
        h: (frame_box.h - caption_room).max(1.0),
        ..frame_box
    };
    let mat = frame.mat_mm.clamp(0.0, MAX_MAT_MM);
    let radius = frame.radius_mm.clamp(0.0, MAX_RADIUS_MM);
    let inner_radius = (radius - mat).max(0.0);
    match frame.fit {
        Fit::Contain => {
            let image = fit_rect(pic.shrunk(mat), size);
            PhotoGeometry {
                outer: image.grown(mat),
                inner: image,
                image,
                outer_radius: radius,
                inner_radius,
            }
        }
        Fit::Fill => {
            let inner = pic.shrunk(mat);
            PhotoGeometry {
                outer: pic,
                inner,
                image: fill_rect(inner, size, frame.focus),
                outer_radius: radius,
                inner_radius,
            }
        }
    }
}

/// The ink that reads on paper of colour `paper`.
pub fn ink_on(paper: [u8; 3]) -> [u8; 3] {
    let luma = 0.299 * paper[0] as f32 + 0.587 * paper[1] as f32 + 0.114 * paper[2] as f32;
    if luma < 115.0 {
        [235, 235, 235]
    } else {
        [30, 30, 30]
    }
}

#[cfg(test)]
mod geometry_tests {
    use super::*;

    fn area() -> Rect {
        Rect { x: 10.0, y: 10.0, w: 100.0, h: 100.0 }
    }

    #[test]
    fn a_fitted_photo_keeps_its_shape_and_the_mat_hugs_it() {
        let frame = FrameStyle { mat_mm: 5.0, ..Default::default() };
        let g = photo_geometry(area(), &frame, (400, 200), 0.0);
        // 90 mm wide inside the mat, so 45 high.
        assert!((g.image.w - 90.0).abs() < 1e-3 && (g.image.h - 45.0).abs() < 1e-3);
        assert!((g.outer.w - 100.0).abs() < 1e-3 && (g.outer.h - 55.0).abs() < 1e-3);
        assert_eq!(g.image, g.inner);
        // Centred in the box.
        assert!((g.outer.y + g.outer.h / 2.0 - 60.0).abs() < 1e-3);
    }

    #[test]
    fn a_filled_photo_covers_its_frame_and_the_focus_picks_what_shows() {
        let frame = FrameStyle { fit: Fit::Fill, focus: (0.0, 0.5), ..Default::default() };
        // A wide photo in a square frame: cropped at the sides.
        let g = photo_geometry(area(), &frame, (400, 200), 0.0);
        assert!((g.image.h - 100.0).abs() < 1e-3 && (g.image.w - 200.0).abs() < 1e-3);
        // Focus at the left edge: the picture starts at the frame's left.
        assert!((g.image.x - 10.0).abs() < 1e-3);
        let right = photo_geometry(area(), &FrameStyle { focus: (1.0, 0.5), ..frame }, (400, 200), 0.0);
        assert!((right.image.x + right.image.w - 110.0).abs() < 1e-3);
        // Always at least covers the inner area.
        assert!(right.image.x <= right.inner.x + 1e-3 && right.image.x + right.image.w >= right.inner.x + right.inner.w - 1e-3);
    }

    #[test]
    fn corners_follow_the_mat_and_a_caption_takes_room() {
        let frame = FrameStyle { radius_mm: 8.0, mat_mm: 3.0, ..Default::default() };
        let g = photo_geometry(area(), &frame, (100, 100), 10.0);
        assert_eq!((g.outer_radius, g.inner_radius), (8.0, 5.0));
        assert!(g.outer.y + g.outer.h <= 100.0 + 1e-3, "room kept for the caption");
        // Dark paper wants light ink, light paper dark.
        assert_eq!(ink_on([0, 0, 0]), [235, 235, 235]);
        assert_eq!(ink_on([250, 250, 250]), [30, 30, 30]);
    }
}
