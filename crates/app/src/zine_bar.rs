//! The bar above the pages in the Zines workbench: what can be done with the
//! page being worked on (a template, its paper colour, copying it) and with
//! what is selected on it (photo or text settings, aligning several).

use gpui::{div, prelude::*, ClickEvent, Context, SharedString, Window};
use rustyedit_core::{
    ZineAlignMode, ZineDistributeAxis, ZineFit, ZineFont, ZineFreeItem, ZineTemplate,
    ZineTextAlign, ZINE_MAX_MAT_MM, ZINE_MAX_RADIUS_MM, ZINE_MAX_TEXT_PT, ZINE_MIN_TEXT_PT,
};
use rustyedit_ui::{sp, Theme};

use crate::{
    workspace::{TextTarget, Workspace},
    zine_pages::NewItem,
};

/// Paper colours offered for a page (and mats): none means the zine's own.
const PAPERS: [(&str, [u8; 3]); 8] = [
    ("White", [255, 255, 255]),
    ("Cream", [247, 241, 226]),
    ("Sand", [232, 220, 196]),
    ("Sage", [200, 213, 190]),
    ("Sky", [196, 214, 230]),
    ("Rose", [236, 205, 205]),
    ("Grey", [150, 150, 150]),
    ("Black", [14, 14, 14]),
];

/// Colours offered for text: the paper's own ink is `None`.
const INKS: [(&str, [u8; 3]); 6] = [
    ("Black", [20, 20, 20]),
    ("White", [255, 255, 255]),
    ("Red", [200, 50, 45]),
    ("Blue", [40, 80, 170]),
    ("Green", [50, 120, 70]),
    ("Gold", [196, 150, 40]),
];

fn rgb_colour(rgb: [u8; 3]) -> gpui::Hsla {
    gpui::rgb(u32::from(rgb[0]) << 16 | u32::from(rgb[1]) << 8 | u32::from(rgb[2])).into()
}

impl Workspace {
    /// What can be done with the page being worked on and with what is
    /// selected on it, as blocks for the side bar.
    pub(crate) fn zine_context_sections(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = &self.zine_ui;
        let pages = self.zine.manual.pages.len();
        let page = ui.page.filter(|&p| p < pages).or(pages.checked_sub(1));

        let mut sections: Vec<gpui::AnyElement> = Vec::new();
        match page {
            Some(page) => sections.push(self.zine_card(
                "zine-side-page",
                format!("Page {}", page + usize::from(self.zine.options.cover && !self.zine.manual.pages[page].is_sheet()) + 1),
                self.page_group(page, cx),
                cx,
            )),
            None => sections.push(
                div()
                    .px_2()
                    .text_size(sp(rustyedit_ui::type_size::BODY))
                    .text_color(theme.text_faint)
                    .child(if pages == 0 {
                        "Add a page to begin."
                    } else {
                        "Click a page to work on it."
                    })
                    .into_any_element(),
            ),
        }
        if let Some(group) = page.and_then(|p| self.selection_group(p, cx)) {
            let count = ui.items.len();
            let title = if count == 1 { "Selection".to_string() } else { format!("{count} selected") };
            sections.push(self.zine_card("zine-side-selection", title, group, cx));
        }
        div()
            .flex()
            .flex_col()
            .children(sections)
            .into_any_element()
    }

