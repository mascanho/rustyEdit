//! Keeping several zines, saving a zine's design as a template to start new
//! ones from, copying one page's look to others, and grouping photos by
//! date, place or colour to lay them out.

use serde::{Deserialize, Serialize};

use crate::zine::{
    FrameStyle, ManualPage, TextAlign, Zine, ZineFont, ZineLayout, ZineMode, ZineOptions,
    ZinePhoto, SHEET_PANELS,
};

// --- Several zines -----------------------------------------------------------------

/// Every zine the user is making. The `active` one is the one being worked
/// on; the program keeps its own copy of that and writes it back with
/// [`ZineLibrary::sync`] before saving or switching.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ZineLibrary {
    pub zines: Vec<Zine>,
    pub active: usize,
}

impl Default for ZineLibrary {
    fn default() -> Self {
        Self {
            zines: vec![Zine::default()],
            active: 0,
        }
    }
}

impl ZineLibrary {
    /// The library with at least one zine and a valid active one.
    pub fn normalised(mut self) -> Self {
        if self.zines.is_empty() {
            self.zines.push(Zine::default());
        }
        self.active = self.active.min(self.zines.len() - 1);
        self
    }

    /// A library of just `zine` (what an older, single-zine save becomes).
    pub fn of(zine: Zine) -> Self {
        Self {
            zines: vec![zine],
            active: 0,
        }
    }

    /// Writes the zine being worked on back into the library.
    pub fn sync(&mut self, current: &Zine) {
        if let Some(slot) = self.zines.get_mut(self.active) {
            *slot = current.clone();
        }
    }

    /// Makes `to` the active zine and returns it.
    pub fn switch(&mut self, to: usize) -> Option<Zine> {
        let zine = self.zines.get(to)?.clone();
        self.active = to;
        Some(zine)
    }

    /// Adds a zine and makes it the active one. Returns its place.
    pub fn add(&mut self, zine: Zine) -> usize {
        self.zines.push(zine);
        self.active = self.zines.len() - 1;
        self.active
    }

    /// Takes a zine away (there is always one left). The active one moves
    /// to a neighbour when it was the one removed. Returns the zine to work
    /// on afterwards when the active one changed.
    pub fn remove(&mut self, ix: usize) -> Option<Zine> {
        if self.zines.len() <= 1 || ix >= self.zines.len() {
            return None;
        }
        self.zines.remove(ix);
        let was_active = ix == self.active;
        if ix < self.active {
            self.active -= 1;
        }
        self.active = self.active.min(self.zines.len() - 1);
        was_active.then(|| self.zines[self.active].clone())
    }
}

/// A name for a zine that is not among `taken`: `base`, or `base 2`...
pub fn unused_title(base: &str, taken: &[&str]) -> String {
    if !taken.contains(&base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|name| !taken.iter().any(|t| t == name))
        .expect("a free name")
}

// --- Templates ------------------------------------------------------------------------

/// A zine's design with the photos taken out: how it is set up, and where
/// photos and text go. Saved to start new zines from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ZineDesign {
    pub name: String,
    pub options: ZineOptions,
    pub pages: Vec<ManualPage>,
}

impl Default for ZineDesign {
    fn default() -> Self {
        Self {
            name: String::new(),
            options: ZineOptions::default(),
            pages: Vec::new(),
        }
    }
}

/// The saved designs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ZineDesigns {
    pub designs: Vec<ZineDesign>,
}

impl ZineDesign {
    /// The design of `zine`, called `name`. Its title, author and where it
    /// was saved belong to that zine, not to the design.
    pub fn of(zine: &Zine, name: &str) -> Self {
        let mut options = zine.options.clone();
        options.title = "Untitled zine".to_string();
        options.author.clear();
        options.destination = None;
        let pages = zine
            .manual
            .pages
            .iter()
            .map(|page| {
                let mut page = page.clone();
                page.slots.iter_mut().for_each(|s| *s = None);
                page.items.iter_mut().for_each(|i| i.photo = None);
                page
            })
            .collect();
        Self {
            name: name.trim().to_string(),
            options,
            pages,
        }
    }

