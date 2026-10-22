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
use rustyedit_processing::DevelopSource;
use rustyedit_core::{
    settings::StartupView, Catalog, ColorLabel, ExportOptions, Flag, Histogram, RailTool, Settings,
};
use rustyedit_io::ThumbnailCache;
use rustyedit_ui::Theme;

use crate::actions::{
    CycleFilter, CycleLayout, FlagPick, FlagReject, FlagUnflag, GoDevelop, GoExport, GoLibrary,
    ImportFolder, LabelBlue, LabelGreen, LabelRed, LabelYellow, NextImage, OpenSettings, PrevImage,
    Rating0, Rating1, Rating2, Rating3, Rating4, Rating5, ToggleZoom,
};
use crate::develop::{DevelopUi, LivePreview};
use crate::file_tree::FileTree;
use crate::histogram_panel::HistogramUi;
use crate::selection::CullOp;

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
    SmartCollection,
    NewCatalog,
    /// Live: every keystroke updates the library's text search.
    Search,
    Title,
    Caption,
    Creator,
    Copyright,
    Keyword,
    /// Renaming the original file of the selected photo.
    Rename,
    /// Naming a new Develop preset from the current look.
    PresetName,
    /// Renaming a saved preset.
    PresetRename,
    /// Fujifilm recipe modal text fields.
    RecipeName,
    RecipeIso,
    RecipeExposure,
    /// Naming a new snapshot of the selected photo.
    SnapshotName,
}

/// What the Library module's center area shows: the selected photo large
/// (Lightroom's "Loupe") or the full thumbnail grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryView {
    Loupe,
    Grid,
    /// Two photos side by side.
    Compare,
    /// Several photos at once.
    Survey,
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
    /// Message to show once the next catalog scan finishes (e.g. "Restored…").
    pub(crate) post_scan_message: Option<SharedString>,
    pub(crate) is_importing: bool,
    // Multi-selection, undo, compare and loupe zoom (see their modules).
    pub(crate) bottom_menu: Option<crate::size_menu::BottomMenu>,
    // Right-click menu, and the state its entries use (see `context_menu.rs`).
    pub(crate) context_menu: Option<crate::context_menu::ContextMenu>,
    /// The photo whose ⓘ popup is open, and where it was clicked.
    pub(crate) card_info: Option<(usize, gpui::Point<gpui::Pixels>)>,
    pub(crate) edit_clipboard: Option<rustyedit_core::DevelopSettings>,
    /// The photo shown now and the one before it (asset ids), for
    /// "Previous" in Develop.
    pub(crate) current_photo: Option<u64>,
    pub(crate) previous_photo: Option<u64>,
    pub(crate) pending_album_photos: Vec<PathBuf>,
    /// Distinct cameras, lenses, years… for the Filters panel.
    pub(crate) facets: crate::filters_panel::Facets,
    /// Unspent Ctrl+wheel travel (see `wheel_zoom.rs`).
    pub(crate) wheel_accum: f32,
    pub(crate) selection: HashSet<usize>,
    pub(crate) selection_anchor: Option<usize>,
    pub(crate) undo_stack: Vec<crate::undo::UndoEntry>,
    pub(crate) redo_stack: Vec<crate::undo::UndoEntry>,
    pub(crate) compare_pair: (usize, usize),
    pub(crate) compare_active_left: bool,
    pub(crate) loupe_zoom: Option<f32>,
    pub(crate) loupe_scroll: ScrollHandle,
    pub(crate) loupe_center_pending: Cell<bool>,
    pub(crate) loupe_drag: Option<(gpui::Point<gpui::Pixels>, gpui::Point<gpui::Pixels>)>,
    /// The loupe's photo area, recorded every frame (for its fit scale).
    pub(crate) loupe_bounds: Rc<Cell<gpui::Bounds<gpui::Pixels>>>,
    /// The display's pixel ratio, recorded every frame.
    pub(crate) pixel_ratio: Cell<f32>,
    pub(crate) loupe_full_paths: HashMap<u64, PathBuf>,
    pub(crate) loupe_full_loading: HashSet<u64>,
    // Albums and smart collections (see `albums.rs`).
    pub(crate) active_album: Option<u64>,
    pub(crate) active_smart: Option<u64>,
    /// Duplicate/similar groups, computed on demand (see `duplicates.rs`).
    pub(crate) duplicate_groups: Option<Vec<rustyedit_core::DuplicateGroup>>,
    pub(crate) duplicates_loading: bool,
    // The active catalog and the others available (see `catalogs.rs`).
    pub(crate) catalog_def: rustyedit_core::CatalogDef,
    pub(crate) catalog_list: Vec<rustyedit_core::CatalogDef>,
    pub(crate) catalog_menu_open: bool,
    /// The text field currently being typed into, if any.
    pub(crate) text_draft: Option<(TextTarget, String)>,
    /// Asset index the rename draft applies to (the selection may move while
    /// the filename is being typed).
    pub(crate) rename_target: Option<usize>,
    /// Set while a rescan should report added/removed/missing counts.
    pub(crate) pending_sync_report: bool,
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
    pub(crate) ba: crate::before_after::BaState,
    pub(crate) export_run: Option<crate::export::ExportRun>,
    /// Photos (by id) the user took out of the export queue.
    pub(crate) export_excluded: HashSet<u64>,
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
    pub(crate) export_panel_scroll: ScrollHandle,
    pub(crate) settings_scroll: ScrollHandle,
    /// Filmstrip scroll position in content pixels (see `filmstrip.rs`).
    pub(crate) filmstrip_scroll_x: f64,
    pub(crate) filmstrip_viewport: Rc<Cell<f32>>,
    pub(crate) grid_columns_cache: Rc<Cell<usize>>,
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
    /// The selected photo as the live preview renders it (preview-sized)
    /// and in full, for the zoomed-in detail view and exports.
    pub(crate) source_image: Option<DevelopSource>,
    pub(crate) full_source: Option<DevelopSource>,
    pub(crate) source_image_for: Option<u64>,
    pub(crate) live_preview: LivePreview,
    pub(crate) develop_ui: DevelopUi,
    pub(crate) histogram_ui: HistogramUi,
    pub(crate) preview_epoch: u64,
}

