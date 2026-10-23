//! The tabs of the Library's right column. Each icon on the rail is a tab:
//!
//! - **Info**: the histogram, an overview of the photo, and every piece of
//!   metadata the file carries (camera, lens, exposure, dates, GPS…);
//! - **Details**: the editable descriptive fields (title, caption, creator,
//!   copyright, keywords);
//! - **Adjust**: quick cull (flag, rating, label) and quick develop sliders;
//! - **Keywords**: the keyword panel.

use gpui::{div, prelude::*, AnyElement, ClickEvent, Context};
use rustyedit_core::{Adjustment, Asset, Flag};
use rustyedit_ui::{sp, Icon};

use crate::workspace::Workspace;

impl Workspace {
    fn no_photo_note(&self) -> Vec<AnyElement> {
        padded(vec![div()
            .pt_3()
            .text_xs()
            .text_color(self.theme.text_faint)
            .child("No photo selected")
            .into_any_element()])
    }

    /// A small heading with an icon.
    fn tab_heading(&self, icon: Icon, text: &'static str) -> AnyElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .pt_3()
            .pb_1()
            .child(rustyedit_ui::icon(icon, 12.0, theme.accent))
            .child(
                div()
                    .text_size(sp(10.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.text_muted)
                    .child(text.to_uppercase()),
            )
            .into_any_element()
    }

    /// One "label ... value" line of the overview. The value stays on one
    /// line and is cut with an ellipsis when it doesn't fit; the whole text
    /// is in the tooltip.
    fn fact(&self, label: &'static str, value: String) -> AnyElement {
        let theme = self.theme;
        div()
            .id(gpui::SharedString::from(format!("overview-fact-{label}")))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_3()
            .w_full()
            .min_w(sp(0.0))
            .overflow_hidden()
            .text_size(sp(11.0))
            .tooltip(rustyedit_ui::tip(format!("{label}: {value}"), &theme))
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(theme.text_faint)
                    .child(label),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(sp(0.0))
                    .truncate()
                    .text_right()
                    .text_color(theme.text)
                    .child(value),
            )
            .into_any_element()
    }

    // --- Info -------------------------------------------------------------------

    pub(crate) fn info_tab_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(asset) = self.catalog.selected_asset() else {
            return self.no_photo_note();
        };

        // The histogram is pinned above the tab by the dock; everything here
        // scrolls.
        let mut out: Vec<AnyElement> = Vec::new();

        // Overview: what the photo is and what has been done to it.
        out.push(self.tab_heading(Icon::Info, "Overview"));
        let mut facts: Vec<AnyElement> = vec![div()
            .w_full()
            .min_w(sp(0.0))
            .overflow_hidden()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w(sp(0.0))
                    .truncate()
                    .text_size(sp(13.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child(asset.file_name()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .px_1p5()
                    .text_size(sp(9.5))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.text_muted)
                    .bg(theme.surface_elevated)
                    .child(asset.kind()),
            )
            .into_any_element()];
        if let Some(copy) = &asset.copy_name {
            facts.push(self.fact("Virtual copy", copy.clone()));
        }
        if !asset.meta.camera.is_empty() {
            facts.push(self.fact("Camera", asset.meta.camera.clone()));
        }
        if !asset.meta.lens.is_empty() {
            facts.push(self.fact("Lens", asset.meta.lens.clone()));
        }
        if asset.width > 0 && asset.height > 0 {
            facts.push(self.fact(
                "Size",
                format!(
                    "{} \u{00D7} {} \u{00B7} {:.1} MP",
                    asset.width,
                    asset.height,
                    asset.width as f64 * asset.height as f64 / 1e6
                ),
            ));
        }
        facts.push(self.fact("Rating", rating_text(asset)));
        facts.push(self.fact("Flag", flag_text(asset.flag).to_string()));
        if asset.color_label != rustyedit_core::ColorLabel::None {
            facts.push(self.fact("Label", asset.color_label.label().to_string()));
        }
        facts.push(self.fact("Edits", edit_summary(asset)));
        if let Some(preset) = asset
            .applied_preset
            .as_ref()
            .filter(|_| asset.develop.is_edited())
        {
            facts.push(self.fact("Preset", preset.name.clone()));
        }
        if !asset.keywords.is_empty() {
            facts.push(self.fact("Keywords", asset.keywords.join(", ")));
        }
        if let Some(folder) = asset.path.parent() {
            facts.push(self.fact("Folder", folder.display().to_string()));
        }
        out.push(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .children(facts)
                .into_any_element(),
        );

        // Actions: rotate, remove, and — when the photo has a position —
        // its place on a map.
        let mut actions: Vec<AnyElement> = vec![self.file_actions_row(cx)];
        if let Some((lat, lon)) = self.selected_gps() {
            actions.push(
                rustyedit_ui::chrome_button(
                    "info-open-map",
                    "Open in Maps",
                    false,
                    &theme,
                    move |_: &ClickEvent, _w: &mut gpui::Window, _cx: &mut gpui::App| {
                        let _ = std::process::Command::new("open")
                            .arg(format!("https://maps.apple.com/?ll={lat:.6},{lon:.6}"))
                            .spawn();
                    },
                )
                .into_any_element(),
            );
        }
        out.push(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .pt_2()
                .children(actions)
                .into_any_element(),
        );

        // Everything the file says about itself.
        out.push(self.tab_heading(Icon::Search, "Metadata"));
        out.extend(self.info_body(self.catalog.selected, cx));
        out.push(div().h(sp(12.0)).into_any_element());
        padded(out)
    }