    /// A short line about what the design holds.
    pub fn summary(&self) -> String {
        let frames: usize = self
            .pages
            .iter()
            .map(|p| {
                p.slots.len()
                    + p.items.iter().filter(|i| !i.is_text()).count()
            })
            .sum();
        if self.options.mode == ZineMode::Manual && !self.pages.is_empty() {
            format!(
                "{} page{}, {frames} photo place{}",
                self.pages.len(),
                if self.pages.len() == 1 { "" } else { "s" },
                if frames == 1 { "" } else { "s" }
            )
        } else {
            format!("{}, {}", self.options.layout.label(), self.options.page_size.label())
        }
    }
}

impl Zine {
    /// A new, empty zine set up as `design` is.
    pub fn from_design(design: &ZineDesign) -> Zine {
        let mut zine = Zine {
            options: design.options.clone(),
            ..Default::default()
        };
        zine.manual.pages = design.pages.clone();
        zine.manual.started = zine.options.mode == ZineMode::Manual;
        zine
    }

    /// Sets this zine up as `design` is, keeping its photos, title and
    /// author, and puts the photos into the design's frames in order.
    /// Returns how many frames were filled.
    pub fn apply_design(&mut self, design: &ZineDesign) -> usize {
        let (title, author, destination) = (
            self.options.title.clone(),
            self.options.author.clone(),
            self.options.destination.clone(),
        );
        self.options = design.options.clone();
        self.options.title = title;
        self.options.author = author;
        self.options.destination = destination;
        self.manual.pages = design.pages.clone();
        self.manual.cover = None;
        self.manual.started = self.options.mode == ZineMode::Manual;
        self.fill_frames_in_order()
    }

    /// Puts the zine's photos, in order, into the empty frames of a manual
    /// layout: the cover first, then each page's slots and frames. Returns
    /// how many went in.
    pub fn fill_frames_in_order(&mut self) -> usize {
        if self.options.mode != ZineMode::Manual {
            return 0;
        }
        let photos = self.photos.clone();
        let mut next = photos.iter();
        let mut filled = 0;
        if self.options.cover {
            if let Some(photo) = next.next() {
                self.manual.cover = Some(photo.clone());
                filled += 1;
            }
        }
        let cover = self.options.cover;
        for (n, page) in self.manual.pages.iter_mut().enumerate() {
            let sheet = page.is_sheet();
            for (slot_ix, slot) in page.slots.iter_mut().enumerate() {
                // A folded sheet's cover and back panels are the design's.
                if sheet && cover && (slot_ix == 0 || slot_ix == SHEET_PANELS - 1) {
                    continue;
                }
                if slot.is_none() {
                    if let Some(photo) = next.next() {
                        *slot = Some(photo.clone());
                        filled += 1;
                    }
                }
            }
            for item in page.items.iter_mut().filter(|i| !i.is_text()) {
                if item.photo.is_none() {
                    if let Some(photo) = next.next() {
                        item.photo = Some(photo.clone());
                        item.frame.focus = (0.5, 0.5);
                        filled += 1;
                    }
                }
            }
            let _ = n;
        }
        filled
    }
}

// --- Copying a page's look ----------------------------------------------------------------

/// How text looks, without its size or words.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextLook {
    pub font: ZineFont,
    pub bold: bool,
    pub colour: Option<[u8; 3]>,
    pub align: TextAlign,
    pub backdrop: Option<[u8; 3]>,
}

/// A page's look: its paper colour, how its photos are framed, how its text
/// is set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageStyle {
    pub background: Option<[u8; 3]>,
    pub frame: Option<FrameStyle>,
    pub text: Option<TextLook>,
}

/// Which parts of a look to copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyleParts {
    pub colour: bool,
    pub frames: bool,
    pub text: bool,
}

impl Default for StyleParts {
    fn default() -> Self {
        Self {
            colour: true,
            frames: true,
            text: false,
        }
    }
}

impl Zine {
    /// The look of page `page`: its paper, the frame of its first photo,
    /// the setting of its first text.
    pub fn page_style(&self, page: usize) -> Option<PageStyle> {
        let manual = self.manual.pages.get(page)?;
        let frame = manual
            .items
            .iter()
            .find(|i| !i.is_text())
            .map(|i| i.frame);
        let text = manual.items.iter().find_map(|i| i.text.as_ref()).map(|t| TextLook {
            font: t.font,
            bold: t.bold,
            colour: t.colour,
            align: t.align,
            backdrop: t.backdrop,
        });
        Some(PageStyle {
            background: manual.background,
            frame,
            text,
        })
    }

