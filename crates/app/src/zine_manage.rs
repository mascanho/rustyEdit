//! Looking after zines: keeping several and switching between them, saving
//! a zine's design as a template to start others from, laying photos out
//! by date, place or colour, and copying a page's look to other pages.

use std::path::PathBuf;

use gpui::{div, prelude::*, AnyElement, ClickEvent, Context, SharedString};
use rustyedit_core::{
    zine_group_photos, zine_unused_title, Zine, ZineDesign, ZineGroupInput, ZineGrouping,
    ZineStyleParts,
};
use rustyedit_ui::{sp, Theme};

use crate::{
    workspace::{TextTarget, Workspace},
    zine_bar::pill,
    zines::plural,
};

/// Which pages a copied look is put on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LookScope {
    /// Every page.
    All,
    /// The pages after the one the look came from.
    After(usize),
    /// The pages picked in the strip below (Cmd- or Shift-click).
    Picked,
}

/// A photo's main colour: hue in degrees, then saturation and brightness
/// 0..=1. The hue is that of the colourful parts, weighted by how strong
/// they are, so a blue sky over grey rock reads as blue.
pub(crate) fn dominant_colour(image: &image::RgbImage) -> Option<(f32, f32, f32)> {
    let step = ((image.width().max(image.height()) / 64).max(1)) as usize;
    let (mut hx, mut hy, mut sat, mut val, mut n) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for y in (0..image.height() as usize).step_by(step) {
        for x in (0..image.width() as usize).step_by(step) {
            let p = image.get_pixel(x as u32, y as u32).0;
            let (r, g, b) = (p[0] as f64 / 255.0, p[1] as f64 / 255.0, p[2] as f64 / 255.0);
            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            let d = max - min;
            let s = if max > 0.0 { d / max } else { 0.0 };
            if d > 0.0 {
                let hue = if max == r {
                    60.0 * ((g - b) / d).rem_euclid(6.0)
                } else if max == g {
                    60.0 * ((b - r) / d + 2.0)
                } else {
                    60.0 * ((r - g) / d + 4.0)
                };
                let weight = s * max;
                hx += hue.to_radians().cos() * weight;
                hy += hue.to_radians().sin() * weight;
            }
            sat += s;
            val += max;
            n += 1.0;
        }
    }
    if n == 0.0 {
        return None;
    }
    let hue = hy.atan2(hx).to_degrees().rem_euclid(360.0);
    Some((hue as f32, (sat / n) as f32, (val / n) as f32))
}

impl Workspace {
    // --- Several zines -------------------------------------------------------------

    /// Makes `zine` the one being worked on: a fresh start for what depends
    /// on the zine (selection, undo, what is open).
    fn load_zine(&mut self, zine: Zine, cx: &mut Context<Self>) {
        self.zine = zine;
        let ui = &mut self.zine_ui;
        ui.clear_selection();
        ui.page = None;
        ui.gesture = None;
        ui.guides.clear();
        ui.marquee = None;
        ui.undo.clear();
        ui.redo.clear();
        ui.preview = None;
        ui.pages_selected.clear();
        ui.confirm_delete = None;
        self.zine_changed(cx);
    }

    pub(crate) fn zine_switch(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.zine_library.sync(&self.zine);
        if ix == self.zine_library.active {
            return;
        }
        if let Some(zine) = self.zine_library.switch(ix) {
            self.load_zine(zine, cx);
        }
    }

    /// A fresh zine.
    pub(crate) fn zine_create(&mut self, cx: &mut Context<Self>) {
        self.zine_add(Zine::default(), cx);
    }

    /// Puts `zine` in the library under a title no other has, and works on it.
    fn zine_add(&mut self, mut zine: Zine, cx: &mut Context<Self>) {
        self.zine_library.sync(&self.zine);
        let titles: Vec<&str> = self
            .zine_library
            .zines
            .iter()
            .map(|z| z.options.title.as_str())
            .collect();
        zine.options.title = zine_unused_title(zine.options.title.trim(), &titles);
        self.zine_library.add(zine.clone());
        self.load_zine(zine, cx);
    }