    /// The selected photo's GPS position as (latitude, longitude), signed.
    fn selected_gps(&self) -> Option<(f64, f64)> {
        let asset = self.catalog.selected_asset()?;
        let metadata = self.metadata_cache.get(&asset.id)?;
        let group = metadata.groups.iter().find(|g| g.title == "Location")?;
        let (_, text) = group.rows.iter().find(|(label, _)| *label == "GPS")?;
        // "47.12345° N, 8.54321° E"
        let mut parts = text.split(',').map(str::trim);
        let parse = |part: &str| -> Option<f64> {
            let value: f64 = part.split('\u{00B0}').next()?.trim().parse().ok()?;
            let south_or_west = part
                .rsplit(' ')
                .next()
                .is_some_and(|h| h.starts_with('S') || h.starts_with('W'));
            Some(if south_or_west { -value } else { value })
        };
        Some((parse(parts.next()?)?, parse(parts.next()?)?))
    }

    // --- Details ----------------------------------------------------------------

    pub(crate) fn details_tab_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if self.catalog.selected_asset().is_none() {
            return self.no_photo_note();
        }
        let mut out = vec![self.tab_heading(Icon::Edit, "Title, caption & rights")];
        out.extend(self.metadata_sections(cx));
        padded(out)
    }

    // --- Adjust -----------------------------------------------------------------

    pub(crate) fn adjust_tab_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let Some(asset) = self.catalog.selected_asset() else {
            return self.no_photo_note();
        };
        let mut out = vec![
            self.tab_heading(Icon::Check, "Cull"),
            self.quick_cull_section(asset, cx),
            self.tab_heading(Icon::Adjust, "Quick develop"),
        ];
        out.extend(self.adjustment_sliders(&Adjustment::BASIC, cx));
        out.extend(self.adjustment_sliders(&Adjustment::COLOR, cx));
        out.push(
            div()
                .flex()
                .flex_row()
                .gap_1()
                .pt_2()
                .child(rustyedit_ui::chrome_button(
                    "adjust-open-develop",
                    "Open in Develop",
                    false,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, _w, cx| this.enter_develop(cx)),
                ))
                .into_any_element(),
        );
        out.push(div().h(sp(12.0)).into_any_element());
        padded(out)
    }
}

/// The tabs' content as one column with side padding: the dock's scroll
/// area has none, since its own sections bring their own.
fn padded(children: Vec<AnyElement>) -> Vec<AnyElement> {
    vec![div()
        .flex()
        .flex_col()
        .w_full()
        .min_w(sp(0.0))
        .px_3()
        .children(children)
        .into_any_element()]
}

fn rating_text(asset: &Asset) -> String {
    match asset.rating.value() {
        0 => "Unrated".to_string(),
        n => "\u{2605}".repeat(n as usize),
    }
}

fn flag_text(flag: Flag) -> &'static str {
    match flag {
        Flag::Picked => "Picked",
        Flag::Rejected => "Rejected",
        Flag::Unflagged => "Unflagged",
    }
}

fn edit_summary(asset: &Asset) -> String {
    if !asset.develop.is_edited() {
        return "Original".to_string();
    }
    let n = asset.history.len();
    if n == 0 {
        "Edited".to_string()
    } else {
        format!(
            "Edited \u{00B7} {n} change{}",
            if n == 1 { "" } else { "s" }
        )
    }
}
