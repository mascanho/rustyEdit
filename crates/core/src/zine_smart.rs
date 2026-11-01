//! Making good pages faster: choosing the best photos, shaping pages to the
//! mix of portrait and landscape photos, and splitting a story across the
//! pages as captions.

use std::collections::HashMap;

use crate::zine::{
    FrameStyle, FreeItem, ManualLayout, ManualPage, TextAlign, TextBox, Zine, ZineLayout,
    ZineMode, ZineTemplate, ZinePhoto,
};
use crate::zine_library::PhotoGroup;

// --- Which photo is best ---------------------------------------------------------------

/// How good a photo looks on the facts the library keeps: its rating, a
/// pick flag and how sharp it is. Higher is better; `None` for one that
/// has been rejected.
pub fn photo_score(rating: u8, picked: bool, rejected: bool, sharpness: Option<f32>) -> Option<f32> {
    if rejected {
        return None;
    }
    let sharp = sharpness.unwrap_or(40.0).clamp(0.0, 100.0);
    Some(f32::from(rating.min(5)) * 20.0 + if picked { 15.0 } else { 0.0 } + sharp * 0.5)
}

/// What the layout needs to know about a photo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhotoFacts {
    pub score: f32,
    /// Width over height.
    pub aspect: f32,
}

impl PhotoFacts {
    fn orientation(&self) -> Orientation {
        if self.aspect <= 0.87 {
            Orientation::Portrait
        } else {
            Orientation::Landscape
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Orientation {
    Portrait,
    Landscape,
}

// --- Shaping pages ------------------------------------------------------------------------

/// Page boxes as fractions: the margins, and the gap between photos.
struct Frame {
    mx: f32,
    my: f32,
    gap: f32,
    /// A page wider than tall.
    wide: bool,
}

impl Frame {
    fn width(&self) -> f32 {
        1.0 - 2.0 * self.mx
    }

    fn height(&self) -> f32 {
        1.0 - 2.0 * self.my
    }
}

fn contain_frame(photo: &ZinePhoto, rect: (f32, f32, f32, f32)) -> FreeItem {
    FreeItem::photo_frame(Some(photo.clone()), rect, FrameStyle::default())
}

/// Pages for `photos` (in order) that suit their shapes: portraits go side
/// by side, landscapes stack, a lone photo has a page to itself.
fn shape_pages(photos: &[(ZinePhoto, PhotoFacts)], f: &Frame) -> Vec<ManualPage> {
    let mut pages = Vec::new();
    let (w, h) = (f.width(), f.height());
    let mut i = 0;
    while i < photos.len() {
        let left = photos.len() - i;
        let portraits = photos[i..]
            .iter()
            .take_while(|(_, facts)| facts.orientation() == Orientation::Portrait)
            .count();
        let mut page = ManualPage::free();
        if portraits >= 3 {
            // Three tall photos in a row.
            let each = (w - 2.0 * f.gap) / 3.0;
            let (height, top) = (h.min(0.7), 0.5 - h.min(0.7) / 2.0);
            for k in 0..3 {
                page.items.push(contain_frame(
                    &photos[i + k].0,
                    (f.mx + k as f32 * (each + f.gap), top, each, height),
                ));
            }
            i += 3;
        } else if portraits == 2 {
            let each = (w - f.gap) / 2.0;
            let (height, top) = (h.min(0.72), 0.5 - h.min(0.72) / 2.0);
            for k in 0..2 {
                page.items.push(contain_frame(
                    &photos[i + k].0,
                    (f.mx + k as f32 * (each + f.gap), top, each, height),
                ));
            }
            i += 2;
        } else if left >= 2 && photos[i].1.orientation() == Orientation::Landscape
            && photos[i + 1].1.orientation() == Orientation::Landscape
        {
            if f.wide {
                // Side by side on a wide page.
                let each = (w - f.gap) / 2.0;
                for k in 0..2 {
                    page.items.push(contain_frame(
                        &photos[i + k].0,
                        (f.mx + k as f32 * (each + f.gap), f.my, each, h),
                    ));
                }
            } else {
                // Stacked on a tall one.
                let each = (h - f.gap) / 2.0;
                for k in 0..2 {
                    page.items.push(contain_frame(
                        &photos[i + k].0,
                        (f.mx, f.my + k as f32 * (each + f.gap), w, each),
                    ));
                }
            }
            i += 2;
        } else if left >= 2 {
            // A portrait and a landscape: the wide one gets the width, the
            // tall one the height.
            let (a, b) = (&photos[i], &photos[i + 1]);
            let (landscape, portrait) = if a.1.orientation() == Orientation::Landscape { (a, b) } else { (b, a) };
            if f.wide {
                let (lw, pw) = (w * 0.58, w * 0.42 - f.gap);
                page.items.push(contain_frame(&landscape.0, (f.mx, f.my, lw, h)));
                page.items.push(contain_frame(&portrait.0, (f.mx + lw + f.gap, f.my, pw, h)));
            } else {
                let top = h * 0.42;
                page.items.push(contain_frame(&landscape.0, (f.mx, f.my, w, top)));
                page.items.push(contain_frame(
                    &portrait.0,
                    (f.mx + w * 0.2, f.my + top + f.gap, w * 0.6, h - top - f.gap),
                ));
            }
            i += 2;
        } else {
            // The last photo, alone.
            let facts = photos[i].1;
            let fill = FrameStyle {
                fit: crate::zine::Fit::Contain,
                ..Default::default()
            };
            let mut item = FreeItem::photo_frame(Some(photos[i].0.clone()), (f.mx, f.my, w, h), fill);
            item.frame.focus = (0.5, 0.5);
            let _ = facts;
            page.items.push(item);
            i += 1;
        }
        pages.push(page);
    }
    pages
}

impl Zine {
    /// Like [`Zine::arrange`], but cleverer: the cover is the best photo of
    /// all, each section opens with its best photo large with the section's
    /// name under it, and the rest of the section's pages are shaped to
    /// whether their photos are tall or wide. `facts` has each photo's
    /// score and shape.
    pub fn arrange_smart(&mut self, groups: &[PhotoGroup], facts: &HashMap<ZinePhoto, PhotoFacts>) {
        let fact = |p: &ZinePhoto| {
            facts.get(p).copied().unwrap_or(PhotoFacts {
                score: 0.0,
                aspect: 1.5,
            })
        };
        // The photos in section order, then any left out.
        let mut ordered: Vec<ZinePhoto> = Vec::new();
        for group in groups {
            for photo in &group.photos {
                if self.contains(photo) && !ordered.contains(photo) {
                    ordered.push(photo.clone());
                }
            }
        }
        for photo in &self.photos {
            if !ordered.contains(photo) {
                ordered.push(photo.clone());
            }
        }
        self.photos = ordered;
        self.options.mode = ZineMode::Manual;
        self.options.layout = ZineLayout::OnePerPage;

        let mut manual = ManualLayout {
            started: true,
            ..Default::default()
        };
        // The cover takes the best photo of all.
        let cover = if self.options.cover {
            self.photos
                .iter()
                .max_by(|a, b| fact(a).score.total_cmp(&fact(b).score))
                .cloned()
        } else {
            None
        };
        manual.cover = cover.clone();

        let (pw, ph) = {
            let (w, h) = self.options.page_size.millimetres();
            if self.options.landscape { (w.max(h), w.min(h)) } else { (w, h) }
        };
        let m = self.options.margin_mm as f32;
        let frame = Frame {
            mx: (m / pw).clamp(0.0, 0.15),
            my: (m / ph).clamp(0.0, 0.15),
            gap: 0.025,
            wide: pw > ph,
        };
        for group in groups {
            let mut members: Vec<ZinePhoto> = group
                .photos
                .iter()
                .filter(|p| self.contains(p) && Some(*p) != cover.as_ref())
                .cloned()
                .collect();
            if members.is_empty() {
                continue;
            }
            // The best photo opens the section.
            let best = members
                .iter()
                .enumerate()
                .max_by(|a, b| fact(a.1).score.total_cmp(&fact(b.1).score))
                .map(|(ix, _)| ix)
                .unwrap_or(0);
            let hero = members.remove(best);
            manual.pages.push(self.hero_page(&hero, &group.label, members.len() + 1));
            let rest: Vec<(ZinePhoto, PhotoFacts)> = members.into_iter().map(|p| {
                let f = fact(&p);
                (p, f)
            }).collect();
            manual.pages.extend(shape_pages(&rest, &frame));
        }
        self.manual = manual;
    }

    /// A page for a section's best photo, large, with the section's name
    /// and how many photos it holds.
    fn hero_page(&self, hero: &ZinePhoto, label: &str, count: usize) -> ManualPage {
        let m = self.options.margin_mm as f32;
        let (pw, ph) = self.options.page_size.millimetres();
        let (pw, ph) = if self.options.landscape { (pw.max(ph), pw.min(ph)) } else { (pw, ph) };
        let mut items = ZineTemplate::HeroCaption.items((m / pw, m / ph), self.options.font);
        if let Some(frame) = items.iter_mut().find(|i| !i.is_text()) {
            frame.photo = Some(hero.clone());
        }
        let mut texts = items.iter_mut().filter_map(|i| i.text.as_mut());
        if let Some(title) = texts.next() {
            title.text = label.to_string();
        }
        if let Some(body) = texts.next() {
            body.text = format!("{count} photo{}", if count == 1 { "" } else { "s" });
            body.size_pt = 12.0;
        }
        ManualPage {
            items,
            ..ManualPage::free()
        }
    }
}

// --- Splitting a story --------------------------------------------------------------------

/// `text` as up to `parts` runs of whole paragraphs or sentences, in order,
/// about as long as each other.
pub fn split_story(text: &str, parts: usize) -> Vec<String> {
    let parts = parts.max(1);
    let paragraphs: Vec<String> = text
        .split("\n\n")
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|p| !p.is_empty())
        .collect();
    // Paragraphs when there are enough to go round; otherwise sentences.
    let pieces: Vec<String> = if paragraphs.len() >= parts {
        paragraphs
    } else {
        paragraphs.iter().flat_map(|p| sentences(p)).collect()
    };
    if pieces.is_empty() {
        return Vec::new();
    }
    let parts = parts.min(pieces.len());
    let total: usize = pieces.iter().map(|p| p.len()).sum();
    let mut runs: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut used = 0usize;
    for (n, piece) in pieces.iter().enumerate() {
        let runs_left = parts - runs.len();
        let pieces_left = pieces.len() - n;
        // Close the run when it has its share, or when the rest are needed
        // one to a run.
        let target = (total - (total - used - current.len()).min(total)) as f32;
        let _ = target;
        let share = total as f32 * (runs.len() + 1) as f32 / parts as f32;
        if !current.is_empty()
            && runs.len() + 1 < parts
            && ((used + current.len()) as f32 >= share || pieces_left < runs_left)
        {
            used += current.len();
            runs.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(piece);
    }
    if !current.is_empty() {
        runs.push(current);
    }
    runs
}

/// Sentences of a paragraph: split after . ! or ? (and a closing quote)
/// followed by a space.
fn sentences(paragraph: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = paragraph.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        current.push(c);
        let ends = matches!(c, '.' | '!' | '?' | '\u{2026}')
            || (matches!(c, '"' | '\u{201D}' | '\'' | '\u{2019}')
                && i > 0
                && matches!(chars[i - 1], '.' | '!' | '?'));
        let before_space = chars.get(i + 1).is_none_or(|n| n.is_whitespace());
        if ends && before_space {
            // Not after an abbreviation of one capital ("J. Smith").
            let words: Vec<&str> = current.split_whitespace().collect();
            let abbreviation = words.last().is_some_and(|w| {
                let w = w.trim_end_matches('.');
                c == '.' && w.chars().count() == 1 && w.chars().all(|ch| ch.is_uppercase())
            });
            if !abbreviation {
                out.push(current.trim().to_string());
                current.clear();
            }
        }
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

impl Zine {
    /// Takes the story's captions off every page.
    pub fn clear_story_captions(&mut self) {
        for page in &mut self.manual.pages {
            page.items.retain(|i| !i.text.as_ref().is_some_and(|t| t.story));
        }
    }

    /// Splits the zine's story across the pages that have photos on them,
    /// as a caption on each. A grid page becomes a free one, its photos
    /// made room for. A page that already has a story caption gets a new one
    /// in its place. Returns how many pages were captioned.
    pub fn apply_story(&mut self) -> usize {
        if !self.manual.started {
            self.seed_manual();
        }
        self.options.mode = ZineMode::Manual;
        let eligible: Vec<usize> = (0..self.manual.pages.len())
            .filter(|&p| {
                let page = &self.manual.pages[p];
                !page.is_sheet()
                    && (page.slots.iter().any(Option::is_some)
                        || page.items.iter().any(|i| i.photo.is_some()))
            })
            .collect();
        if eligible.is_empty() {
            return 0;
        }
        let chunks = split_story(&self.story, eligible.len());
        // Fewer chunks than pages: spread them out along the zine.
        let pages: Vec<usize> = (0..chunks.len())
            .map(|k| eligible[k * eligible.len() / chunks.len()])
            .collect();
        // First clear the old story captions, so a new split starts clean.
        for page in &mut self.manual.pages {
            page.items.retain(|i| !i.text.as_ref().is_some_and(|t| t.story));
        }
        let (pw, ph) = self.manual_page_size();
        let m = self.options.margin_mm as f32;
        let (mx, my) = ((m / pw).clamp(0.0, 0.15), (m / ph).clamp(0.0, 0.15));
        let font = self.options.font;
        for (page, text) in pages.iter().zip(&chunks) {
            self.make_page_free(*page);
            let manual = &mut self.manual.pages[*page];
            // Make room under the photos for the words.
            let limit = 0.80;
            let bottom = manual
                .items
                .iter()
                .filter(|i| !i.is_text())
                .map(|i| i.y + i.h)
                .fold(0.0f32, f32::max);
            if bottom > limit {
                let k = (limit - my) / (bottom - my).max(0.01);
                for item in manual.items.iter_mut().filter(|i| !i.is_text()) {
                    item.y = my + (item.y - my).max(0.0) * k;
                    item.h *= k;
                }
            }
            let mut caption = TextBox::new(text, font);
            caption.size_pt = 11.0;
            caption.align = TextAlign::Left;
            caption.story = true;
            manual
                .items
                .push(FreeItem::text_box(caption, (mx, 0.82, 1.0 - 2.0 * mx, 0.12)).clamped());
        }
        pages.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn photo(name: &str) -> ZinePhoto {
        ZinePhoto {
            path: PathBuf::from(format!("/s/{name}")),
            copy_name: None,
        }
    }

    fn zine_of(n: usize) -> Zine {
        Zine {
            photos: (0..n).map(|i| photo(&format!("p{i}"))).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn scores_favour_ratings_picks_and_sharpness_and_drop_rejects() {
        let plain = photo_score(0, false, false, Some(50.0)).unwrap();
        assert!(photo_score(5, false, false, Some(50.0)).unwrap() > plain + 90.0);
        assert!(photo_score(0, true, false, Some(50.0)).unwrap() > plain);
        assert!(photo_score(0, false, false, Some(90.0)).unwrap() > plain);
        assert_eq!(photo_score(5, true, true, Some(99.0)), None);
        // Unmeasured sharpness counts as middling, not zero.
        assert!(photo_score(0, false, false, None).unwrap() > 0.0);
    }

    #[test]
    fn a_story_is_split_by_paragraph_or_sentence_into_even_runs() {
        let text = "One.\n\nTwo is longer than one.\n\nThree.";
        assert_eq!(split_story(text, 3), vec!["One.", "Two is longer than one.", "Three."]);
        // Fewer pages than paragraphs: neighbours join.
        let two = split_story(text, 2);
        assert_eq!(two.len(), 2);
        assert_eq!(two.join(" "), "One. Two is longer than one. Three.");
        // One paragraph, split by sentences.
        let one = "We arrived late. The light was low! Was it worth it? Yes. Always.";
        let parts = split_story(one, 3);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts.join(" "), one);
        assert!(parts.iter().all(|p| p.ends_with('.') || p.ends_with('!') || p.ends_with('?')));
        // More pages than sentences: one sentence each, no empty runs.
        assert_eq!(split_story("Only one.", 4), vec!["Only one."]);
        assert!(split_story("   ", 3).is_empty());
        // "J. Smith" is not two sentences.
        assert_eq!(split_story("Thanks to J. Smith for the loan.", 2).len(), 1);
    }

    #[test]
    fn the_story_becomes_a_caption_on_each_photo_page_with_room_made_for_it() {
        let mut zine = zine_of(4);
        zine.options.cover = false;
        zine.options.layout = ZineLayout::OnePerPage;
        zine.seed_manual();
        zine.story = "First page words.\n\nSecond page words.\n\nThird page words.\n\nFourth.".into();
        let done = zine.apply_story();
        assert_eq!(done, 4);
        for (n, page) in zine.manual.pages.iter().enumerate() {
            assert!(page.is_free());
            let caption = page.items.iter().find(|i| i.text.as_ref().is_some_and(|t| t.story)).unwrap();
            assert!(caption.y >= 0.8);
            // No photo reaches into the caption.
            assert!(page.items.iter().filter(|i| !i.is_text()).all(|i| i.y + i.h <= 0.8 + 1e-4), "page {n}");
        }
        assert!(zine.manual.pages[1].items.iter().any(|i| i.text.as_ref().is_some_and(|t| t.text == "Second page words.")));
        // Splitting again replaces the captions instead of piling them up.
        zine.story = "Just one thing to say.".into();
        zine.apply_story();
        let count: usize = zine
            .manual
            .pages
            .iter()
            .map(|p| p.items.iter().filter(|i| i.text.as_ref().is_some_and(|t| t.story)).count())
            .sum();
        assert_eq!(count, 1);
    }

    #[test]
    fn smart_arranging_picks_heroes_and_shapes_pages_to_the_mix() {
        let mut zine = zine_of(9);
        zine.options.cover = true;
        let mut facts = HashMap::new();
        let shape = |i: usize| if i % 3 == 0 { 0.67 } else { 1.5 };
        for i in 0..9 {
            facts.insert(photo(&format!("p{i}")), PhotoFacts { score: i as f32, aspect: shape(i) });
        }
        // p8 is the best of all (the cover); p5 the best of the second group.
        let groups = vec![
            PhotoGroup { label: "Morning".into(), photos: (0..4).map(|i| photo(&format!("p{i}"))).collect() },
            PhotoGroup { label: "Evening".into(), photos: (4..9).map(|i| photo(&format!("p{i}"))).collect() },
        ];
        zine.arrange_smart(&groups, &facts);
        assert_eq!(zine.manual.cover, Some(photo("p8")));
        assert_eq!(zine.options.mode, ZineMode::Manual);
        // Each section opens with its best photo and its name.
        let first = &zine.manual.pages[0];
        assert_eq!(first.items.iter().find(|i| !i.is_text()).unwrap().photo, Some(photo("p3")));
        assert!(first.items.iter().any(|i| i.text.as_ref().is_some_and(|t| t.text == "Morning")));
        // Every photo is on exactly one page (the cover counts).
        for i in 0..9 {
            assert_eq!(zine.placements(&photo(&format!("p{i}"))), 1, "p{i}");
        }
        // Frames stay on the page and never overlap.
        for page in &zine.manual.pages {
            let frames: Vec<_> = page.items.iter().filter(|i| !i.is_text()).collect();
            for (a, fa) in frames.iter().enumerate() {
                assert!(fa.x >= -1e-4 && fa.x + fa.w <= 1.0 + 1e-4 && fa.y >= -1e-4 && fa.y + fa.h <= 1.0 + 1e-4);
                for fb in frames.iter().skip(a + 1) {
                    let apart = fa.x + fa.w <= fb.x + 1e-4 || fb.x + fb.w <= fa.x + 1e-4
                        || fa.y + fa.h <= fb.y + 1e-4 || fb.y + fb.h <= fa.y + 1e-4;
                    assert!(apart, "overlap");
                }
            }
        }
    }

    #[test]
    fn portraits_pair_up_side_by_side_and_landscapes_stack() {
        let f = Frame { mx: 0.06, my: 0.05, gap: 0.025, wide: false };
        let fact = |aspect| PhotoFacts { score: 0.0, aspect };
        let two_portraits = [(photo("a"), fact(0.7)), (photo("b"), fact(0.7))];
        let pages = shape_pages(&two_portraits, &f);
        assert_eq!(pages.len(), 1);
        assert!(pages[0].items[1].x > pages[0].items[0].x && pages[0].items[0].y == pages[0].items[1].y);
        let two_landscapes = [(photo("a"), fact(1.5)), (photo("b"), fact(1.5))];
        let pages = shape_pages(&two_landscapes, &f);
        assert!(pages[0].items[1].y > pages[0].items[0].y && pages[0].items[0].x == pages[0].items[1].x);
        // On a wide page landscapes sit side by side.
        let wide = Frame { wide: true, ..f };
        let pages = shape_pages(&two_landscapes, &wide);
        assert!(pages[0].items[1].x > pages[0].items[0].x);
        // Three portraits make a row; a lone photo has a page.
        let three: Vec<_> = (0..3).map(|i| (photo(&format!("t{i}")), fact(0.7))).collect();
        assert_eq!(shape_pages(&three, &f)[0].items.len(), 3);
        assert_eq!(shape_pages(&[(photo("x"), fact(1.5))], &f)[0].items.len(), 1);
        // A mixed pair shares a page.
        let mixed = [(photo("l"), fact(1.5)), (photo("p"), fact(0.7))];
        assert_eq!(shape_pages(&mixed, &f).len(), 1);
    }
}