impl Workspace {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let settings = rustyedit_io::load_settings();
        let develop_ui = DevelopUi::new(&settings);
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
        crate::display_profile::refresh();
        let mut this = Self {
            focus_handle: cx.focus_handle(),
            theme: Theme::from_appearance(&settings.appearance),
            mode: Mode::Library,
            catalog: Catalog::default(),
            thumbnail_cache: None,
            status_message: Some("Import a folder to get started.".into()),
            post_scan_message: None,
            is_importing: false,
            bottom_menu: None,
            context_menu: None,
            card_info: None,
            edit_clipboard: None,
            current_photo: None,
            previous_photo: None,
            pending_album_photos: Vec::new(),
            facets: Default::default(),
            wheel_accum: 0.0,
            selection: HashSet::new(),
            selection_anchor: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            compare_pair: (0, 0),
            compare_active_left: true,
            loupe_zoom: None,
            loupe_scroll: ScrollHandle::new(),
            loupe_center_pending: Cell::new(false),
            loupe_drag: None,
            loupe_bounds: Rc::new(Cell::new(gpui::Bounds::default())),
            pixel_ratio: Cell::new(1.0),
            loupe_full_paths: HashMap::new(),
            loupe_full_loading: HashSet::new(),
            active_album: None,
            active_smart: None,
            duplicate_groups: None,
            duplicates_loading: false,
            catalog_def,
            catalog_list,
            catalog_menu_open: false,
            text_draft: None,
            rename_target: None,
            pending_sync_report: false,
            resizing: None,
            resize_anchor: (0.0, 0.0),
            info_visible: false,
            info_collapsed: HashSet::new(),
            metadata_cache: HashMap::new(),
            metadata_loading: HashSet::new(),
            export_options: ExportOptions::default(),
            ba: Default::default(),
            export_run: None,
            export_excluded: HashSet::new(),
            export_message: None,
            left_rail_tool: None,
            right_rail_tool: None,
            import_progress: None,
            grid_scroll: UniformListScrollHandle::new(),
            left_dock_scroll: ScrollHandle::new(),
            right_dock_scroll: ScrollHandle::new(),
            develop_panel_scroll: ScrollHandle::new(),
            export_panel_scroll: ScrollHandle::new(),
            settings_scroll: ScrollHandle::new(),
            filmstrip_scroll_x: 0.0,
            filmstrip_viewport: Rc::new(Cell::new(0.0)),
            grid_columns_cache: Rc::new(Cell::new(1)),
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
            full_source: None,
            source_image_for: None,
            live_preview: LivePreview::default(),
            develop_ui,
            histogram_ui: HistogramUi::default(),
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
        self.clear_selection();
        if let Some(ix) = self.catalog.select_relative(1) {
            self.selection_anchor = Some(ix);
            self.on_selection_changed(cx);
        }
    }