    /// Gives `pages` the look `style` describes, the parts `parts` names.
    /// Returns how many pages changed.
    pub fn apply_page_style(&mut self, pages: &[usize], style: &PageStyle, parts: StyleParts) -> usize {
        let mut changed = 0;
        for &ix in pages {
            let Some(page) = self.manual.pages.get_mut(ix) else {
                continue;
            };
            let before = page.clone();
            if parts.colour {
                page.background = style.background;
            }
            for item in &mut page.items {
                if parts.frames && !item.is_text() {
                    if let Some(frame) = style.frame {
                        // Where the picture sits in its frame stays.
                        item.frame = FrameStyle {
                            focus: item.frame.focus,
                            ..frame
                        };
                    }
                }
                if parts.text {
                    if let (Some(text), Some(look)) = (item.text.as_mut(), style.text) {
                        text.font = look.font;
                        text.bold = look.bold;
                        text.colour = look.colour;
                        text.align = look.align;
                        text.backdrop = look.backdrop;
                    }
                }
            }
            if *page != before {
                changed += 1;
            }
        }
        changed
    }
}

// --- Grouping photos -----------------------------------------------------------------------

/// What photos can be grouped by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZineGrouping {
    Day,
    Month,
    Place,
    Colour,
    Folder,
}

impl ZineGrouping {
    pub const ALL: [ZineGrouping; 5] = [
        ZineGrouping::Day,
        ZineGrouping::Month,
        ZineGrouping::Place,
        ZineGrouping::Colour,
        ZineGrouping::Folder,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ZineGrouping::Day => "Day",
            ZineGrouping::Month => "Month",
            ZineGrouping::Place => "Place",
            ZineGrouping::Colour => "Colour",
            ZineGrouping::Folder => "Folder",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ZineGrouping::Day => "One section for each day the photos were taken",
            ZineGrouping::Month => "One section for each month",
            ZineGrouping::Place => "Photos taken near each other (from their GPS position)",
            ZineGrouping::Colour => "Reds together, then oranges, greens, blues and so on",
            ZineGrouping::Folder => "One section for each folder the photos are in",
        }
    }

    /// Whether grouping needs something read from the files.
    pub fn needs_files(&self) -> bool {
        matches!(self, ZineGrouping::Place | ZineGrouping::Colour)
    }
}

/// What is known about a photo for grouping it.
#[derive(Debug, Clone)]
pub struct GroupInput {
    pub photo: ZinePhoto,
    /// When it was taken, seconds since 1970 (0 when unknown).
    pub captured: i64,
    /// Where, as degrees north and east.
    pub location: Option<(f64, f64)>,
    /// Its main colour: hue in degrees, saturation and brightness 0..=1.
    pub colour: Option<(f32, f32, f32)>,
}

/// A run of photos with a name for it.
#[derive(Debug, Clone, PartialEq)]
pub struct PhotoGroup {
    pub label: String,
    pub photos: Vec<ZinePhoto>,
}

const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September",
    "October", "November", "December",
];

/// Year, month (1..=12) and day of an instant, in UTC.
pub fn civil_date(seconds: i64) -> (i32, u32, u32) {
    // Howard Hinnant's days-to-civil.
    let z = seconds.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((year + i64::from(month <= 2)) as i32, month, day)
}

/// The colour family of a photo, for colour sections, in the order they
/// run.
fn colour_family(hue: f32, saturation: f32, brightness: f32) -> (usize, &'static str) {
    if brightness < 0.22 {
        return (7, "Dark");
    }
    if saturation < 0.14 {
        return (6, "Black and white");
    }
    match hue.rem_euclid(360.0) {
        h if !(20.0..330.0).contains(&h) => (0, "Reds"),
        h if h < 50.0 => (1, "Oranges and golds"),
        h if h < 75.0 => (2, "Yellows"),
        h if h < 165.0 => (3, "Greens"),
        h if h < 255.0 => (4, "Blues"),
        _ => (5, "Purples and pinks"),
    }
}

/// How a position reads: degrees and hemisphere.
fn place_name((lat, lon): (f64, f64)) -> String {
    format!(
        "{:.2}\u{00B0}{} {:.2}\u{00B0}{}",
        lat.abs(),
        if lat >= 0.0 { "N" } else { "S" },
        lon.abs(),
        if lon >= 0.0 { "E" } else { "W" }
    )
}