    /// What can be done to the whole page.
    fn page_group(&self, page: usize, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let manual = &self.zine.manual.pages[page];
        let sheet = manual.is_sheet();
        let templates = ZineTemplate::ALL.map(|t| {
            pill(
                &theme,
                SharedString::from(format!("zine-template-{t:?}")),
                t.label(),
                t.description(),
                false,
                !sheet,
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                this.zine_ui.clear_selection();
                this.update_zine(cx, |z| {
                    z.apply_template(page, t);
                });
            }))
            .into_any_element()
        });
        let current = manual.background;
        let mut swatches = vec![swatch(
            &theme,
            "zine-bg-auto".into(),
            None,
            current.is_none(),
            "The zine's own paper",
        )
        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
            this.update_zine(cx, |z| z.set_page_background(page, None))
        }))
        .into_any_element()];
        swatches.extend(PAPERS.iter().map(|&(name, rgb)| {
            swatch(
                &theme,
                SharedString::from(format!("zine-bg-{name}")),
                Some(rgb),
                current == Some(rgb),
                name,
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                this.update_zine(cx, |z| z.set_page_background(page, Some(rgb)))
            }))
            .into_any_element()
        }));
        let has_clipboard = self.zine_ui.layout_clipboard.is_some();
        let actions = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_1()
            .child(
                pill(&theme, "zine-add-text".into(), "Add text", "Put a box of text on this page", false, !sheet)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.add_new_item(NewItem::Text, page, None, cx)
                    })),
            )
            .child(
                pill(&theme, "zine-dup-page".into(), "Duplicate page", "Copy this page, photos and all, right after it", false, true)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.zine_ui.clear_selection();
                        this.update_zine(cx, |z| {
                            z.duplicate_page(page);
                        });
                        this.zine_ui.page = Some(page + 1);
                    })),
            )
            .child(
                pill(&theme, "zine-copy-layout".into(), "Copy layout", "Remember where this page's photos and text sit", false, !sheet)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| this.zine_copy_layout(page, cx))),
            )
            .child(
                pill(
                    &theme,
                    "zine-paste-layout".into(),
                    "Paste layout",
                    "Lay this page out as the copied one, keeping its own photos",
                    false,
                    has_clipboard && !sheet,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| this.zine_paste_layout(page, cx))),
            );
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(labelled(&theme, "Template", div().flex().flex_row().flex_wrap().gap_1().children(templates).into_any_element()))
            .child(labelled(&theme, "Page colour", div().flex().flex_row().flex_wrap().gap_1().children(swatches).into_any_element()))
            .child(labelled(&theme, "Page", actions.into_any_element()))
            .child(labelled(&theme, "Copy a page's look", self.look_controls(page, cx)))
            .into_any_element()
    }

    /// What can be done with the selected boxes of `page`.
    fn selection_group(&self, page: usize, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let ui = &self.zine_ui;
        if ui.page != Some(page) || ui.items.is_empty() {
            return None;
        }
        let manual = self.zine.manual.pages.get(page)?;
        let mut parts: Vec<gpui::AnyElement> = Vec::new();

        if let Some((_, ix)) = ui.single() {
            let item = manual.items.get(ix)?.clone();
            if let Some(text) = &item.text {
                parts.extend(self.text_controls(page, ix, text, cx));
            } else {
                parts.extend(self.photo_controls(page, ix, &item, cx));
            }
            parts.push(self.common_controls(page, ix, &item, cx));
        } else {
            parts.push(self.group_controls(cx));
        }
        Some(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .children(parts)
                .into_any_element(),
        )
    }

    fn photo_controls(&self, page: usize, ix: usize, item: &ZineFreeItem, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let theme = self.theme;
        let frame = item.frame;
        let fits = ZineFit::ALL.map(|f| {
            pill(
                &theme,
                SharedString::from(format!("zine-fit-{f:?}")),
                f.label(),
                match f {
                    ZineFit::Contain => "All of the photo, with room round it",
                    ZineFit::Fill => "Fill the frame, cropping the photo",
                },
                frame.fit == f,
                true,
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                this.zine_edit_selection(cx, move |i| i.frame.fit = f)
            }))
            .into_any_element()
        });
        let mut fit_row: Vec<gpui::AnyElement> = fits.into_iter().collect();
        if frame.fit == ZineFit::Fill {
            let on = self.zine_ui.reposition;
            fit_row.push(
                pill(
                    &theme,
                    "zine-reposition".into(),
                    if on { "Done" } else { "Reposition" },
                    "Drag the photo to choose what shows in its frame",
                    on,
                    item.photo.is_some(),
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                    this.zine_ui.reposition = !this.zine_ui.reposition;
                    cx.notify();
                }))
                .into_any_element(),
            );
        }
        let mats: Vec<gpui::AnyElement> = [[255, 255, 255], [247, 241, 226], [14, 14, 14]]
            .into_iter()
            .map(|rgb| {
                swatch(
                    &theme,
                    SharedString::from(format!("zine-mat-{rgb:?}")),
                    Some(rgb),
                    frame.mat == rgb,
                    "Mat colour",
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.zine_edit_selection(cx, move |i| i.frame.mat = rgb)
                }))
                .into_any_element()
            })
            .collect();
        let _ = ix;
        let _ = page;
        vec![
            labelled(&theme, "Photo", div().flex().flex_row().flex_wrap().gap_1().children(fit_row).into_any_element()),
            labelled(
                &theme,
                "Mat",
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(self.bar_slider("zine-mat", "Width", frame.mat_mm, 0.0, ZINE_MAX_MAT_MM, 0.0, |i, v| {
                        i.frame.mat_mm = v
                    }, cx))
                    .child(div().flex().flex_row().flex_wrap().gap_1().children(mats))
                    .into_any_element(),
            ),
            labelled(
                &theme,
                "Frame",
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(self.bar_slider("zine-radius", "Corners", frame.radius_mm, 0.0, ZINE_MAX_RADIUS_MM, 0.0, |i, v| {
                        i.frame.radius_mm = v
                    }, cx))
                    .child(
                        pill(&theme, "zine-shadow".into(), "Shadow", "A soft drop shadow", frame.shadow, true)
                            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                                this.zine_edit_selection(cx, |i| i.frame.shadow = !i.frame.shadow)
                            })),
                    )
                    .into_any_element(),
            ),
            labelled(
                &theme,
                "Tilt",
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(self.bar_slider("zine-tilt", "Degrees", item.rotation, -180.0, 180.0, 0.0, |i, v| {
                        i.rotation = v.round()
                    }, cx))
                    .child(
                        pill(&theme, "zine-tilt-reset".into(), "Level", "Turn it back upright", false, item.rotation != 0.0)
                            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                                this.zine_edit_selection(cx, |i| i.rotation = 0.0)
                            })),
                    )
                    .into_any_element(),
            ),
        ]
    }

    fn text_controls(
        &self,
        page: usize,
        ix: usize,
        text: &rustyedit_core::ZineTextBox,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let theme = self.theme;
        let typing = self.draft_for(TextTarget::ZineText).is_some();
        let words = match self.draft_for(TextTarget::ZineText) {
            Some(draft) => div()
                .flex()
                .items_center()
                .w_full()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(theme.surface)
                .border_1()
                .border_color(theme.accent)
                .text_size(sp(rustyedit_ui::type_size::LABEL))
                .text_color(theme.text)
                .child(draft.to_string())
                .child(div().w(sp(1.0)).h(sp(14.0)).ml(sp(1.0)).bg(theme.accent))
                .into_any_element(),
            None => div()
                .id("zine-text-words")
                .flex()
                .items_center()
                .w_full()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(theme.surface)
                .border_1()
                .border_color(theme.border)
                .text_size(sp(rustyedit_ui::type_size::LABEL))
                .text_color(theme.text)
                .cursor_text()
                .hover(|el| el.border_color(theme.accent))
                .tooltip(rustyedit_ui::tip("Click to type (double-click the text on the page does the same)", &theme))
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| this.start_text_edit(page, ix, cx)))
                .child(SharedString::from(text.text.replace('\n', " \u{21B5} ")))
                .into_any_element(),
        };
        let fonts = ZineFont::ALL.map(|f| {
            pill(&theme, SharedString::from(format!("zine-font-{f:?}")), f.label(), "The typeface", text.font == f, true)
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.zine_edit_selection(cx, move |i| {
                        if let Some(t) = i.text.as_mut() {
                            t.font = f;
                        }
                    })
                }))
                .into_any_element()
        });
        let aligns = ZineTextAlign::ALL.map(|a| {
            pill(&theme, SharedString::from(format!("zine-align-{a:?}")), a.label(), "Where the lines sit in the box", text.align == a, true)
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.zine_edit_selection(cx, move |i| {
                        if let Some(t) = i.text.as_mut() {
                            t.align = a;
                        }
                    })
                }))
                .into_any_element()
        });
        let mut inks = vec![swatch(&theme, "zine-ink-auto".into(), None, text.colour.is_none(), "The paper's own ink")
            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                this.zine_edit_selection(cx, |i| {
                    if let Some(t) = i.text.as_mut() {
                        t.colour = None;
                    }
                })
            }))
            .into_any_element()];
        inks.extend(INKS.iter().map(|&(name, rgb)| {
            swatch(&theme, SharedString::from(format!("zine-ink-{name}")), Some(rgb), text.colour == Some(rgb), name)
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.zine_edit_selection(cx, move |i| {
                        if let Some(t) = i.text.as_mut() {
                            t.colour = Some(rgb);
                        }
                    })
                }))
                .into_any_element()
        }));
        vec![
            labelled(
                &theme,
                if typing { "Typing \u{2014} Enter keeps it, Shift+Enter starts a line, Esc cancels" } else { "Text" },
                words,
            ),
            labelled(&theme, "Typeface", div().flex().flex_row().flex_wrap().gap_1().children(fonts).into_any_element()),
            labelled(
                &theme,
                "Size",
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(self.bar_slider("zine-text-size", "Points", text.size_pt, ZINE_MIN_TEXT_PT, ZINE_MAX_TEXT_PT, 14.0, |i, v| {
                        if let Some(t) = i.text.as_mut() {
                            t.size_pt = v.round();
                        }
                    }, cx))
                    .child(
                        pill(&theme, "zine-text-bold".into(), "Bold", "Heavier letters", text.bold, true)
                            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                                this.zine_edit_selection(cx, |i| {
                                    if let Some(t) = i.text.as_mut() {
                                        t.bold = !t.bold;
                                    }
                                })
                            })),
                    )
                    .into_any_element(),
            ),
            labelled(&theme, "Align", div().flex().flex_row().flex_wrap().gap_1().children(aligns).into_any_element()),
            labelled(
                &theme,
                "Colour",
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .children(inks)
                    .child(
                        pill(
                            &theme,
                            "zine-text-backdrop".into(),
                            "Tint behind",
                            "A half see-through dark band behind the text, to read it over a photo",
                            text.backdrop.is_some(),
                            true,
                        )
                        .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                            this.zine_edit_selection(cx, |i| {
                                if let Some(t) = i.text.as_mut() {
                                    t.backdrop = if t.backdrop.is_some() { None } else { Some([0, 0, 0]) };
                                }
                            })
                        })),
                    )
                    .into_any_element(),
            ),
        ]
    }

    /// Stack, centre, fit, copy and remove: for one box.
    fn common_controls(&self, page: usize, ix: usize, item: &ZineFreeItem, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let _ = item;
        labelled(
            &theme,
            "Box",
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_1()
                .child(
                    pill(&theme, "zine-front".into(), "To front", "Stack it over the others", false, true)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            let mut moved = None;
                            this.update_zine(cx, |z| moved = z.free_raise(page, ix));
                            if let Some(i) = moved {
                                this.zine_select(page, vec![i], cx);
                            }
                        })),
                )
                .child(
                    pill(&theme, "zine-centre".into(), "Centre", "Centre it across the page", false, true)
                        .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                            this.zine_edit_selection(cx, |i| i.x = (1.0 - i.w) / 2.0)
                        })),
                )
                .child(
                    pill(&theme, "zine-fit-width".into(), "Fit width", "Stretch to the margins, keeping its shape", false, true)
                        .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                            let (pw, _) = this.zine.manual_page_size();
                            let margin = this.zine.options.margin_mm as f32 / pw;
                            this.zine_edit_selection(cx, move |i| {
                                let width = 1.0 - 2.0 * margin;
                                i.h *= width / i.w.max(0.01);
                                i.x = margin;
                                i.w = width;
                            })
                        })),
                )
                .child(
                    pill(&theme, "zine-duplicate".into(), "Duplicate", "A copy in front (\u{2318}D)", false, true)
                        .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.zine_duplicate_selection(cx))),
                )
                .child(
                    pill(&theme, "zine-remove".into(), "Remove", "Take it off the page (Delete)", false, true)
                        .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.zine_delete_selection(cx))),
                )
                .into_any_element(),
        )
    }

    /// Aligning and spacing several selected boxes.
    fn group_controls(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let count = self.zine_ui.items.len();
        let aligns = ZineAlignMode::ALL.map(|mode| {
            let short = match mode {
                ZineAlignMode::Left => "Left",
                ZineAlignMode::CentreHorizontally => "Centre",
                ZineAlignMode::Right => "Right",
                ZineAlignMode::Top => "Top",
                ZineAlignMode::CentreVertically => "Middle",
                ZineAlignMode::Bottom => "Bottom",
            };
            pill(&theme, SharedString::from(format!("zine-al-{mode:?}")), short, mode.label(), false, true)
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| this.zine_align(mode, cx)))
                .into_any_element()
        });
        let spread = [
            (ZineDistributeAxis::Horizontally, "Across", "Space them evenly left to right"),
            (ZineDistributeAxis::Vertically, "Down", "Space them evenly top to bottom"),
        ]
        .map(|(axis, label, tip)| {
            pill(&theme, SharedString::from(format!("zine-dist-{label}")), label, tip, false, count >= 3)
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| this.zine_distribute(axis, cx)))
                .into_any_element()
        });
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(labelled(&theme, &format!("{count} selected \u{2014} align"), div().flex().flex_row().flex_wrap().gap_1().children(aligns).into_any_element()))
            .child(labelled(&theme, "Space evenly", div().flex().flex_row().flex_wrap().gap_1().children(spread).into_any_element()))
            .child(labelled(
                &theme,
                "Together",
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        pill(&theme, "zine-group-dup".into(), "Duplicate", "Copies in front (\u{2318}D)", false, true)
                            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.zine_duplicate_selection(cx))),
                    )
                    .child(
                        pill(&theme, "zine-group-remove".into(), "Remove", "Take them off the page (Delete)", false, true)
                            .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.zine_delete_selection(cx))),
                    )
                    .into_any_element(),
            ))
            .into_any_element()
    }

    /// A slider that changes every selected box, folding a drag into one
    /// undo step.
    #[allow(clippy::too_many_arguments)]
    fn bar_slider(
        &self,
        id: &'static str,
        label: &'static str,
        value: f32,
        min: f32,
        max: f32,
        default: f32,
        apply: fn(&mut ZineFreeItem, f32),
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let entity = cx.entity();
        div()
            .w_full()
            .child(rustyedit_ui::labeled_slider(
                id,
                label,
                value,
                rustyedit_ui::SliderSpec {
                    min,
                    max,
                    default,
                    signed: min < 0.0,
                    track: rustyedit_ui::SliderTrack::Accent,
                },
                &theme,
                move |v, _w: &mut Window, cx: &mut gpui::App| {
                    entity.update(cx, |this, cx| {
                        this.zine_edit_selection_keyed(id, cx, move |i| apply(i, v))
                    });
                },
            ))
            .into_any_element()
    }

    // --- Typing in a text box -------------------------------------------------------

    /// Starts typing in a text box: its words become a draft, shown in the
    /// bar and on the page as they are typed.
    pub(crate) fn start_text_edit(&mut self, page: usize, item: usize, cx: &mut Context<Self>) {
        let Some(text) = self
            .zine
            .manual
            .pages
            .get(page)
            .and_then(|p| p.items.get(item))
            .and_then(|i| i.text.as_ref())
            .map(|t| t.text.clone())
        else {
            return;
        };
        self.zine_ui.page = Some(page);
        self.zine_ui.items = vec![item];
        self.zine_ui.text_edit = Some((page, item));
        self.text_draft = Some((TextTarget::ZineText, text));
        cx.notify();
    }

    /// Enter in a text box's field: its words are what was typed.
    pub(crate) fn commit_zine_item_text(&mut self, text: String, cx: &mut Context<Self>) {
        let Some((page, item)) = self.zine_ui.text_edit.take() else {
            return;
        };
        self.update_zine(cx, |z| {
            z.free_edit(page, item, |i| {
                if let Some(t) = i.text.as_mut() {
                    t.text = text;
                }
            })
        });
    }
}

