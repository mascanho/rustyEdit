use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use gpui::{
    div, prelude::*, App, ClickEvent, Context, FocusHandle, Focusable, ScrollHandle, SharedString,
    Subscription, UniformListScrollHandle, Window,
};
use image::DynamicImage;
use rustyedit_core::{
    settings::StartupView, Catalog, ColorLabel, ExportOptions, Flag, Histogram, RailTool, Rating,
    Settings,
};
use rustyedit_io::ThumbnailCache;
use rustyedit_ui::Theme;

use crate::actions::{
    CycleFilter, CycleLayout, FlagPick, FlagReject, FlagUnflag, GoDevelop, GoExport, GoLibrary,
    ImportFolder, LabelBlue, LabelGreen, LabelRed, LabelYellow, NextImage, OpenSettings, PrevImage,
    Rating0, Rating1, Rating2, Rating3, Rating4, Rating5, ToggleZoom,
};
use crate::file_tree::FileTree;

/// How long after the last edit the catalog is written to disk.
const CATALOG_SAVE_DEBOUNCE: Duration = Duration::from_millis(750);

/// Which top-level module the workspace is showing, mirroring Lightroom's
/// Library (cull/organize) and Develop (edit) modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Library,
    Develop,
    Export,
}

/// Which text field a typed draft belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextTarget {
    Album,
    NewCatalog,
}

/// What the Library module's center area shows: the selected photo large
/// (Lightroom's "Loupe") or the full thumbnail grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryView {
    Loupe,
    Grid,
}

/// Root view of the application: owns the catalog, the current mode, and
/// all of the background-job bookkeeping for thumbnails and live develop
/// previews. Rendering itself is split across `workspace.rs` (chrome),
/// `library.rs` (culling grid) and `develop.rs` (editor) via separate
/// `impl Workspace` blocks.
pub struct Workspace {
    pub(crate) focus_handle: FocusHandle,
    pub(crate) theme: Theme,
    pub(crate) mode: Mode,
    pub(crate) catalog: Catalog,
    pub(crate) thumbnail_cache: Option<Arc<ThumbnailCache>>,
    pub(crate) status_message: Option<SharedString>,
    pub(crate) is_importing: bool,
    // Albums (see `albums.rs`).
    pub(crate) active_album: Option<u64>,
    // The active catalog and the others available (see `catalogs.rs`).
    pub(crate) catalog_def: rustyedit_core::CatalogDef,
    pub(crate) catalog_list: Vec<rustyedit_core::CatalogDef>,
    pub(crate) catalog_menu_open: bool,
    /// The text field currently being typed into, if any.
    pub(crate) text_draft: Option<(TextTarget, String)>,
    /// The dock or filmstrip edge currently being dragged (see `resize.rs`).
    pub(crate) resizing: Option<crate::resize::ResizeTarget>,
    /// Pointer y and starting size when a relative drag began.
    pub(crate) resize_anchor: (f32, f32),
    // Loupe info overlay (see `info_overlay.rs`).
    pub(crate) info_visible: bool,
    /// Titles of the info groups the user has collapsed.
    pub(crate) info_collapsed: HashSet<&'static str>,
    pub(crate) metadata_cache: HashMap<u64, Arc<rustyedit_io::Metadata>>,
    pub(crate) metadata_loading: HashSet<u64>,
    // Export module state (see `export.rs`).
    pub(crate) export_options: ExportOptions,
    pub(crate) export_progress: Option<(Arc<std::sync::atomic::AtomicUsize>, usize)>,
    pub(crate) export_message: Option<SharedString>,
    /// Active tool on each side's icon rail (see `rail.rs`).
    pub(crate) left_rail_tool: Option<RailTool>,
    pub(crate) right_rail_tool: Option<RailTool>,
    /// Thumbnail progress of the running import: (done counter, total).
    pub(crate) import_progress: Option<(Arc<std::sync::atomic::AtomicUsize>, usize)>,
    pub(crate) grid_scroll: UniformListScrollHandle,
    // Scroll positions of the side panels and Settings content, which
    // also drive their scrollbars.
    pub(crate) left_dock_scroll: ScrollHandle,
    pub(crate) right_dock_scroll: ScrollHandle,
    pub(crate) develop_panel_scroll: ScrollHandle,
    pub(crate) settings_scroll: ScrollHandle,
    /// Filmstrip scroll position in content pixels (see `filmstrip.rs`).
    pub(crate) filmstrip_scroll_x: f64,
    pub(crate) filmstrip_viewport: Rc<Cell<f32>>,
    pub(crate) grid_columns_cache: Rc<Cell<usize>>,
    pub(crate) zoom_fit: bool,
    pub(crate) library_view: LibraryView,
    pub(crate) left_panel_collapsed: bool,
    pub(crate) right_panel_collapsed: bool,
    pub(crate) filmstrip_collapsed: bool,
    pub(crate) file_tree: FileTree,
    /// Keys of side-panel disclosure sections the user has collapsed.
    pub(crate) collapsed_sections: HashSet<&'static str>,

