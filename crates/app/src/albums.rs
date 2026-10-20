//! Albums: hand-picked sets of photos (the Collections rail tool). The
//! model is `rustyedit_core::Albums`; this file is the panel, the inline
//! "new album" text entry, and applying an album as the catalog's view.

use std::collections::HashSet;

use gpui::{
    div, prelude::*, px, AnyElement, ClickEvent, Context, KeyDownEvent, SharedString, Window,
};

use crate::workspace::{TextTarget, Workspace};

impl Workspace {
    pub(crate) fn albums_sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let selected_photo = self.catalog.selected_asset().map(|a| a.path.clone());

        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .pt_3()
            .pb_1()
            .child(
                div()
                    .text_size(px(10.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.accent)
                    .child("ALBUMS"),
            )
            .child(rustyedit_ui::glyph_button(
                "album-new",
                "+",
                "New album",
                theme.text_muted,
                true,
                &theme,
                cx.listener(|this, _: &ClickEvent, window, cx| this.begin_album_draft(window, cx)),
            ))
            .into_any_element();
        let mut sections = vec![header];

        if let Some(draft) = self.draft_for(TextTarget::Album) {
            sections.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .pb_2()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .h(px(26.0))
                            .px_2()
                            .rounded_md()
                            .bg(theme.background)
                            .border_1()
                            .border_color(theme.accent)
                            .text_sm()
                            .text_color(theme.text)
                            .child(draft.to_string())
                            .child(div().w(px(1.0)).h(px(14.0)).ml(px(1.0)).bg(theme.accent)),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_faint)
                            .child(
                                "Name the album \u{00B7} Enter to create \u{00B7} Esc to cancel",
                            ),
                    )
                    .into_any_element(),
            );
        }

        sections.push(self.album_row(
            "album-all".into(),
            "All photos".to_string(),
            None,
            self.active_album.is_none() && self.active_smart.is_none(),
            None,
            cx,
        ));

        for album in &self.catalog_def.albums.items {
            let id = album.id;
            let membership = selected_photo.as_ref().map(|p| album.contains(p));
            sections.push(self.album_row(
                format!("album-{id}").into(),
                album.name.clone(),
                Some(album.photos.len()),
                self.active_album == Some(id),
                Some((id, membership)),
                cx,
            ));
        }

        if self.catalog_def.albums.items.is_empty() && self.draft_for(TextTarget::Album).is_none() {
            sections.push(
                div()
                    .py_2()
                    .text_xs()
                    .text_color(theme.text_faint)
                    .child("No albums yet. Press + to make one, then add photos to it from here.")
                    .into_any_element(),
            );
        }

        // --- Smart collections ------------------------------------------------
        sections.push(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .pt_4()
                .pb_1()
                .child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(theme.accent)
                        .child("SMART COLLECTIONS"),
                )
                .child(rustyedit_ui::glyph_button(
                    "smart-new",
                    "+",
                    "Save the current filters as a smart collection",
                    theme.text_muted,
                    true,
                    &theme,
                    cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.begin_smart_draft(window, cx)
                    }),
                ))
                .into_any_element(),
        );

        if let Some(draft) = self.draft_for(TextTarget::SmartCollection) {
            sections.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .pb_2()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .h(px(26.0))
                            .px_2()
                            .rounded_md()
                            .bg(theme.background)
                            .border_1()
                            .border_color(theme.accent)
                            .text_sm()
                            .text_color(theme.text)
                            .child(draft.to_string())
                            .child(div().w(px(1.0)).h(px(14.0)).ml(px(1.0)).bg(theme.accent)),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_faint)
                            .child("Name it · it saves the current filters and rating · Enter to create · Esc to cancel"),
                    )
                    .into_any_element(),
            );
        }

        for collection in &self.catalog_def.smart_collections.items {
            let id = collection.id;
            let name = collection.name.clone();
            let count = self.smart_match_count(collection);
            sections.push(self.smart_row(id, name, count, self.active_smart == Some(id), cx));
        }

        if self.catalog_def.smart_collections.items.is_empty()
            && self.draft_for(TextTarget::SmartCollection).is_none()
        {
            sections.push(
                div()
                    .pb_2()
                    .text_xs()
                    .text_color(theme.text_faint)
                    .child("Set filters, then press + to save them as a live collection.")
                    .into_any_element(),
            );
        }
        sections
    }

    fn smart_row(
        &self,
        id: u64,
        name: String,
        count: usize,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        div()
            .id(SharedString::from(format!("smart-{id}")))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(28.0))
            .px_2()
            .rounded_md()
            .cursor_pointer()
            .text_sm()
            .when(active, |el| {
                el.bg(theme.accent_muted).text_color(theme.text)
            })
            .when(!active, |el| {
                el.text_color(theme.text_muted)
                    .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
            })
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(11.0))
                    .text_color(theme.accent)
                    .child("\u{2699}"),
            )
            .child(div().flex_1().min_w(px(0.0)).overflow_hidden().child(name))
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme.text_faint)
                    .child(count.to_string()),
            )
            .child(rustyedit_ui::glyph_button(
                SharedString::from(format!("smart-delete-{id}")),
                "\u{2715}",
                "Delete smart collection",
                theme.text_muted,
                true,
                &theme,
                cx.listener(move |this, _: &ClickEvent, _w, cx| this.delete_smart(id, cx)),
            ))
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| this.view_smart(id, cx)))
            .into_any_element()
    }

    /// One row: click to view the album; the buttons add / remove the
    /// selected photo and delete the album. `album` is `None` for the
    /// "All photos" row.
    fn album_row(
        &self,
        id: SharedString,
        name: String,
        count: Option<usize>,
        active: bool,
        album: Option<(u64, Option<bool>)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let album_id = album.map(|(id, _)| id);

        let mut actions = div().flex().flex_row().items_center().flex_shrink_0();
        if let Some((album_id, membership)) = album {
            if let Some(is_member) = membership {
                actions = actions.child(rustyedit_ui::glyph_button(
                    SharedString::from(format!("album-toggle-{album_id}")),
                    if is_member { "\u{2713}" } else { "+" },
                    if is_member {
                        "Remove selected photo"
                    } else {
                        "Add selected photo"
                    },
                    theme.pick,
                    is_member,
                    &theme,
                    cx.listener(move |this, _: &ClickEvent, _w, cx| {
                        this.toggle_in_album(album_id, cx)
                    }),
                ));
            }
            actions = actions.child(rustyedit_ui::glyph_button(
                SharedString::from(format!("album-delete-{album_id}")),
                "\u{2715}",
                "Delete album",
                theme.text_muted,
                true,
                &theme,
                cx.listener(move |this, _: &ClickEvent, _w, cx| this.delete_album(album_id, cx)),
            ));
        }

        div()
            .id(id)
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(28.0))
            .px_2()
            .rounded_md()
            .cursor_pointer()
            .text_sm()
            .when(active, |el| {
                el.bg(theme.accent_muted).text_color(theme.text)
            })
            .when(!active, |el| {
                el.text_color(theme.text_muted)
                    .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
            })
            .child(div().flex_1().min_w(px(0.0)).overflow_hidden().child(name))
            .when_some(count, |el, n| {
                el.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme.text_faint)
                        .child(n.to_string()),
                )
            })
            .child(actions)
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _w, cx| this.view_album(album_id, cx)),
            )
            .into_any_element()
    }

    /// Makes `album` (or the whole folder, for `None`) the visible set.
    pub(crate) fn view_album(&mut self, album: Option<u64>, cx: &mut Context<Self>) {
        self.active_album = album;
        self.active_smart = None;
        self.apply_album_scope();
        self.on_selection_changed(cx);
    }

    // --- Smart collections -------------------------------------------------

    pub(crate) fn begin_smart_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The name field lives in the Collections panel, so reveal it even if
        // the request came from the Filters panel.
        self.open_left_tool(rustyedit_core::RailTool::Collections, cx);
        self.begin_text_draft(TextTarget::SmartCollection, window, cx);
    }

    /// How many photos the saved recipe would show right now.
    fn smart_match_count(&self, collection: &rustyedit_core::SmartCollection) -> usize {
        self.catalog
            .assets
            .iter()
            .filter(|asset| {
                collection.filter_mode.matches(asset.flag)
                    && collection.rating.matches(asset.rating)
                    && collection.filters.matches(asset)
            })
            .count()
    }

    fn create_smart_collection(&mut self, name: &str, cx: &mut Context<Self>) {
        let filters = self.catalog.filters().clone();
        let mode = self.catalog.filter();
        let rating = self.catalog.rating_filter();
        if let Some(id) = self
            .catalog_def
            .smart_collections
            .create(name, filters, mode, rating)
        {
            self.save_catalog_def();
            self.view_smart(id, cx);
        }
    }

    fn view_smart(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(collection) = self.catalog_def.smart_collections.get(id).cloned() else {
            return;
        };
        self.active_album = None;
        self.active_smart = Some(id);
        self.catalog.set_scope(None);
        self.catalog.set_filters(collection.filters);
        self.catalog.set_rating_filter(collection.rating);
        self.catalog.set_filter(collection.filter_mode);
        self.on_selection_changed(cx);
    }

    fn delete_smart(&mut self, id: u64, cx: &mut Context<Self>) {
        self.catalog_def.smart_collections.delete(id);
        self.save_catalog_def();
        if self.active_smart == Some(id) {
            self.active_smart = None;
        }
        cx.notify();
    }

    /// Pushes the active album's photos into the catalog as its scope.
    fn apply_album_scope(&mut self) {
        let scope: Option<HashSet<_>> = self
            .active_album
            .and_then(|id| self.catalog_def.albums.get(id))
            .map(|album| album.photos.iter().cloned().collect());
        if scope.is_none() {
            self.active_album = None;
        }
        self.catalog.set_scope(scope);
    }

    fn toggle_in_album(&mut self, album: u64, cx: &mut Context<Self>) {
        let Some(photo) = self.catalog.selected_asset().map(|a| a.path.clone()) else {
            return;
        };
        self.catalog_def.albums.toggle(album, &photo);
        self.save_catalog_def();
        if self.active_album == Some(album) {
            self.apply_album_scope();
            self.on_selection_changed(cx);
        }
        cx.notify();
    }

    fn delete_album(&mut self, album: u64, cx: &mut Context<Self>) {
        self.catalog_def.albums.delete(album);
        self.save_catalog_def();
        if self.active_album == Some(album) {
            self.active_album = None;
            self.apply_album_scope();
            self.on_selection_changed(cx);
        }
        cx.notify();
    }

    // --- Inline text entry -------------------------------------------------

    fn begin_album_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_text_draft(TextTarget::Album, window, cx);
    }

    pub(crate) fn begin_text_draft(
        &mut self,
        target: TextTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.text_draft = Some((target, String::new()));
        // Keystrokes only reach the draft if the workspace has focus.
        window.focus(&self.focus_handle);
        cx.notify();
    }

    /// The text typed so far, if `target` is the field being edited.
    pub(crate) fn draft_for(&self, target: TextTarget) -> Option<&str> {
        match &self.text_draft {
            Some((t, text)) if *t == target => Some(text),
            _ => None,
        }
    }

    /// Feeds typing into whichever text field is being edited. Runs on the
    /// workspace root, whose key context switches to `TextEntry` during
    /// entry so single-letter shortcuts don't fire.
    pub(crate) fn on_text_key(
        &mut self,
        event: &KeyDownEvent,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((target, draft)) = self.text_draft.as_mut() else {
            return;
        };
        let target = *target;
        let keystroke = &event.keystroke;
        match keystroke.key.as_str() {
            "enter" => {
                let name = std::mem::take(draft);
                self.text_draft = None;
                self.commit_text(target, name, cx);
            }
            "escape" => {
                if target == TextTarget::Rename {
                    self.rename_target = None;
                }
                self.text_draft = None;
            }
            "backspace" => {
                draft.pop();
            }
            _ => {
                let m = &keystroke.modifiers;
                if m.platform || m.control {
                    return;
                }
                if let Some(text) = keystroke
                    .key_char
                    .as_ref()
                    .filter(|t| !t.chars().any(char::is_control))
                {
                    draft.push_str(text);
                }
            }
        }
        if target == TextTarget::Search {
            if let Some((_, text)) = &self.text_draft {
                let query = text.clone();
                self.set_search_query(query, cx);
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn commit_text(&mut self, target: TextTarget, text: String, cx: &mut Context<Self>) {
        match target {
            TextTarget::Album => {
                if let Some(id) = self.catalog_def.albums.create(&text) {
                    self.save_catalog_def();
                    self.view_album(Some(id), cx);
                }
            }
            TextTarget::SmartCollection => self.create_smart_collection(&text, cx),
            TextTarget::NewCatalog => self.create_catalog(&text, cx),
            TextTarget::Keyword => self.apply_keywords(vec![text], cx),
            TextTarget::Rename => self.commit_rename(text, cx),
            TextTarget::Title
            | TextTarget::Caption
            | TextTarget::Creator
            | TextTarget::Copyright => self.commit_metadata_text(target, text, cx),
            // Search applies live as you type; Enter just ends editing.
            TextTarget::Search => {}
        }
    }
}