/// A small button.
pub(crate) fn pill(
    theme: &Theme,
    id: SharedString,
    label: &str,
    tip: &str,
    on: bool,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    let theme = *theme;
    div()
        .id(id)
        .flex()
        .items_center()
        .h(sp(24.0))
        .px_2()
        .rounded_md()
        .text_size(sp(rustyedit_ui::type_size::BODY))
        .border_1()
        .when(on, |el| {
            el.bg(theme.accent_muted)
                .border_color(theme.accent)
                .text_color(theme.text)
        })
        .when(!on && enabled, |el| {
            el.border_color(theme.border_subtle)
                .text_color(theme.text_muted)
                .cursor_pointer()
                .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
        })
        .when(!enabled, |el| {
            el.border_color(theme.border_subtle)
                .text_color(theme.text_faint.opacity(0.5))
        })
        .tooltip(rustyedit_ui::tip(tip.to_string(), &theme))
        .child(label.to_string())
}

/// A colour square; `None` is "the zine's own" (drawn as a slashed square).
fn swatch(
    theme: &Theme,
    id: SharedString,
    rgb: Option<[u8; 3]>,
    selected: bool,
    tip: &str,
) -> gpui::Stateful<gpui::Div> {
    let theme = *theme;
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(sp(20.0))
        .rounded_md()
        .cursor_pointer()
        .border_2()
        .border_color(if selected { theme.accent } else { theme.border })
        .when_some(rgb, |el, c| el.bg(rgb_colour(c)))
        .when(rgb.is_none(), |el| {
            el.bg(theme.surface_elevated)
                .text_size(sp(rustyedit_ui::type_size::MICRO))
                .text_color(theme.text_muted)
                .child("A")
        })
        .hover(|el| el.border_color(theme.accent.opacity(0.8)))
        .tooltip(rustyedit_ui::tip(tip.to_string(), &theme))
}

/// A caption over some controls.
fn labelled(theme: &Theme, label: &str, control: gpui::AnyElement) -> gpui::AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_size(sp(rustyedit_ui::type_size::CAPTION))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.text_faint)
                .child(label.to_uppercase()),
        )
        .child(control)
        .into_any_element()
}