/// Groups `inputs` as `by` says. Groups come in a sensible order (dates in
/// time, colours around the wheel); a photo with nothing to group by goes in
/// a last group. Photos inside a group are in the order they were taken.
pub fn group_photos(mut inputs: Vec<GroupInput>, by: ZineGrouping) -> Vec<PhotoGroup> {
    // Taken order first, then groups keep it inside.
    inputs.sort_by_key(|i| (i.captured <= 0, i.captured));
    // (sort key, label, photo)
    let keyed: Vec<((i64, i64), String, ZinePhoto)> = inputs
        .into_iter()
        .map(|i| {
            let unknown = (i64::MAX, 0);
            let (key, label) = match by {
                ZineGrouping::Day | ZineGrouping::Month => {
                    if i.captured <= 0 {
                        (unknown, "Undated".to_string())
                    } else {
                        let (y, m, d) = civil_date(i.captured);
                        if by == ZineGrouping::Day {
                            (
                                (i64::from(y) * 10_000 + i64::from(m) * 100 + i64::from(d), 0),
                                format!("{d} {} {y}", MONTHS[(m - 1) as usize]),
                            )
                        } else {
                            (
                                (i64::from(y) * 100 + i64::from(m), 0),
                                format!("{} {y}", MONTHS[(m - 1) as usize]),
                            )
                        }
                    }
                }
                ZineGrouping::Place => match i.location {
                    // About five kilometres a cell.
                    Some((lat, lon)) => {
                        let cell = ((lat / 0.05).round() as i64, (lon / 0.05).round() as i64);
                        (cell, place_name((cell.0 as f64 * 0.05, cell.1 as f64 * 0.05)))
                    }
                    None => (unknown, "No location".to_string()),
                },
                ZineGrouping::Colour => match i.colour {
                    Some((h, s, v)) => {
                        let (rank, name) = colour_family(h, s, v);
                        ((rank as i64, 0), name.to_string())
                    }
                    None => (unknown, "Unknown colour".to_string()),
                },
                ZineGrouping::Folder => {
                    let folder = i
                        .photo
                        .path
                        .parent()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Photos".to_string());
                    // Alphabetical, by the name's bytes.
                    let lower = folder.to_lowercase();
                    let rank = (0..7).fold(0i64, |acc, n| {
                        acc * 256 + i64::from(lower.as_bytes().get(n).copied().unwrap_or(0))
                    });
                    ((rank, 0), folder)
                }
            };
            (key, label, i.photo)
        })
        .collect();

    let mut keys: Vec<(i64, i64)> = keyed.iter().map(|k| k.0).collect();
    keys.sort_unstable();
    keys.dedup();
    // Places are ordered by when the first photo there was taken, so the
    // zine follows the trip: the stable sort above already put photos in
    // taken order, so the first seen is the earliest.
    if by == ZineGrouping::Place {
        let mut seen = Vec::new();
        for (key, _, _) in &keyed {
            if !seen.contains(key) {
                seen.push(*key);
            }
        }
        // "No location" goes last.
        seen.sort_by_key(|k| *k == (i64::MAX, 0));
        keys = seen;
    }
    keys.into_iter()
        .map(|key| {
            let members: Vec<&((i64, i64), String, ZinePhoto)> =
                keyed.iter().filter(|k| k.0 == key).collect();
            PhotoGroup {
                label: members[0].1.clone(),
                photos: members.iter().map(|k| k.2.clone()).collect(),
            }
        })
        .collect()
}

impl Zine {
    /// Lays the photos out section by section: the zine's photos are put in
    /// the order of `groups`, a heading page opens each section (when
    /// `headings`), and each section's photos fill pages of `per_page`
    /// (1, 2 or 4). The pages are made by hand from here, so the zine
    /// becomes a manual one.
    pub fn arrange(&mut self, groups: &[PhotoGroup], per_page: usize, headings: bool) {
        let per_page = match per_page {
            1 => 1,
            2 => 2,
            _ => 4,
        };
        // The photos in section order, then any the groups left out.
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
        self.options.layout = match per_page {
            1 => ZineLayout::OnePerPage,
            2 => ZineLayout::TwoPerPage,
            _ => ZineLayout::FourPerPage,
        };
        let mut manual = crate::zine::ManualLayout {
            started: true,
            ..Default::default()
        };
        // The cover takes the very first photo.
        let mut cover = None;
        if self.options.cover {
            cover = self.photos.first().cloned();
            manual.cover = cover.clone();
        }
        let (w, h) = {
            let (w, h) = self.options.page_size.millimetres();
            if self.options.landscape { (w.max(h), w.min(h)) } else { (w, h) }
        };
        let _ = (w, h);
        for group in groups {
            let photos: Vec<&ZinePhoto> = group
                .photos
                .iter()
                .filter(|p| self.photos.contains(p) && Some(*p) != cover.as_ref())
                .collect();
            if photos.is_empty() {
                continue;
            }
            if headings && groups.len() > 1 {
                manual.pages.push(self.heading_page(&group.label, photos.len()));
            }
            for chunk in photos.chunks(per_page) {
                let mut page = ManualPage::new(per_page);
                for (slot, photo) in page.slots.iter_mut().zip(chunk) {
                    *slot = Some((*photo).clone());
                }
                manual.pages.push(page);
            }
        }
        self.manual = manual;
    }