    /// A copy of the zine being worked on.
    pub(crate) fn zine_duplicate_zine(&mut self, cx: &mut Context<Self>) {
        let mut copy = self.zine.clone();
        copy.options.title = format!("{} copy", copy.options.title.trim());
        self.zine_add(copy, cx);
    }

    /// Deletes a zine; the first click asks, the second does it.
    pub(crate) fn zine_delete(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.zine_library.zines.len() <= 1 {
            return;
        }
        if self.zine_ui.confirm_delete != Some(ix) {
            self.zine_ui.confirm_delete = Some(ix);
            cx.notify();
            return;
        }
        self.zine_library.sync(&self.zine);
        match self.zine_library.remove(ix) {
            Some(next) => self.load_zine(next, cx),
            None => {
                // Another zine went: the one being worked on is the same,
                // though its place may have moved.
                self.zine_ui.confirm_delete = None;
                self.zine_changed(cx);
            }
        }
    }

    // --- Templates ---------------------------------------------------------------------

    /// Saves the design of the zine being worked on.
    pub(crate) fn save_design(&mut self, name: String, cx: &mut Context<Self>) {
        let name = if name.trim().is_empty() {
            self.zine.options.title.clone()
        } else {
            name
        };
        let design = ZineDesign::of(&self.zine, &name);
        // The same name replaces the older design.
        let designs = &mut self.zine_designs.designs;
        match designs.iter_mut().find(|d| d.name == design.name) {
            Some(existing) => *existing = design,
            None => designs.push(design),
        }
        if let Err(error) = rustyedit_io::zine::save_designs(&self.zine_designs) {
            eprintln!("Could not save the zine templates: {error}");
        }
        self.status_message = Some(format!("Saved the template \u{201C}{}\u{201D}", name.trim()).into());
        cx.notify();
    }

    fn delete_design(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix < self.zine_designs.designs.len() {
            self.zine_designs.designs.remove(ix);
            if let Err(error) = rustyedit_io::zine::save_designs(&self.zine_designs) {
                eprintln!("Could not save the zine templates: {error}");
            }
            cx.notify();
        }
    }

    /// A new zine set up as a template is.
    fn zine_from_design(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(design) = self.zine_designs.designs.get(ix).cloned() {
            let mut zine = Zine::from_design(&design);
            zine.options.title = design.name.clone();
            self.zine_add(zine, cx);
        }
    }

    /// The zine being worked on, set up as a template is, its photos in the
    /// template's frames.
    fn use_design_here(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(design) = self.zine_designs.designs.get(ix).cloned() else {
            return;
        };
        let mut filled = 0;
        self.zine_ui.clear_selection();
        self.update_zine(cx, |z| filled = z.apply_design(&design));
        self.status_message = Some(
            format!(
                "Used \u{201C}{}\u{201D}: {filled} photo{} placed",
                design.name,
                plural(filled)
            )
            .into(),
        );
    }

    // --- Arranging by theme ----------------------------------------------------------------