    pub fn prev_image(&mut self, _: &PrevImage, _window: &mut Window, cx: &mut Context<Self>) {
        self.clear_selection();
        if let Some(ix) = self.catalog.select_relative(-1) {
            self.selection_anchor = Some(ix);
            self.on_selection_changed(cx);
        }
    }

    pub fn select_asset(&mut self, index: usize, cx: &mut Context<Self>) {
        self.clear_selection();
        self.catalog.select(index);
        self.on_selection_changed(cx);
    }

    pub(crate) fn on_selection_changed(&mut self, cx: &mut Context<Self>) {
        self.remember_previous_photo();
        self.sync_compare_with_selection();
        // A new photo starts at fit-to-window.
        if self.loupe_zoom.is_some() {
            self.loupe_zoom = None;
        }
        self.preview_epoch += 1;
        // The previous asset's rendered preview is now meaningless for the
        // newly selected asset; drop it immediately so the viewer falls
        // back to the (correct) source image while the new preview is
        // generated, instead of briefly showing the old photo.
        self.reset_develop_preview();
        if self.mode == Mode::Develop || self.ba_active() {
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
        // Escape first closes the recipe modal, then a context menu.
        if self.develop_ui.preset_ui.recipe.is_some() {
            self.close_recipe_modal(cx);
            return;
        }
        // Then an open context menu.
        if self.context_menu.take().is_some() || self.card_info.take().is_some() {
            cx.notify();
            return;
        }
        // Then puts down a Develop photo tool (Heal, Clone, gradients).
        if self.mode == Mode::Develop && self.put_down_photo_tool(cx) {
            return;
        }
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
        if self.info_visible {
            self.request_metadata(cx);
        }
        cx.notify();
    }

    pub fn toggle_zoom(&mut self, _: &ToggleZoom, _window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == Mode::Library && self.library_view == LibraryView::Loupe {
            self.toggle_loupe_zoom(cx);
            return;
        }
        self.toggle_develop_zoom(cx);
    }

    // --- Flags & ratings -----------------------------------------------------

    pub fn flag_pick(&mut self, _: &FlagPick, _window: &mut Window, cx: &mut Context<Self>) {
        self.apply_cull(CullOp::Flag(Flag::Picked), cx);
    }

    pub fn flag_reject(&mut self, _: &FlagReject, _window: &mut Window, cx: &mut Context<Self>) {
        self.apply_cull(CullOp::Flag(Flag::Rejected), cx);
    }

    pub fn flag_unflag(&mut self, _: &FlagUnflag, _window: &mut Window, cx: &mut Context<Self>) {
        self.apply_cull(CullOp::Flag(Flag::Unflagged), cx);
    }

    pub fn set_rating_value(&mut self, value: u8, cx: &mut Context<Self>) {
        self.apply_cull(CullOp::Rating(value), cx);
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
        self.apply_cull(CullOp::Label(label), cx);
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
            {
                let selected = self.settings_open;
                div()
                    .id("settings")
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(gpui::px(24.0))
                    .px_2()
                    .rounded_md()
                    .text_xl()
                    .cursor_pointer()
                    .when(selected, |el| {
                        el.bg(theme.accent_muted).text_color(theme.text)
                    })
                    .when(!selected, |el| {
                        el.text_color(theme.text_muted)
                            .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
                    })
                    .child("\u{2699}")
                    .tooltip(rustyedit_ui::tip("Settings (\u{2318},)", &theme))
                    .on_click(cx.listener(Self::click_settings))
                    .into_any_element()
            },
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
        // The Library has the filter dropdown in its bottom bar.
        if self.mode != Mode::Library {
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
        }

        if let Some(zoom) = self.render_footer_zoom(cx) {
            right.push(rustyedit_ui::status_divider(&theme).into_any_element());
            right.push(zoom);
        }

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
        // Replaced Develop preview frames are no longer drawn; free their
        // GPU memory before this frame paints.
        self.live_preview.release_retired(window);
        self.pixel_ratio.set(window.scale_factor());
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
            .on_key_down(cx.listener(Self::develop_key_down))
            .on_key_up(cx.listener(Self::develop_key_up))
            .track_focus(&focus_handle)
            .on_action(cx.listener(Self::go_export))
            .on_action(cx.listener(Self::toggle_info))
            .on_action(cx.listener(Self::cycle_background))
            .on_action(cx.listener(Self::focus_search))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::view_compare))
            .on_action(cx.listener(Self::view_survey))
            .on_action(cx.listener(Self::stack_selected))
            .on_action(cx.listener(Self::auto_stack))
            .on_action(cx.listener(Self::unstack_selected))
            .on_action(cx.listener(Self::toggle_stack))
            .on_action(cx.listener(Self::find_duplicates))
            .on_action(cx.listener(Self::delete_from_catalog))
            .on_action(cx.listener(Self::delete_from_disk))
            .on_action(cx.listener(Self::rename_photo))
            .on_action(cx.listener(Self::rotate_left))
            .on_action(cx.listener(Self::rotate_right))
            .on_action(cx.listener(Self::reveal_in_finder))
            .on_action(cx.listener(Self::sync_folders_action))
            .on_action(cx.listener(Self::restore_removed_photos))
            .on_action(cx.listener(Self::backup_catalog_action))
            .on_action(cx.listener(Self::restore_catalog_action))
            .on_action(cx.listener(Self::optimize_catalog_action))
            .on_action(cx.listener(Self::clear_cache_action))
            .on_action(cx.listener(Self::next_image))
            .on_action(cx.listener(Self::prev_image))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::select_page_up))
            .on_action(cx.listener(Self::select_page_down))
            .on_action(cx.listener(Self::extend_left))
            .on_action(cx.listener(Self::extend_right))
            .on_action(cx.listener(Self::extend_up))
            .on_action(cx.listener(Self::extend_down))
            .on_action(cx.listener(Self::extend_first))
            .on_action(cx.listener(Self::extend_last))
            .on_action(cx.listener(Self::cycle_grid_style))
            .on_action(cx.listener(Self::grid_larger))
            .on_action(cx.listener(Self::grid_smaller))
            .on_action(cx.listener(Self::tool_spot_removal))
            .on_action(cx.listener(Self::tool_linear_gradient))
            .on_action(cx.listener(Self::tool_radial_gradient))
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
            .on_action(cx.listener(Self::toggle_before_after))
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
                Mode::Export => self.render_export(window, cx),
            })
            .child(self.render_status_bar(cx))
            .child(self.wheel_zoom_hook(cx))
            .when(
                self.bottom_menu.is_some() && self.mode == Mode::Library,
                |el| el.child(self.render_bottom_menu(cx)),
            )
            .when(self.card_info.is_some(), |el| {
                el.child(self.render_card_info(window, cx))
            })
            .when(self.context_menu.is_some(), |el| {
                el.child(self.render_context_menu(window, cx))
            })
            .when(self.catalog_menu_open, |el| {
                el.child(self.render_catalog_menu(cx))
            })
            .when(self.settings_open, |el| {
                el.child(self.render_settings_modal(cx))
            })
            .when(self.develop_ui.preset_ui.recipe.is_some(), |el| {
                el.child(self.render_recipe_modal(cx))
            })
    }
}