    /// A free page that is only a title: the name of a section, large and
    /// centred, with how many photos follow.
    fn heading_page(&self, label: &str, count: usize) -> ManualPage {
        use crate::zine::{FreeItem, TextBox};
        let font = self.options.font;
        let mut title = TextBox::new(label, font);
        title.size_pt = 34.0;
        title.bold = true;
        title.align = TextAlign::Centre;
        let mut count_line = TextBox::new(
            &format!("{count} photo{}", if count == 1 { "" } else { "s" }),
            font,
        );
        count_line.size_pt = 13.0;
        count_line.align = TextAlign::Centre;
        let mut page = ManualPage::free();
        page.items.push(FreeItem::text_box(title, (0.1, 0.40, 0.8, 0.12)));
        page.items.push(FreeItem::text_box(count_line, (0.1, 0.54, 0.8, 0.06)));
        page
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn photo(name: &str) -> ZinePhoto {
        ZinePhoto {
            path: PathBuf::from(format!("/shoot/{name}")),
            copy_name: None,
        }
    }

    fn input(name: &str, captured: i64) -> GroupInput {
        GroupInput {
            photo: photo(name),
            captured,
            location: None,
            colour: None,
        }
    }

    /// 12 March 2025, 10:00 UTC.
    const MARCH_12: i64 = 1_741_773_600;
    const DAY: i64 = 86_400;

    fn zine_of(n: usize) -> Zine {
        Zine {
            photos: (0..n).map(|i| photo(&format!("p{i}.jpg"))).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn several_zines_are_kept_switched_and_removed() {
        let mut library = ZineLibrary::default();
        let mut current = Zine::default();
        current.options.title = "First".into();
        library.sync(&current);
        let mut second = Zine::default();
        second.options.title = "Second".into();
        assert_eq!(library.add(second), 1);
        assert_eq!(library.active, 1);
        assert_eq!(library.switch(0).unwrap().options.title, "First");
        assert_eq!(library.active, 0);
        assert!(library.switch(7).is_none());
        // Removing the zine before the active one keeps the active one.
        library.switch(1);
        assert!(library.remove(0).is_none());
        assert_eq!((library.active, library.zines.len()), (0, 1));
        assert_eq!(library.zines[0].options.title, "Second");
        // The last zine cannot go.
        assert!(library.remove(0).is_none() && library.zines.len() == 1);
        // Removing the active one moves to a neighbour.
        library.add(Zine::default());
        let next = library.remove(1).unwrap();
        assert_eq!(next.options.title, "Second");
        assert_eq!(unused_title("Zine", &["Zine", "Zine 2"]), "Zine 3");
        assert_eq!(unused_title("Other", &["Zine"]), "Other");
    }

    #[test]
    fn a_design_keeps_the_layout_and_loses_the_photos_and_title() {
        let mut zine = zine_of(5);
        zine.options.mode = ZineMode::Manual;
        zine.options.title = "Summer".into();
        zine.options.author = "Me".into();
        zine.options.cover = false;
        zine.add_free_page();
        zine.apply_template(0, crate::zine::ZineTemplate::GridQuote);
        zine.add_page(2);
        zine.place(photo("p4.jpg"), crate::zine::Spot::Slot { page: 1, slot: 0 }, None);
        let design = ZineDesign::of(&zine, "  Quote pages ");
        assert_eq!(design.name, "Quote pages");
        assert_eq!(design.options.title, "Untitled zine");
        assert!(design.options.author.is_empty());
        assert!(design.pages[0].items.iter().all(|i| i.photo.is_none()));
        assert!(design.pages[0].items.iter().any(|i| i.is_text()), "text stays");
        assert!(design.pages[1].slots.iter().all(Option::is_none));
        assert_eq!(design.pages[1].slots.len(), 2);
        assert!(design.summary().contains("2 pages"));

        // A new zine from it has the pages and no photos.
        let fresh = Zine::from_design(&design);
        assert!(fresh.photos.is_empty() && fresh.manual.pages.len() == 2);
        // Applying it to a zine with photos fills the frames in order.
        let mut other = zine_of(7);
        other.options.title = "Mine".into();
        let filled = other.apply_design(&design);
        assert_eq!(filled, 6, "4 frames and 2 slots");
        assert_eq!(other.options.title, "Mine");
        let first: Vec<_> = other.manual.pages[0].items.iter().filter_map(|i| i.photo.clone()).collect();
        assert_eq!(first, (0..4).map(|i| photo(&format!("p{i}.jpg"))).collect::<Vec<_>>());
        assert_eq!(other.manual.pages[1].slots, vec![Some(photo("p4.jpg")), Some(photo("p5.jpg"))]);
        assert_eq!(other.placements(&photo("p6.jpg")), 0, "the seventh has no place");
    }

    #[test]
    fn a_page_look_is_copied_to_the_chosen_pages() {
        let mut zine = zine_of(4);
        zine.options.mode = ZineMode::Manual;
        zine.add_free_page();
        zine.add_free_page();
        zine.add_page(1);
        zine.free_place(0, photo("p0.jpg"), (0.1, 0.1, 0.4, 0.3));
        zine.free_edit(0, 0, |i| {
            i.frame.mat_mm = 4.0;
            i.frame.shadow = true;
            i.frame.radius_mm = 3.0;
        });
        zine.set_page_background(0, Some([10, 20, 30]));
        zine.free_place(1, photo("p1.jpg"), (0.2, 0.2, 0.4, 0.3));
        zine.free_edit(1, 0, |i| i.frame.focus = (0.2, 0.8));
        let style = zine.page_style(0).unwrap();
        assert_eq!(style.background, Some([10, 20, 30]));
        assert_eq!(style.frame.unwrap().mat_mm, 4.0);

        let changed = zine.apply_page_style(&[1, 2], &style, StyleParts::default());
        assert_eq!(changed, 2);
        let frame = zine.manual.pages[1].items[0].frame;
        assert_eq!((frame.mat_mm, frame.shadow, frame.radius_mm), (4.0, true, 3.0));
        assert_eq!(frame.focus, (0.2, 0.8), "the crop is the photo's own");
        assert_eq!(zine.manual.pages[1].background, Some([10, 20, 30]));
        // Only the colour: frames stay as they were.
        let mut plain = zine.clone();
        plain.manual.pages[1].items[0].frame = FrameStyle::default();
        plain.apply_page_style(&[1], &style, StyleParts { colour: true, frames: false, text: false });
        assert_eq!(plain.manual.pages[1].items[0].frame, FrameStyle::default());
        // Applying it again changes nothing.
        assert_eq!(zine.apply_page_style(&[1, 2], &style, StyleParts::default()), 0);
    }

    #[test]
    fn dates_become_days_and_months_in_time_order() {
        let inputs = vec![
            input("c.jpg", MARCH_12 + 2 * DAY),
            input("a.jpg", MARCH_12),
            input("b.jpg", MARCH_12 + 3600),
            input("none.jpg", 0),
        ];
        let days = group_photos(inputs.clone(), ZineGrouping::Day);
        assert_eq!(days.len(), 3);
        assert_eq!(days[0].label, "12 March 2025");
        assert_eq!(days[0].photos, vec![photo("a.jpg"), photo("b.jpg")]);
        assert_eq!(days[1].label, "14 March 2025");
        assert_eq!(days[2].label, "Undated");
        let months = group_photos(inputs, ZineGrouping::Month);
        assert_eq!(months.len(), 2);
        assert_eq!(months[0].label, "March 2025");
        assert_eq!(months[0].photos.len(), 3);
        assert_eq!(civil_date(MARCH_12), (2025, 3, 12));
    }

    #[test]
    fn places_cluster_by_position_in_the_order_they_were_visited() {
        let at = |name: &str, t: i64, loc: Option<(f64, f64)>| GroupInput {
            location: loc,
            ..input(name, t)
        };
        let groups = group_photos(
            vec![
                at("lisbon1.jpg", 100, Some((38.7223, -9.1393))),
                at("porto.jpg", 200, Some((41.1579, -8.6291))),
                at("lisbon2.jpg", 300, Some((38.7230, -9.1400))),
                at("nowhere.jpg", 50, None),
            ],
            ZineGrouping::Place,
        );
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].photos, vec![photo("lisbon1.jpg"), photo("lisbon2.jpg")]);
        assert!(groups[0].label.contains("N") && groups[0].label.contains("W"));
        assert_eq!(groups[1].photos, vec![photo("porto.jpg")]);
        assert_eq!(groups[2].label, "No location");
    }

    #[test]
    fn colours_run_around_the_wheel_with_neutrals_and_darks_last() {
        let coloured = |name: &str, c: Option<(f32, f32, f32)>| GroupInput { colour: c, ..input(name, 1) };
        let groups = group_photos(
            vec![
                coloured("sea.jpg", Some((210.0, 0.7, 0.6))),
                coloured("night.jpg", Some((30.0, 0.5, 0.1))),
                coloured("rose.jpg", Some((355.0, 0.8, 0.7))),
                coloured("grey.jpg", Some((0.0, 0.03, 0.5))),
                coloured("leaf.jpg", Some((110.0, 0.6, 0.5))),
                coloured("sun.jpg", Some((30.0, 0.9, 0.9))),
            ],
            ZineGrouping::Colour,
        );
        let labels: Vec<&str> = groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, ["Reds", "Oranges and golds", "Greens", "Blues", "Black and white", "Dark"]);
    }