    /// Groups the zine's photos by what the Arrange card says and lays them
    /// out section by section.
    pub(crate) fn arrange_photos(&mut self, cx: &mut Context<Self>) {
        if self.zine_ui.arranging || self.zine.photos.is_empty() {
            return;
        }
        let (by, per_page, headings) = (
            self.zine_ui.arrange_by,
            self.zine_ui.arrange_per_page,
            self.zine_ui.arrange_headings,
        );
        // What is known without opening files.
        let rows: Vec<(rustyedit_core::ZinePhoto, i64, PathBuf, Option<PathBuf>)> = self
            .zine
            .photos
            .iter()
            .map(|photo| {
                let asset = self
                    .catalog
                    .assets
                    .iter()
                    .find(|a| a.path == photo.path && a.copy_name == photo.copy_name);
                (
                    photo.clone(),
                    asset.map_or(0, |a| a.meta.captured),
                    photo.path.clone(),
                    asset.and_then(|a| self.export_thumbnail(a).or_else(|| a.thumbnail_path.clone())),
                )
            })
            .collect();
        self.zine_ui.arranging = true;
        self.status_message = Some("Arranging the photos\u{2026}".into());
        cx.notify();

        let job = cx.background_executor().spawn(async move {
            let inputs: Vec<ZineGroupInput> = rows
                .into_iter()
                .map(|(photo, captured, path, thumb)| ZineGroupInput {
                    photo,
                    captured,
                    location: (by == ZineGrouping::Place)
                        .then(|| rustyedit_io::read_gps(&path))
                        .flatten(),
                    colour: (by == ZineGrouping::Colour)
                        .then(|| {
                            thumb
                                .and_then(|t| image::open(t).ok())
                                .and_then(|i| dominant_colour(&i.to_rgb8()))
                        })
                        .flatten(),
                })
                .collect();
            zine_group_photos(inputs, by)
        });
        cx.spawn(async move |this, cx| {
            let groups = job.await;
            this.update(cx, |this, cx| {
                this.zine_ui.arranging = false;
                this.zine_ui.clear_selection();
                this.zine_ui.page = None;
                let sections = groups.len();
                this.update_zine(cx, |z| z.arrange(&groups, per_page, headings));
                this.status_message = Some(
                    format!(
                        "Arranged {} photos in {sections} section{}",
                        this.zine.photos.len(),
                        plural(sections)
                    )
                    .into(),
                );
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // --- Copying a page's look --------------------------------------------------------------------

    pub(crate) fn zine_copy_look(&mut self, page: usize, cx: &mut Context<Self>) {
        if let Some(style) = self.zine.page_style(page) {
            self.zine_ui.style_clipboard = Some(style);
            self.status_message = Some("Copied the look of the page".into());
            cx.notify();
        }
    }

    /// Puts the copied look on `scope`'s pages.
    pub(crate) fn zine_paste_look(&mut self, scope: LookScope, cx: &mut Context<Self>) {
        let Some(style) = self.zine_ui.style_clipboard else {
            return;
        };
        let count = self.zine.manual.pages.len();
        let pages: Vec<usize> = match scope {
            LookScope::All => (0..count).collect(),
            LookScope::After(page) => (page + 1..count).collect(),
            LookScope::Picked => self.zine_ui.pages_selected.clone(),
        };
        let pages: Vec<usize> = pages
            .into_iter()
            .filter(|&p| self.zine.manual.pages.get(p).is_some_and(|m| !m.is_sheet()))
            .collect();
        let parts = self.zine_ui.style_parts;
        let mut changed = 0;
        self.update_zine(cx, |z| changed = z.apply_page_style(&pages, &style, parts));
        self.status_message = Some(
            if pages.is_empty() {
                "No pages to put the look on".to_string()
            } else {
                format!("Put the look on {changed} page{}", plural(changed))
            }
            .into(),
        );
    }

    // --- The cards in the options panel --------------------------------------------------------------

    /// The list of zines, and what can be done with them.
    pub(crate) fn zines_card(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let active = self.zine_library.active;
        let rows = self.zine_library.zines.iter().enumerate().map(|(ix, zine)| {
            // The one being worked on has its latest in `self.zine`.
            let zine = if ix == active { &self.zine } else { zine };
            let selected = ix == active;
            let count = zine.photos.len();
            let confirming = self.zine_ui.confirm_delete == Some(ix);
            div()
                .id(SharedString::from(format!("zine-row-{ix}")))
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_2()
                .py_1p5()
                .rounded_lg()
                .cursor_pointer()
                .border_1()
                .when(selected, |el| {
                    el.bg(theme.accent_muted).border_color(theme.accent)
                })
                .when(!selected, |el| {
                    el.bg(theme.background)
                        .border_color(theme.border_subtle)
                        .hover(|el| el.border_color(theme.accent))
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w(sp(0.0))
                        .child(
                            div()
                                .truncate()
                                .text_size(sp(12.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.text)
                                .child(zine.options.title.clone()),
                        )
                        .child(
                            div()
                                .text_size(sp(10.0))
                                .text_color(theme.text_faint)
                                .child(format!(
                                    "{count} photo{} \u{00B7} {}",
                                    plural(count),
                                    zine.options.format.label()
                                )),
                        ),
                )
                .when(self.zine_library.zines.len() > 1, |el| {
                    el.child(
                        div()
                            .id(SharedString::from(format!("zine-delete-{ix}")))
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .px_1p5()
                            .h(sp(20.0))
                            .rounded_md()
                            .text_size(sp(10.0))
                            .text_color(if confirming { theme.danger } else { theme.text_faint })
                            .hover(|el| el.bg(theme.surface).text_color(theme.danger))
                            .tooltip(rustyedit_ui::tip("Delete this zine (the photos stay in the library)", &theme))
                            .child(if confirming { "Delete?" } else { "\u{2715}" })
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| this.zine_delete(ix, cx))),
                    )
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| this.zine_switch(ix, cx)))
                .into_any_element()
        });
        let actions = div()
            .flex()
            .flex_row()
            .gap_1()
            .child(
                pill(&theme, "zine-new".into(), "New zine", "Start another zine", false, true)
                    .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.zine_create(cx))),
            )
            .child(
                pill(&theme, "zine-copy".into(), "Duplicate", "A copy of this zine, to try another look", false, true)
                    .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.zine_duplicate_zine(cx))),
            )
            .into_any_element();
        let mut out: Vec<AnyElement> = rows.collect();
        out.push(actions);
        out
    }

    /// Saved templates, and saving this zine's.
    pub(crate) fn designs_card(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let mut out: Vec<AnyElement> = vec![
            div()
                .text_size(sp(10.5))
                .text_color(theme.text_faint)
                .child("A template remembers a zine's setup and where its photos and text go.")
                .into_any_element(),
            self.zine_text_field(
                "zine-design-name",
                TextTarget::ZineDesignName,
                "",
                "Name this zine's template, then Enter",
                cx,
            ),
        ];
        if self.zine_designs.designs.is_empty() {
            out.push(
                div()
                    .text_size(sp(10.5))
                    .text_color(theme.text_faint)
                    .child("No templates yet.")
                    .into_any_element(),
            );
        }
        for (ix, design) in self.zine_designs.designs.iter().enumerate() {
            out.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded_lg()
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(sp(12.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(theme.text)
                                    .child(design.name.clone()),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("zine-design-delete-{ix}")))
                                    .px_1p5()
                                    .text_size(sp(10.0))
                                    .text_color(theme.text_faint)
                                    .cursor_pointer()
                                    .hover(|el| el.text_color(theme.danger))
                                    .tooltip(rustyedit_ui::tip("Delete the template", &theme))
                                    .child("\u{2715}")
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| this.delete_design(ix, cx))),
                            ),
                    )
                    .child(
                        div()
                            .text_size(sp(10.0))
                            .text_color(theme.text_faint)
                            .child(design.summary()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_1()
                            .child(
                                pill(&theme, SharedString::from(format!("zine-design-new-{ix}")), "New zine", "Start a new zine laid out like this", false, true)
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| this.zine_from_design(ix, cx))),
                            )
                            .child(
                                pill(&theme, SharedString::from(format!("zine-design-use-{ix}")), "Use here", "Lay this zine out like the template, its photos in the frames in order", false, true)
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| this.use_design_here(ix, cx))),
                            ),
                    )
                    .into_any_element(),
            );
        }
        out
    }

    /// Grouping the photos and laying them out.
    pub(crate) fn arrange_card(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let ui = &self.zine_ui;
        let by = ui.arrange_by;
        let groupings = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_1()
            .children(ZineGrouping::ALL.map(|g| {
                pill(&theme, SharedString::from(format!("zine-by-{g:?}")), g.label(), g.description(), by == g, true)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.zine_ui.arrange_by = g;
                        cx.notify();
                    }))
                    .into_any_element()
            }));
        let per = div()
            .flex()
            .flex_row()
            .gap_1()
            .children([1usize, 2, 4].map(|n| {
                pill(
                    &theme,
                    SharedString::from(format!("zine-arrange-per-{n}")),
                    &format!("{n}"),
                    "Photos on each page",
                    ui.arrange_per_page == n,
                    true,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.zine_ui.arrange_per_page = n;
                    cx.notify();
                }))
                .into_any_element()
            }));
        let busy = ui.arranging;
        vec![
            small_caption(&theme, "Group by"),
            groupings.into_any_element(),
            div()
                .text_size(sp(10.5))
                .text_color(theme.text_faint)
                .child(by.description())
                .into_any_element(),
            small_caption(&theme, "Photos per page"),
            per.into_any_element(),
            pill(
                &theme,
                "zine-arrange-headings".into(),
                "A title page for each section",
                "Open every section with a page naming it",
                ui.arrange_headings,
                true,
            )
            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                this.zine_ui.arrange_headings = !this.zine_ui.arrange_headings;
                cx.notify();
            }))
            .into_any_element(),
            pill(
                &theme,
                "zine-arrange-go".into(),
                if busy { "Arranging\u{2026}" } else { "Arrange the photos" },
                "Put the photos in sections and lay the pages out. You can undo it.",
                false,
                !busy && !self.zine.photos.is_empty(),
            )
            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.arrange_photos(cx)))
            .into_any_element(),
        ]
    }

    /// The bar's controls for copying a page's look to other pages.
    pub(crate) fn look_controls(&self, page: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let ui = &self.zine_ui;
        let sheet = self.zine.manual.pages.get(page).is_some_and(|p| p.is_sheet());
        let has = ui.style_clipboard.is_some();
        let picked = ui.pages_selected.len();
        let parts = ui.style_parts;
        let toggle = |id: &'static str, label: &'static str, on: bool, flip: fn(&mut ZineStyleParts)| {
            pill(&theme, id.into(), label, "Whether the copied look includes this", on, true)
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    flip(&mut this.zine_ui.style_parts);
                    cx.notify();
                }))
                .into_any_element()
        };
        let apply = |id: &'static str, label: String, tip: &'static str, scope: LookScope, enabled: bool| {
            pill(&theme, id.into(), &label, tip, false, enabled)
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| this.zine_paste_look(scope, cx)))
                .into_any_element()
        };
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_end()
            .gap_x_3()
            .gap_y_1()
            .child(
                pill(&theme, "zine-copy-look".into(), "Copy look", "Remember this page's paper colour, photo frames and text setting", false, !sheet)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| this.zine_copy_look(page, cx))),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .child(toggle("zine-look-colour", "Page colour", parts.colour, |p| p.colour = !p.colour))
                    .child(toggle("zine-look-frames", "Photo frames", parts.frames, |p| p.frames = !p.frames))
                    .child(toggle("zine-look-text", "Text", parts.text, |p| p.text = !p.text)),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .child(apply("zine-look-all", "Put on all pages".to_string(), "Every page of the zine", LookScope::All, has))
                    .child(apply("zine-look-after", "On the pages after".to_string(), "Every page after this one", LookScope::After(page), has))
                    .child(apply(
                        "zine-look-picked",
                        if picked > 0 { format!("On {picked} picked") } else { "On picked pages".to_string() },
                        "The pages picked in the strip below (Cmd- or Shift-click them)",
                        LookScope::Picked,
                        has && picked > 0,
                    )),
            )
            .into_any_element()
    }
}

