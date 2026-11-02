//! The Zines module's left panel, built from the same pieces as the Export
//! module's panel: collapsible cards, balanced rows of choices, switches,
//! sliders and one kind of button, so the two read as one app.
//!
//! The panel is ordered by what you do: set the zine up (Zine, Pages, Look),
//! fill it (Suggested photos, Arrange, Story, Templates), then save it
//! (Output). What is rarely needed starts folded.

use gpui::{
    div, img, prelude::*, AnyElement, App, ClickEvent, Context, Entity, ObjectFit, SharedString,
    Window,
};
use rustyedit_core::{
    ZineFont, ZineFormat, ZineGrouping, ZineLayout, ZineMode, ZineOptions, ZinePageSize, ZinePaper,
    ZinePreset, ZineStyle,
};
use rustyedit_ui::{sp, Theme};

use crate::workspace::{TextTarget, Workspace};

// --- The pieces ---------------------------------------------------------------------------

/// The panel's building blocks, tied to the app's theme.
#[derive(Clone)]
pub(crate) struct Kit {
    theme: Theme,
    entity: Entity<Workspace>,
}

impl Kit {
    pub(crate) fn new(theme: Theme, entity: Entity<Workspace>) -> Self {
        Self { theme, entity }
    }

    /// A small heading over a control.
    pub(crate) fn caption(&self, text: impl Into<SharedString>) -> AnyElement {
        div()
            .text_size(sp(10.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(self.theme.text_faint)
            .child(text.into().to_uppercase())
            .into_any_element()
    }

    /// A line of explanation.
    pub(crate) fn hint(&self, text: impl Into<SharedString>) -> AnyElement {
        div()
            .text_size(sp(10.5))
            .line_height(gpui::relative(1.35))
            .text_color(self.theme.text_faint)
            .child(text.into())
            .into_any_element()
    }

    /// A control under its caption.
    pub(crate) fn field(&self, label: impl Into<SharedString>, control: AnyElement) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(self.caption(label))
            .child(control)
            .into_any_element()
    }

    /// A label at the left and a control at the right.
    pub(crate) fn row(&self, label: impl Into<SharedString>, control: AnyElement) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(self.theme.text_muted)
                    .child(label.into()),
            )
            .child(control)
            .into_any_element()
    }

    /// A line between groups.
    pub(crate) fn divider(&self) -> AnyElement {
        div()
            .h(sp(1.0))
            .w_full()
            .bg(self.theme.border_subtle)
            .into_any_element()
    }

    /// Choices in balanced rows that each spread across the width, the
    /// current one highlighted by colour alone (so nothing shifts when it
    /// changes). This is the Export panel's own control.
    pub(crate) fn segmented<T: Copy + PartialEq + 'static>(
        &self,
        id: &'static str,
        values: &[T],
        current: T,
        label: impl Fn(T) -> String,
        pick: impl Fn(&mut Workspace, T, &mut Context<Workspace>) + Clone + 'static,
    ) -> AnyElement {
        let theme = self.theme;
        let labels: Vec<String> = values.iter().map(|&v| label(v)).collect();
        let longest = labels.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        // As many to a row as the labels allow, then rows as even as
        // possible (7 choices make rows of 4 and 3, not 3, 3 and 1).
        let most = if longest <= 9 {
            4
        } else if longest <= 13 {
            3
        } else {
            2
        };
        let rows = values.len().div_ceil(most).max(1);
        let per_row = values.len().div_ceil(rows).max(1);

        let mut chips = values
            .iter()
            .zip(labels)
            .enumerate()
            .map(|(i, (&value, text))| {
                let entity = self.entity.clone();
                let pick = pick.clone();
                let selected = value == current;
                div()
                    .id(SharedString::from(format!("{id}-{i}")))
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .min_w(sp(0.0))
                    .h(sp(24.0))
                    .px_1()
                    .rounded_md()
                    .text_xs()
                    .cursor_pointer()
                    .border_1()
                    .when(selected, |el| {
                        el.bg(theme.accent_muted)
                            .border_color(theme.accent)
                            .text_color(theme.text)
                    })
                    .when(!selected, |el| {
                        el.border_color(theme.background)
                            .text_color(theme.text_muted)
                            .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                    })
                    .child(div().min_w(sp(0.0)).truncate().child(text))
                    .on_click(move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                        entity.update(cx, |this, cx| pick(this, value, cx));
                    })
                    .into_any_element()
            })
            .collect::<Vec<_>>()
            .into_iter();
        let mut lines = Vec::new();
        loop {
            let line: Vec<AnyElement> = chips.by_ref().take(per_row).collect();
            if line.is_empty() {
                break;
            }
            lines.push(div().flex().flex_row().gap_1().children(line).into_any_element());
        }
        div()
            .flex()
            .flex_col()
            .gap_1()
            .p_1()
            .rounded_lg()
            .bg(theme.background)
            .border_1()
            .border_color(theme.border_subtle)
            .children(lines)
            .into_any_element()
    }

    /// A label with an on/off switch.
    pub(crate) fn switch(
        &self,
        id: &'static str,
        label: &'static str,
        on: bool,
        flip: impl Fn(&mut Workspace, &mut Context<Workspace>) + 'static,
    ) -> AnyElement {
        let entity = self.entity.clone();
        self.row(
            label,
            rustyedit_ui::toggle_switch(id, on, &self.theme, move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                entity.update(cx, |this, cx| flip(this, cx));
            })
            .into_any_element(),
        )
    }

    /// A labelled slider.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn slider(
        &self,
        id: &'static str,
        label: &'static str,
        value: f32,
        min: f32,
        max: f32,
        default: f32,
        apply: impl Fn(&mut Workspace, f32, &mut Context<Workspace>) + 'static,
    ) -> AnyElement {
        let entity = self.entity.clone();
        rustyedit_ui::labeled_slider(
            id,
            label,
            value,
            rustyedit_ui::SliderSpec {
                min,
                max,
                default,
                signed: false,
                track: rustyedit_ui::SliderTrack::Amount,
            },
            &self.theme,
            move |v, _w: &mut Window, cx: &mut App| {
                entity.update(cx, |this, cx| apply(this, v, cx));
            },
        )
        .into_any_element()
    }

    /// The panel's one kind of button. `strong` marks the action a block
    /// exists for.
    pub(crate) fn button(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        tip: &str,
        strong: bool,
        enabled: bool,
        act: impl Fn(&mut Workspace, &mut Context<Workspace>) + 'static,
    ) -> AnyElement {
        let theme = self.theme;
        let entity = self.entity.clone();
        div()
            .id(id.into())
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .h(sp(26.0))
            .px_2()
            .rounded_md()
            .text_size(sp(11.5))
            .border_1()
            .when(strong && enabled, |el| {
                el.bg(theme.accent_muted)
                    .border_color(theme.accent)
                    .text_color(theme.text)
            })
            .when(!strong && enabled, |el| {
                el.border_color(theme.border).text_color(theme.text_muted)
            })
            .when(enabled, |el| {
                el.cursor_pointer()
                    .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                    .on_click(move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                        entity.update(cx, |this, cx| act(this, cx));
                    })
            })
            .when(!enabled, |el| {
                el.border_color(theme.border_subtle)
                    .text_color(theme.text_faint.opacity(0.6))
            })
            .tooltip(rustyedit_ui::tip(tip.to_string(), &theme))
            .child(label.into())
            .into_any_element()
    }

    /// Buttons side by side.
    pub(crate) fn buttons(&self, buttons: Vec<AnyElement>) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_1()
            .children(buttons)
            .into_any_element()
    }

    /// A tile in a list: a thumbnail or nothing, two lines, and what is at
    /// the right. The card the Export panel uses for its folder, too.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn item(
        &self,
        id: impl Into<SharedString>,
        selected: bool,
        thumb: Option<Option<std::path::PathBuf>>,
        title: String,
        detail: String,
        trailing: Option<AnyElement>,
        on_click: Option<Box<dyn Fn(&mut Workspace, &mut Context<Workspace>)>>,
    ) -> AnyElement {
        let theme = self.theme;
        let entity = self.entity.clone();
        let clickable = on_click.is_some();
        let on_click = on_click.map(std::rc::Rc::new);
        div()
            .id(id.into())
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .p_2()
            .rounded_lg()
            .border_1()
            .when(selected, |el| {
                el.bg(theme.accent_muted).border_color(theme.accent)
            })
            .when(!selected, |el| {
                el.bg(theme.background).border_color(theme.border_subtle)
            })
            .when(clickable, |el| {
                el.cursor_pointer().hover(|el| el.border_color(theme.accent))
            })
            .children(thumb.map(|path| {
                div()
                    .flex_shrink_0()
                    .w(sp(48.0))
                    .h(sp(36.0))
                    .rounded_md()
                    .overflow_hidden()
                    .bg(gpui::black())
                    .child(match path {
                        Some(path) => img(path).object_fit(ObjectFit::Contain).size_full().into_any_element(),
                        None => div().size_full().into_any_element(),
                    })
            }))
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
                            .child(title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(sp(10.5))
                            .text_color(theme.text_faint)
                            .child(detail),
                    ),
            )
            .children(trailing)
            .when_some(on_click, |el, act| {
                el.on_click(move |_: &ClickEvent, _w: &mut Window, cx: &mut App| {
                    entity.update(cx, |this, cx| act(this, cx));
                })
            })
            .into_any_element()
    }
}