    #[test]
    fn folders_group_alphabetically() {
        let in_folder = |folder: &str, name: &str| GroupInput {
            photo: ZinePhoto {
                path: PathBuf::from(format!("/photos/{folder}/{name}")),
                copy_name: None,
            },
            ..input(name, 1)
        };
        let groups = group_photos(
            vec![in_folder("Zurich", "a.jpg"), in_folder("Amsterdam", "b.jpg"), in_folder("Zurich", "c.jpg")],
            ZineGrouping::Folder,
        );
        assert_eq!(groups[0].label, "Amsterdam");
        assert_eq!(groups[1].photos.len(), 2);
    }

    #[test]
    fn arranging_lays_sections_out_with_headings_and_a_cover() {
        let mut zine = zine_of(7);
        zine.options.cover = true;
        let groups = vec![
            PhotoGroup { label: "Monday".into(), photos: vec![photo("p3.jpg"), photo("p1.jpg"), photo("p0.jpg")] },
            PhotoGroup { label: "Tuesday".into(), photos: vec![photo("p6.jpg"), photo("p2.jpg"), photo("p5.jpg"), photo("p4.jpg")] },
        ];
        zine.arrange(&groups, 2, true);
        assert_eq!(zine.options.mode, ZineMode::Manual);
        assert_eq!(zine.options.layout, ZineLayout::TwoPerPage);
        // The photos follow the sections.
        assert_eq!(zine.photos[0], photo("p3.jpg"));
        assert_eq!(zine.photos[3], photo("p6.jpg"));
        // The first photo is the cover and is not repeated.
        assert_eq!(zine.manual.cover, Some(photo("p3.jpg")));
        let pages = &zine.manual.pages;
        // Monday: heading, then p1+p0. Tuesday: heading, then two pages of two.
        assert_eq!(pages.len(), 1 + 1 + 1 + 2);
        assert!(pages[0].is_free() && pages[0].items[0].text.as_ref().unwrap().text == "Monday");
        assert_eq!(pages[1].slots, vec![Some(photo("p1.jpg")), Some(photo("p0.jpg"))]);
        assert!(pages[2].items[0].text.as_ref().unwrap().text == "Tuesday");
        assert_eq!(pages[4].slots, vec![Some(photo("p5.jpg")), Some(photo("p4.jpg"))]);
        assert_eq!(zine.placements(&photo("p3.jpg")), 1);
        // The layout it makes prints every photo once.
        let layout = zine.layout();
        assert_eq!(layout.pages.len(), 1 + pages.len());
        // A single group has no heading, and a folded sheet becomes pages.
        let mut one = zine_of(3);
        one.options.layout = ZineLayout::FoldedSheet;
        one.options.cover = false;
        one.arrange(&[PhotoGroup { label: "All".into(), photos: one.photos.clone() }], 1, true);
        assert_eq!(one.manual.pages.len(), 3);
        assert_eq!(one.options.layout, ZineLayout::OnePerPage);
    }
}