fn small_caption(theme: &Theme, text: &'static str) -> AnyElement {
    div()
        .pt_1()
        .text_size(sp(10.0))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(theme.text_faint)
        .child(text.to_uppercase())
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(r: u8, g: u8, b: u8) -> image::RgbImage {
        image::RgbImage::from_pixel(32, 32, image::Rgb([r, g, b]))
    }

    #[test]
    fn a_photos_colour_is_its_strong_hue_not_its_grey() {
        let (h, s, v) = dominant_colour(&flat(40, 90, 220)).unwrap();
        assert!((205.0..235.0).contains(&h), "{h}");
        assert!(s > 0.7 && v > 0.8);
        // Grey rock with a small bright blue patch: the blue is the hue.
        let mut image = flat(110, 110, 110);
        for y in 0..6 {
            for x in 0..6 {
                image.put_pixel(x, y, image::Rgb([20, 60, 230]));
            }
        }
        let (h, ..) = dominant_colour(&image).unwrap();
        assert!((200.0..240.0).contains(&h), "{h}");
        let (_, s, _) = dominant_colour(&flat(120, 120, 120)).unwrap();
        assert!(s < 0.05);
        let (_, _, v) = dominant_colour(&flat(5, 5, 8)).unwrap();
        assert!(v < 0.1);
    }
}