// --- The cards ----------------------------------------------------------------------------------

/// The cards that start folded.
pub(crate) const FOLDED_AT_FIRST: [&str; 5] = [
    "zine-card-look",
    "zine-card-ideas",
    "zine-card-arrange",
    "zine-card-story",
    "zine-card-designs",
];

impl Workspace {
    pub(crate) fn update_zine_options(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut ZineOptions),
    ) {
        self.update_zine(cx, |zine| change(&mut zine.options));
    }

    /// Every card of the panel, in the order you work in.
    pub(crate) fn zine_panel_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let kit = Kit::new(self.theme, cx.entity());
        let options = self.zine.options.clone();
        let zine_title = if options.title.trim().is_empty() {
            "Untitled".to_string()
        } else {
            options.title.clone()
        };
        let pages_summary = format!(
            "{} \u{00B7} {}",
            if options.layout == ZineLayout::FoldedSheet { "Folded sheet" } else { options.layout.label() },
            options.page_size.label()
        );
        let ideas = self.zine_suggestions(12).len();
        let words = self.zine.story.split_whitespace().count();
        vec![
            self.export_card("zine-card-zine", "Zine", zine_title, self.card_zine(&kit, cx), cx),
            self.export_card("zine-card-pages", "Pages", pages_summary, self.card_pages(&kit, &options, cx), cx),
            self.export_card("zine-card-look", "Look", options.style.label().to_string(), self.card_look(&kit, &options), cx),
            self.export_card(
                "zine-card-ideas",
                "Suggested photos",
                if ideas == 0 { String::new() } else { ideas.to_string() },
                self.card_ideas(&kit, cx),
                cx,
            ),
            self.export_card(
                "zine-card-arrange",
                "Arrange by theme",
                self.zine_ui.arrange_by.label().to_string(),
                self.card_arrange(&kit),
                cx,
            ),
            self.export_card(
                "zine-card-story",
                "Story",
                if words == 0 { String::new() } else { format!("{words} words") },
                self.card_story(&kit, cx),
                cx,
            ),
            self.export_card(
                "zine-card-designs",
                "Templates",
                if self.zine_designs.designs.is_empty() { String::new() } else { self.zine_designs.designs.len().to_string() },
                self.card_designs(&kit, cx),
                cx,
            ),
            self.export_card(
                "zine-card-output",
                "Output",
                options.format.label().to_string(),
                self.card_output(&kit, &options, cx),
                cx,
            ),
        ]
    }

    /// Which zine this is, and the others.
    fn card_zine(&self, kit: &Kit, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let options = &self.zine.options;
        let many = self.zine_library.zines.len() > 1;
        let mut out: Vec<AnyElement> = Vec::new();
        if many {
            let active = self.zine_library.active;
            for (ix, zine) in self.zine_library.zines.iter().enumerate() {
                // The one being worked on has its latest in `self.zine`.
                let zine = if ix == active { &self.zine } else { zine };
                let count = zine.photos.len();
                let confirming = self.zine_ui.confirm_delete == Some(ix);
                let delete = div()
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
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| this.zine_delete(ix, cx)))
                    .into_any_element();
                out.push(kit.item(
                    SharedString::from(format!("zine-row-{ix}")),
                    ix == active,
                    None,
                    zine.options.title.clone(),
                    format!("{count} photo{} \u{00B7} {}", crate::zines::plural(count), zine.options.format.label()),
                    Some(delete),
                    Some(Box::new(move |this, cx| this.zine_switch(ix, cx))),
                ));
            }
        }
        out.push(kit.buttons(vec![
            kit.button("zine-new", "New zine", "Start another zine", false, true, |this, cx| this.zine_create(cx)),
            kit.button("zine-copy", "Duplicate", "A copy of this zine, to try another look", false, true, |this, cx| {
                this.zine_duplicate_zine(cx)
            }),
        ]));
        out.push(kit.divider());
        out.push(kit.field(
            "Title",
            self.zine_text_field("zine-title", TextTarget::ZineTitle, &options.title, "Untitled zine", cx),
        ));
        out.push(kit.field(
            "Author",
            self.zine_text_field("zine-author", TextTarget::ZineAuthor, &options.author, "Your name", cx),
        ));
        out
    }

    /// How the pages are laid out.
    fn card_pages(&self, kit: &Kit, options: &ZineOptions, _cx: &mut Context<Self>) -> Vec<AnyElement> {
        let folded = options.layout == ZineLayout::FoldedSheet;
        let mut out = vec![
            kit.field(
                "Arrangement",
                kit.segmented("zine-mode", &ZineMode::ALL, options.mode, |m| m.label().to_string(), |this, v, cx| {
                    this.update_zine_options(cx, |o| o.mode = v)
                }),
            ),
            kit.hint(match options.mode {
                ZineMode::Auto => "The photos flow onto the pages in order.",
                ZineMode::Manual => "Drag each photo to the page and place you want.",
            }),
            kit.field(
                "Photos per page",
                kit.segmented("zine-layout", &ZineLayout::ALL, options.layout, |l| l.label().to_string(), |this, v, cx| {
                    this.update_zine_options(cx, |o| o.layout = v)
                }),
            ),
            kit.hint(options.layout.description()),
            kit.field(
                "Page size",
                kit.segmented("zine-size", &ZinePageSize::ALL, options.page_size, |s| s.label().to_string(), |this, v, cx| {
                    this.update_zine_options(cx, |o| o.page_size = v)
                }),
            ),
        ];
        if !folded {
            out.push(kit.field(
                "Orientation",
                kit.segmented(
                    "zine-orientation",
                    &[false, true],
                    options.landscape,
                    |landscape| if landscape { "Landscape" } else { "Portrait" }.to_string(),
                    |this, v, cx| this.update_zine_options(cx, |o| o.landscape = v),
                ),
            ));
        }
        out.push(kit.slider("zine-margin", "Margin (mm)", options.margin_mm as f32, 0.0, 30.0, 12.0, |this, v, cx| {
            this.update_zine_options(cx, |o| o.margin_mm = v.round().clamp(0.0, 30.0) as u8)
        }));
        out.push(kit.divider());
        out.push(kit.switch("zine-cover", "Cover page", options.cover, |this, cx| {
            this.update_zine_options(cx, |o| o.cover = !o.cover)
        }));
        out.push(kit.switch("zine-captions", "File names under photos", options.captions, |this, cx| {
            this.update_zine_options(cx, |o| o.captions = !o.captions)
        }));
        out
    }

    /// The zine's style, type and paper.
    fn card_look(&self, kit: &Kit, options: &ZineOptions) -> Vec<AnyElement> {
        vec![
            kit.field(
                "Style",
                kit.segmented("zine-style", &ZineStyle::ALL, options.style, |s| s.label().to_string(), |this, v, cx| {
                    this.update_zine(cx, |z| z.apply_style(v))
                }),
            ),
            kit.hint(options.style.description()),
            kit.field(
                "Typeface",
                kit.segmented("zine-font", &ZineFont::ALL, options.font, |f| f.label().to_string(), |this, v, cx| {
                    this.update_zine_options(cx, |o| o.font = v)
                }),
            ),
            kit.field(
                "Paper",
                kit.segmented("zine-paper", &ZinePaper::ALL, options.paper, |p| p.label().to_string(), |this, v, cx| {
                    this.update_zine_options(cx, |o| o.paper = v)
                }),
            ),
        ]
    }

    /// The library's best photos, to add.
    fn card_ideas(&self, kit: &Kit, _cx: &mut Context<Self>) -> Vec<AnyElement> {
        let ideas = self.zine_suggestions(12);
        if ideas.is_empty() {
            return vec![kit.hint("Rate or flag photos in the Library and the best of them show up here.")];
        }
        let mut out = vec![kit.hint("The highest rated, picked and sharpest photos not yet in this zine.")];
        for &ix in ideas.iter().take(6) {
            let asset = &self.catalog.assets[ix];
            let stars = "\u{2605}".repeat(asset.rating.value() as usize);
            let detail = match (asset.flag == rustyedit_core::Flag::Picked, stars.is_empty()) {
                (true, true) => "picked".to_string(),
                (true, false) => format!("{stars} \u{00B7} picked"),
                (false, true) => "unrated".to_string(),
                (false, false) => stars,
            };
            let add = kit.button(
                SharedString::from(format!("zine-idea-add-{ix}")),
                "Add",
                "Put this photo in the zine",
                false,
                true,
                move |this, cx| this.add_suggested(&[ix], cx),
            );
            out.push(kit.item(
                SharedString::from(format!("zine-idea-{ix}")),
                false,
                Some(self.display_thumbnail(asset)),
                asset.display_name(),
                detail,
                Some(div().w(sp(46.0)).flex_shrink_0().child(add).into_any_element()),
                None,
            ));
        }
        let all = ideas.clone();
        out.push(kit.buttons(vec![kit.button(
            "zine-idea-add-all",
            format!("Add all {}", ideas.len()),
            "Put every suggested photo in the zine",
            true,
            true,
            move |this, cx| this.add_suggested(&all, cx),
        )]));
        out
    }

    /// Grouping the photos and laying them out.
    fn card_arrange(&self, kit: &Kit) -> Vec<AnyElement> {
        let ui = &self.zine_ui;
        let busy = ui.arranging;
        let mut out = vec![
            kit.field(
                "Group by",
                kit.segmented("zine-by", &ZineGrouping::ALL, ui.arrange_by, |g| g.label().to_string(), |this, v, cx| {
                    this.zine_ui.arrange_by = v;
                    cx.notify();
                }),
            ),
            kit.hint(ui.arrange_by.description()),
            kit.divider(),
            kit.switch("zine-arrange-smart", "Smart layout", ui.arrange_smart, |this, cx| {
                this.zine_ui.arrange_smart = !this.zine_ui.arrange_smart;
                cx.notify();
            }),
            kit.hint(if ui.arrange_smart {
                "Each section opens with its best photo, large. The cover is the best of all, and pages are shaped to tall and wide photos."
            } else {
                "Photos fill the pages in order, as many to a page as you choose."
            }),
        ];
        if !ui.arrange_smart {
            out.push(kit.field(
                "Photos per page",
                kit.segmented("zine-arrange-per", &[1usize, 2, 4], ui.arrange_per_page, |n| n.to_string(), |this, v, cx| {
                    this.zine_ui.arrange_per_page = v;
                    cx.notify();
                }),
            ));
            out.push(kit.switch("zine-arrange-headings", "A title page for each section", ui.arrange_headings, |this, cx| {
                this.zine_ui.arrange_headings = !this.zine_ui.arrange_headings;
                cx.notify();
            }));
        }
        out.push(kit.buttons(vec![kit.button(
            "zine-arrange-go",
            if busy { "Arranging\u{2026}" } else { "Arrange the photos" },
            "Put the photos in sections and lay the pages out. You can undo it.",
            true,
            !busy && !self.zine.photos.is_empty(),
            |this, cx| this.arrange_photos(cx),
        )]));
        out
    }

    /// Writing the story and splitting it across the pages.
    fn card_story(&self, kit: &Kit, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let typing = self.draft_for(TextTarget::ZineStory).map(str::to_string);
        let story = self.zine.story.clone();
        let words = story.split_whitespace().count();
        let field = div()
            .id("zine-story-field")
            .flex()
            .flex_col()
            .min_h(sp(72.0))
            .max_h(sp(160.0))
            .overflow_hidden()
            .px_2()
            .py_1p5()
            .rounded_md()
            .bg(theme.background)
            .border_1()
            .text_size(sp(11.5))
            .map(|el| match typing {
                Some(draft) => el
                    .border_color(theme.accent)
                    .text_color(theme.text)
                    .child(format!("{draft}\u{258F}")),
                None => {
                    let current = story.clone();
                    el.border_color(theme.border)
                        .text_color(if story.is_empty() { theme.text_faint } else { theme.text })
                        .cursor_text()
                        .hover(|el| el.border_color(theme.accent))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            this.text_draft = Some((TextTarget::ZineStory, current.clone()));
                            cx.notify();
                        }))
                        .child(if story.is_empty() {
                            "Click to write it. Shift+Enter starts a new line, Enter keeps it.".to_string()
                        } else {
                            story.clone()
                        })
                }
            })
            .into_any_element();
        let has_photo_pages = self
            .zine
            .manual
            .pages
            .iter()
            .any(|p| !p.is_sheet() && (p.slots.iter().any(Option::is_some) || p.items.iter().any(|i| i.photo.is_some())));
        vec![
            kit.hint("Write it once. It is split by paragraph or sentence and set as a caption under the photos of each page."),
            field,
            kit.buttons(vec![
                kit.button(
                    "zine-story-split",
                    "Split across the pages",
                    "Caption each page that has photos with its share of the story",
                    true,
                    words > 0 && !self.zine.photos.is_empty(),
                    |this, cx| this.split_story_now(cx),
                ),
                kit.button(
                    "zine-story-clear",
                    "Remove captions",
                    "Take the story's captions off the pages",
                    false,
                    has_photo_pages,
                    |this, cx| this.update_zine(cx, |z| z.clear_story_captions()),
                ),
            ]),
        ]
    }

    /// Saved templates, and saving this zine's.
    fn card_designs(&self, kit: &Kit, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = vec![
            kit.hint("A template remembers a zine's set-up and where its photos and text go."),
            kit.field(
                "Save this zine as a template",
                self.zine_text_field("zine-design-name", TextTarget::ZineDesignName, "", "Name it, then press Enter", cx),
            ),
        ];
        if self.zine_designs.designs.is_empty() {
            out.push(kit.hint("No templates yet."));
        }
        for (ix, design) in self.zine_designs.designs.iter().enumerate() {
            let theme = self.theme;
            let delete = div()
                .id(SharedString::from(format!("zine-design-delete-{ix}")))
                .px_1p5()
                .text_size(sp(10.0))
                .text_color(theme.text_faint)
                .cursor_pointer()
                .hover(|el| el.text_color(theme.danger))
                .tooltip(rustyedit_ui::tip("Delete the template", &theme))
                .child("\u{2715}")
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| this.delete_design(ix, cx)))
                .into_any_element();
            out.push(kit.item(
                SharedString::from(format!("zine-design-{ix}")),
                false,
                None,
                design.name.clone(),
                design.summary(),
                Some(delete),
                None,
            ));
            out.push(kit.buttons(vec![
                kit.button(
                    SharedString::from(format!("zine-design-new-{ix}")),
                    "New zine",
                    "Start a new zine laid out like this",
                    false,
                    true,
                    move |this, cx| this.zine_from_design(ix, cx),
                ),
                kit.button(
                    SharedString::from(format!("zine-design-use-{ix}")),
                    "Use here",
                    "Lay this zine out like the template, its photos in the frames in order",
                    false,
                    true,
                    move |this, cx| this.use_design_here(ix, cx),
                ),
            ]));
        }
        out
    }

    /// What it is saved as, and where.
    fn card_output(&self, kit: &Kit, options: &ZineOptions, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let presets: Vec<AnyElement> = ZinePreset::ALL
            .iter()
            .map(|&preset| {
                kit.button(
                    SharedString::from(format!("zine-preset-{preset:?}")),
                    preset.label(),
                    preset.description(),
                    false,
                    true,
                    move |this, cx| this.update_zine_options(cx, |o| preset.apply(o)),
                )
            })
            .collect();
        let mut out = vec![
            kit.field("Quick set-up", kit.buttons(presets)),
            kit.field(
                "Save as",
                kit.segmented("zine-format", &ZineFormat::ALL, options.format, |f| f.label().to_string(), |this, v, cx| {
                    this.update_zine_options(cx, |o| o.format = v)
                }),
            ),
            kit.hint(options.format.description()),
            kit.slider("zine-quality", "Picture quality", options.quality as f32, 50.0, 100.0, 88.0, |this, v, cx| {
                this.update_zine_options(cx, |o| o.quality = v.round().clamp(50.0, 100.0) as u8)
            }),
        ];
        if options.format.is_page_pictures() {
            out.push(kit.slider(
                "zine-image-edge",
                "Size (pixels, long side)",
                options.image_edge as f32,
                600.0,
                4000.0,
                1600.0,
                |this, v, cx| this.update_zine_options(cx, |o| o.image_edge = (v / 20.0).round() as u32 * 20),
            ));
        }
        out.push(kit.field("Save in", self.zine_destination(options, cx)));
        out
    }
}