    // Catalog persistence: edits mark the catalog dirty and a debounced
    // background task writes it out (see `mark_catalog_dirty`).
    pub(crate) catalog_dirty: bool,
    pub(crate) catalog_save_scheduled: bool,
    pub(crate) catalog_save_in_flight: bool,
    _quit_subscription: Subscription,

    // Preferences and the Settings window (see `settings_view.rs`).
    pub(crate) settings: Settings,
    pub(crate) settings_open: bool,
    pub(crate) settings_page: usize,
    pub(crate) settings_focus: FocusHandle,

    // Histogram panel (see `histogram.rs`): per-photo histograms of the
    // original keyed by asset id, plus the live one for the edited preview.
    pub(crate) histogram_cache: HashMap<u64, Arc<Histogram>>,
    pub(crate) histogram_loading: Option<u64>,

    // Loupe-size previews generated for RAW files, keyed by asset id.
    pub(crate) loupe_paths: HashMap<u64, PathBuf>,
    pub(crate) loupe_loading: HashSet<u64>,
    pub(crate) develop_histogram: Option<(u64, Arc<Histogram>)>,

    // Develop-mode live preview pipeline state.
    pub(crate) source_image: Option<Arc<DynamicImage>>,
    pub(crate) source_image_for: Option<u64>,
    pub(crate) preview_path: Option<PathBuf>,
    pub(crate) previous_preview_path: Option<PathBuf>,
    pub(crate) preview_busy: bool,
    pub(crate) preview_pending: bool,
    pub(crate) preview_epoch: u64,
}

impl Workspace {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let settings = rustyedit_io::load_settings();
        let (catalog_def, catalog_list) = Self::init_catalogs();
        // Flush any not-yet-written edits synchronously on quit, since the
        // debounced background save may not have fired yet (or may be cut
        // off mid-write when the process exits).
        let quit_subscription = cx.on_app_quit(|this: &mut Self, _cx| {
            let unsaved = this.catalog_dirty || this.catalog_save_in_flight;
            if unsaved && this.catalog.folder.is_some() {
                let _ = rustyedit_io::save_catalog(&this.catalog);
            }
            async {}
        });
        // Bring the GPU device up in the background so the first Develop
        // slider drag doesn't pay for adapter discovery and shader compile.
        cx.background_executor()
            .spawn(async { rustyedit_processing::gpu_adapter_name() })
            .detach();
        let mut this = Self {
            focus_handle: cx.focus_handle(),
            theme: Theme::dark(),
            mode: Mode::Library,
            catalog: Catalog::default(),
            thumbnail_cache: None,
            status_message: Some("Import a folder to get started.".into()),
            is_importing: false,
            active_album: None,
            catalog_def,
            catalog_list,
            catalog_menu_open: false,
            text_draft: None,
            resizing: None,
            resize_anchor: (0.0, 0.0),
            info_visible: false,
            info_collapsed: HashSet::new(),
            metadata_cache: HashMap::new(),
            metadata_loading: HashSet::new(),
            export_options: ExportOptions::default(),
            export_progress: None,
            export_message: None,
            left_rail_tool: None,
            right_rail_tool: None,
            import_progress: None,
            grid_scroll: UniformListScrollHandle::new(),
            left_dock_scroll: ScrollHandle::new(),
            right_dock_scroll: ScrollHandle::new(),
            develop_panel_scroll: ScrollHandle::new(),
            settings_scroll: ScrollHandle::new(),
            filmstrip_scroll_x: 0.0,
            filmstrip_viewport: Rc::new(Cell::new(0.0)),
            grid_columns_cache: Rc::new(Cell::new(1)),
            zoom_fit: true,
            library_view: match settings.library.startup_view {
                StartupView::Loupe => LibraryView::Loupe,
                StartupView::Grid => LibraryView::Grid,
            },
            left_panel_collapsed: !settings.library.show_left_panel,
            right_panel_collapsed: !settings.library.show_right_panel,
            filmstrip_collapsed: !settings.filmstrip.show_on_startup,
            file_tree: FileTree::new(),
            collapsed_sections: HashSet::new(),
            catalog_dirty: false,
            catalog_save_scheduled: false,
            catalog_save_in_flight: false,
            _quit_subscription: quit_subscription,
            settings,
            settings_open: false,
            settings_page: 0,
            settings_focus: cx.focus_handle(),
            histogram_cache: HashMap::new(),
            loupe_paths: HashMap::new(),
            loupe_loading: HashSet::new(),
            histogram_loading: None,
            develop_histogram: None,
            source_image: None,
            source_image_for: None,
            preview_path: None,
            previous_preview_path: None,
            preview_busy: false,
            preview_pending: false,
            preview_epoch: 0,
        };
        // Pick up where the last session left off.
        if !this.catalog_def.folders.is_empty() {
            this.open_catalog(None, cx);
        }
        this
    }

    /// A side-panel section that expands/collapses on click. `key` must be
    /// unique per section; it doubles as the element id and the key the
    /// open/closed state is remembered under.
    pub(crate) fn disclosure(
        &self,
        key: &'static str,
        title: &'static str,
        children: Vec<gpui::AnyElement>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        rustyedit_ui::disclosure_section(
            key,
            &self.theme,
            title,
            !self.collapsed_sections.contains(key),
            cx.listener(move |this, _: &ClickEvent, _window, cx| {
                if !this.collapsed_sections.remove(key) {
                    this.collapsed_sections.insert(key);
                }
                cx.notify();
            }),
            children,
        )
        .into_any_element()
    }

    /// Columns the grid laid out on its last render (it derives them from
    /// the card size and the space available), for keeping scroll in sync.
    pub(crate) fn grid_columns(&self) -> usize {
        self.grid_columns_cache.get().max(1)
    }

    /// Records that the catalog has unsaved edits and schedules a save.
    /// Bursts of edits (holding an arrow key while flagging, dragging a
    /// develop slider) coalesce into one write, and the write itself happens
    /// off the UI thread, so large catalogs never stall interaction.
    pub(crate) fn mark_catalog_dirty(&mut self, cx: &mut Context<Self>) {
        self.catalog_dirty = true;
        if self.catalog_save_scheduled {
            return;
        }
        self.catalog_save_scheduled = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CATALOG_SAVE_DEBOUNCE).await;
            this.update(cx, |this, cx| this.flush_catalog(cx)).ok();
        })
        .detach();
    }

    /// Synchronously writes unsaved edits before the catalog is replaced
    /// (opening another folder), since the debounced save would otherwise
    /// write the *new* catalog and silently drop them. Callers must make
    /// sure no background save is in flight.
    pub(crate) fn save_outgoing_catalog(&mut self) {
        if self.catalog_dirty && self.catalog.folder.is_some() {
            if let Err(err) = rustyedit_io::save_catalog(&self.catalog) {
                self.status_message = Some(format!("Couldn't save catalog: {err}").into());
            }
        }
        self.catalog_dirty = false;
    }

    fn flush_catalog(&mut self, cx: &mut Context<Self>) {
        self.catalog_save_scheduled = false;
        // Only one write at a time, so an older snapshot can never land on
        // disk after a newer one; the in-flight save re-checks when done.
        if !self.catalog_dirty || self.catalog_save_in_flight || self.catalog.folder.is_none() {
            return;
        }
        self.catalog_dirty = false;
        self.catalog_save_in_flight = true;

        let snapshot = self.catalog.clone();
        let save = cx
            .background_executor()
            .spawn(async move { rustyedit_io::save_catalog(&snapshot) });

        cx.spawn(async move |this, cx| {
            let result = save.await;
            this.update(cx, |this, cx| {
                this.catalog_save_in_flight = false;
                if let Err(err) = result {
                    this.status_message = Some(format!("Couldn't save catalog: {err}").into());
                    cx.notify();
                }
                if this.catalog_dirty {
                    this.mark_catalog_dirty(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    // --- Selection & mode ---------------------------------------------------

    pub fn next_image(&mut self, _: &NextImage, _window: &mut Window, cx: &mut Context<Self>) {
        if self.catalog.select_relative(1).is_some() {
            self.on_selection_changed(cx);
        }
    }

    pub fn prev_image(&mut self, _: &PrevImage, _window: &mut Window, cx: &mut Context<Self>) {
        if self.catalog.select_relative(-1).is_some() {
            self.on_selection_changed(cx);
        }
    }

    pub fn select_asset(&mut self, index: usize, cx: &mut Context<Self>) {
        self.catalog.select(index);
        self.on_selection_changed(cx);
    }

    pub(crate) fn on_selection_changed(&mut self, cx: &mut Context<Self>) {
        self.preview_epoch += 1;
        self.source_image = None;
        self.source_image_for = None;
        // The previous asset's rendered preview is now meaningless for the
        // newly selected asset; drop it immediately so the viewer falls
        // back to the (correct) thumbnail/source image while the new
        // preview is generated, instead of briefly showing the old photo.
        if let Some(stale) = self.preview_path.take() {
            rustyedit_io::cleanup_live_preview(&stale);
        }
        if let Some(stale) = self.previous_preview_path.take() {
            rustyedit_io::cleanup_live_preview(&stale);
        }
        if self.mode == Mode::Develop {
            self.load_source_image(cx);
        }
        if let Some(pos) = self.catalog.visible_position(self.catalog.selected) {
            let row = pos / self.grid_columns();
            self.grid_scroll
                .scroll_to_item(row, gpui::ScrollStrategy::Center);
        }
        self.reveal_in_filmstrip(self.catalog.selected);
        self.request_histogram(cx);
        self.request_loupe(cx);
        if self.info_visible {
            self.request_metadata(cx);
        }
        cx.notify();
    }

    pub fn go_library(&mut self, _: &GoLibrary, _window: &mut Window, cx: &mut Context<Self>) {
        self.mode = Mode::Library;
        cx.notify();
    }

    pub fn go_export(&mut self, _: &GoExport, _window: &mut Window, cx: &mut Context<Self>) {
        self.mode = Mode::Export;
        cx.notify();
    }

    pub fn go_develop(&mut self, _: &GoDevelop, _window: &mut Window, cx: &mut Context<Self>) {
        self.enter_develop(cx);
    }

    pub fn enter_develop(&mut self, cx: &mut Context<Self>) {
        if self.catalog.selected_asset().is_none() {
            return;
        }
        self.mode = Mode::Develop;
        if self.source_image_for != self.catalog.selected_asset().map(|a| a.id) {
            self.load_source_image(cx);
        }
        cx.notify();
    }

    pub fn toggle_zoom(&mut self, _: &ToggleZoom, _window: &mut Window, cx: &mut Context<Self>) {
        self.zoom_fit = !self.zoom_fit;
        cx.notify();
    }

    // --- Flags & ratings -----------------------------------------------------

    pub fn flag_pick(&mut self, _: &FlagPick, _window: &mut Window, cx: &mut Context<Self>) {
        let ix = self.catalog.selected;
        self.catalog.set_flag(ix, Flag::Picked);
        self.mark_catalog_dirty(cx);
        cx.notify();
    }

    pub fn flag_reject(&mut self, _: &FlagReject, _window: &mut Window, cx: &mut Context<Self>) {
        let ix = self.catalog.selected;
        self.catalog.set_flag(ix, Flag::Rejected);
        self.mark_catalog_dirty(cx);
        cx.notify();
    }

    pub fn flag_unflag(&mut self, _: &FlagUnflag, _window: &mut Window, cx: &mut Context<Self>) {
        let ix = self.catalog.selected;
        self.catalog.set_flag(ix, Flag::Unflagged);
        self.mark_catalog_dirty(cx);
        cx.notify();
    }

    pub fn set_rating_value(&mut self, value: u8, cx: &mut Context<Self>) {
        let ix = self.catalog.selected;
        self.catalog.set_rating(ix, Rating::new(value));
        self.mark_catalog_dirty(cx);
        cx.notify();
    }

    pub fn rating_0(&mut self, _: &Rating0, _w: &mut Window, cx: &mut Context<Self>) {
        self.set_rating_value(0, cx);
    }
    pub fn rating_1(&mut self, _: &Rating1, _w: &mut Window, cx: &mut Context<Self>) {
        self.set_rating_value(1, cx);
    }
    pub fn rating_2(&mut self, _: &Rating2, _w: &mut Window, cx: &mut Context<Self>) {
        self.set_rating_value(2, cx);
    }
    pub fn rating_3(&mut self, _: &Rating3, _w: &mut Window, cx: &mut Context<Self>) {
        self.set_rating_value(3, cx);
    }
    pub fn rating_4(&mut self, _: &Rating4, _w: &mut Window, cx: &mut Context<Self>) {
        self.set_rating_value(4, cx);
    }
    pub fn rating_5(&mut self, _: &Rating5, _w: &mut Window, cx: &mut Context<Self>) {
        self.set_rating_value(5, cx);
    }

    pub fn set_color_label_value(&mut self, label: ColorLabel, cx: &mut Context<Self>) {
        let ix = self.catalog.selected;
        self.catalog.set_color_label(ix, label);
        self.mark_catalog_dirty(cx);
        cx.notify();
    }

    pub fn label_red(&mut self, _: &LabelRed, _w: &mut Window, cx: &mut Context<Self>) {
        self.set_color_label_value(ColorLabel::Red, cx);
    }
    pub fn label_yellow(&mut self, _: &LabelYellow, _w: &mut Window, cx: &mut Context<Self>) {
        self.set_color_label_value(ColorLabel::Yellow, cx);
    }
    pub fn label_green(&mut self, _: &LabelGreen, _w: &mut Window, cx: &mut Context<Self>) {
        self.set_color_label_value(ColorLabel::Green, cx);
    }
    pub fn label_blue(&mut self, _: &LabelBlue, _w: &mut Window, cx: &mut Context<Self>) {
        self.set_color_label_value(ColorLabel::Blue, cx);
    }

    pub fn cycle_filter(&mut self, _: &CycleFilter, _window: &mut Window, cx: &mut Context<Self>) {
        self.catalog.cycle_filter();
        // Cycling the filter can silently move `selected` (if the
        // previously selected asset no longer matches), so route through
        // the same path a manual selection would take to keep the develop
        // preview/filmstrip/grid scroll in sync with whatever is now
        // actually selected.
        self.on_selection_changed(cx);
    }

    /// Switches to the next layout preset (⌘⇧L), saving it like any other
    /// setting and announcing the new layout in the status bar.
    pub fn cycle_layout(&mut self, _: &CycleLayout, _window: &mut Window, cx: &mut Context<Self>) {
        let next = self.settings.layout.preset.next();
        self.update_settings(|s| s.layout.preset = next, cx);
        // Make sure the newly arranged panels are actually visible.
        self.left_panel_collapsed = false;
        self.right_panel_collapsed = false;
        self.status_message = Some(format!("Layout: {}", next.label()).into());
        cx.notify();
    }

    pub fn open_settings_action(
        &mut self,
        _: &OpenSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_settings(window, cx);
    }

    pub fn import_folder_action(
        &mut self,
        _: &ImportFolder,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_import(cx);
    }

    // --- Chrome click handlers (title bar / status bar buttons) -------------

    fn click_go_library(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.go_library(&GoLibrary, window, cx);
    }

    fn click_go_export(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.go_export(&GoExport, window, cx);
    }

    fn click_go_develop(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.enter_develop(cx);
    }

    fn click_import(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.start_import(cx);
    }

    fn click_settings(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings(window, cx);
    }

    pub(crate) fn click_cycle_filter(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cycle_filter(&CycleFilter, window, cx);
    }

    // --- Shared chrome --------------------------------------------------------

    pub(crate) fn render_title_bar(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;

        let left = vec![
            rustyedit_ui::chrome_button(
                "catalog-switcher",
                format!("{} \u{25BE}", self.catalog_def.name),
                self.catalog_menu_open,
                &theme,
                cx.listener(|this, _: &ClickEvent, _w, cx| {
                    this.catalog_menu_open = !this.catalog_menu_open;
                    cx.notify();
                }),
            )
            .into_any_element(),
            rustyedit_ui::chrome_button(
                "import",
                "Import…",
                false,
                &theme,
                cx.listener(Self::click_import),
            )
            .into_any_element(),
        ];

        let right = vec![
            rustyedit_ui::chrome_button(
                "mode-library",
                "Library",
                self.mode == Mode::Library,
                &theme,
                cx.listener(Self::click_go_library),
            )
            .into_any_element(),
            rustyedit_ui::chrome_button(
                "mode-develop",
                "Develop",
                self.mode == Mode::Develop,
                &theme,
                cx.listener(Self::click_go_develop),
            )
            .into_any_element(),
            rustyedit_ui::chrome_button(
                "mode-export",
                "Export",
                self.mode == Mode::Export,
                &theme,
                cx.listener(Self::click_go_export),
            )
            .into_any_element(),
            rustyedit_ui::chrome_icon_button(
                "settings",
                "\u{2699}",
                Some("Settings (\u{2318},)".into()),
                self.settings_open,
                &theme,
                cx.listener(Self::click_settings),
            )
            .into_any_element(),
        ];

        rustyedit_ui::title_bar(&theme, "RustyEdit", left, right).into_any_element()
    }

    pub(crate) fn render_status_bar(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let counts = self.catalog.counts();

        let mut left: Vec<gpui::AnyElement> = Vec::new();
        if let Some(message) = &self.status_message {
            left.push(div().child(message.clone()).into_any_element());
        } else {
            left.push(
                div()
                    .child(format!(
                        "{} \u{00B7} {} folder{}",
                        self.catalog_def.name,
                        self.catalog_def.folders.len(),
                        if self.catalog_def.folders.len() == 1 {
                            ""
                        } else {
                            "s"
                        }
                    ))
                    .into_any_element(),
            );
        }
        left.push(rustyedit_ui::status_divider(&theme).into_any_element());
        left.push(
            div()
                .child(format!(
                    "{} photos · {} picked · {} rejected",
                    counts.total, counts.picked, counts.rejected
                ))
                .into_any_element(),
        );

        let mut right: Vec<gpui::AnyElement> = Vec::new();
        right.push(
            rustyedit_ui::chrome_button(
                "filter",
                format!("Filter: {}", self.catalog.filter().label()),
                false,
                &theme,
                cx.listener(Self::click_cycle_filter),
            )
            .into_any_element(),
        );

        if let Some(asset) = self.catalog.selected_asset() {
            let entity = cx.entity();
            right.push(rustyedit_ui::status_divider(&theme).into_any_element());
            right.push(
                rustyedit_ui::rating_stars(
                    "status-rating",
                    asset.rating,
                    &theme,
                    12.0,
                    Some(move |value: u8, _window: &mut Window, cx: &mut App| {
                        entity.update(cx, |this, cx| this.set_rating_value(value, cx));
                    }),
                )
                .into_any_element(),
            );
        }

        rustyedit_ui::status_bar(&theme, left, right).into_any_element()
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let focus_handle = self.focus_handle.clone();

        div()
            .id("workspace-root")
            .key_context(if self.text_draft.is_some() {
                "TextEntry"
            } else {
                "Workspace"
            })
            .on_key_down(cx.listener(Self::on_text_key))
            .track_focus(&focus_handle)
            .on_action(cx.listener(Self::go_export))
            .on_action(cx.listener(Self::toggle_info))
            .on_action(cx.listener(Self::next_image))
            .on_action(cx.listener(Self::prev_image))
            .on_action(cx.listener(Self::flag_pick))
            .on_action(cx.listener(Self::flag_reject))
            .on_action(cx.listener(Self::flag_unflag))
            .on_action(cx.listener(Self::rating_0))
            .on_action(cx.listener(Self::rating_1))
            .on_action(cx.listener(Self::rating_2))
            .on_action(cx.listener(Self::rating_3))
            .on_action(cx.listener(Self::rating_4))
            .on_action(cx.listener(Self::rating_5))
            .on_action(cx.listener(Self::label_red))
            .on_action(cx.listener(Self::label_yellow))
            .on_action(cx.listener(Self::label_green))
            .on_action(cx.listener(Self::label_blue))
            .on_action(cx.listener(Self::go_library))
            .on_action(cx.listener(Self::go_develop))
            .on_action(cx.listener(Self::cycle_filter))
            .on_action(cx.listener(Self::toggle_zoom))
            .on_action(cx.listener(Self::import_folder_action))
            .on_action(cx.listener(Self::open_settings_action))
            .on_action(cx.listener(Self::cycle_layout))
            .on_mouse_move(cx.listener(Self::on_resize_move))
            .on_mouse_up(gpui::MouseButton::Left, cx.listener(Self::on_resize_up))
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.background)
            .text_color(theme.text)
            .child(self.render_title_bar(cx))
            .child(match self.mode {
                Mode::Library => self.render_library(window, cx),
                Mode::Develop => self.render_develop(window, cx),
                Mode::Export => self.render_export(cx),
            })
            .child(self.render_status_bar(cx))
            .when(self.catalog_menu_open, |el| {
                el.child(self.render_catalog_menu(cx))
            })
            .when(self.settings_open, |el| {
                el.child(self.render_settings_modal(cx))
            })
    }
}
