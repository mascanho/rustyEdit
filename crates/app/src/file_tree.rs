//! The Navigator's folder tree: browse the disk, open a folder as the
//! catalog, or jump straight to a photo.
//!
//! Folders are listed lazily, one level at a time on a background thread,
//! the first time they're expanded, and the tree is flattened into rows
//! rendered with a `uniform_list`, so folders with tens of thousands of
//! photos stay as fast as small ones.

use rustyedit_ui::sp;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use gpui::{
    div, linear_color_stop, linear_gradient, prelude::*, px, uniform_list, AnyElement, ClickEvent,
    Context, SharedString, UniformListScrollHandle,
};
use rustyedit_core::settings::FolderTreeSettings;
use rustyedit_ui::Icon;
use rustyedit_io::FsEntry;
use rustyedit_ui::Theme;

use crate::workspace::Workspace;

const ROW_HEIGHT: f32 = 22.0;
const INDENT: f32 = 12.0;

/// What's known about one folder's contents.
pub(crate) enum Listing {
    Loading,
    Loaded(Arc<Vec<FsEntry>>),
    Failed(SharedString),
}

pub(crate) struct FileTree {
    roots: Vec<(SharedString, PathBuf)>,
    expanded: HashSet<PathBuf>,
    listings: HashMap<PathBuf, Listing>,
    scroll: UniformListScrollHandle,
    /// Flattened visible rows, rebuilt only when something changes.
    rows: Rc<Vec<TreeRow>>,
    rows_dirty: bool,
    rows_built_for: Option<FolderTreeSettings>,
    /// A folder to scroll into view once the rows next rebuild.
    pending_reveal: Option<PathBuf>,
}

impl FileTree {
    pub(crate) fn new() -> Self {
        Self {
            roots: Vec::new(),
            expanded: HashSet::new(),
            listings: HashMap::new(),
            scroll: UniformListScrollHandle::new(),
            rows: Rc::new(Vec::new()),
            rows_dirty: true,
            rows_built_for: None,
            pending_reveal: None,
        }
    }

    fn load_roots(&mut self) {
        self.roots = rustyedit_io::browse_roots()
            .into_iter()
            .map(|(label, path)| (label.into(), path))
            .collect();
        self.rows_dirty = true;
    }

    fn rebuild_rows(&mut self, prefs: &FolderTreeSettings) {
        let mut rows = Vec::new();
        for (label, path) in &self.roots {
            rows.push(TreeRow {
                depth: 0,
                path: path.clone(),
                name: label.clone(),
                kind: RowKind::Folder {
                    expanded: self.expanded.contains(path),
                    is_root: true,
                },
            });
            if self.expanded.contains(path) {
                self.push_children(path, 1, prefs, &mut rows);
            }
        }

        if let Some(target) = self.pending_reveal.take() {
            if let Some(ix) = rows.iter().position(|row| row.path == target) {
                self.scroll.scroll_to_item(ix, gpui::ScrollStrategy::Center);
            }
        }
        self.rows = Rc::new(rows);
        self.rows_dirty = false;
        self.rows_built_for = Some(prefs.clone());
    }

    fn push_children(
        &self,
        dir: &Path,
        depth: usize,
        prefs: &FolderTreeSettings,
        rows: &mut Vec<TreeRow>,
    ) {
        let message = |text: SharedString| TreeRow {
            depth,
            path: dir.to_path_buf(),
            name: text,
            kind: RowKind::Message,
        };

        let entries = match self.listings.get(dir) {
            None | Some(Listing::Loading) => return rows.push(message("Loading…".into())),
            Some(Listing::Failed(err)) => return rows.push(message(err.clone())),
            Some(Listing::Loaded(entries)) => entries,
        };

        let before = rows.len();
        for entry in entries.iter() {
            if (entry.is_hidden && !prefs.show_hidden) || (!entry.is_dir && !prefs.show_photos) {
                continue;
            }
            let expanded = entry.is_dir && self.expanded.contains(&entry.path);
            rows.push(TreeRow {
                depth,
                path: entry.path.clone(),
                name: entry.name.clone().into(),
                kind: if entry.is_dir {
                    RowKind::Folder {
                        expanded,
                        is_root: false,
                    }
                } else {
                    RowKind::Photo
                },
            });
            if expanded {
                self.push_children(&entry.path, depth + 1, prefs, rows);
            }
        }
        if rows.len() == before {
            rows.push(message("Empty".into()));
        }
    }
}

#[derive(Clone)]
struct TreeRow {
    depth: usize,
    path: PathBuf,
    name: SharedString,
    kind: RowKind,
}

#[derive(Clone, Copy, PartialEq)]
enum RowKind {
    Folder {
        expanded: bool,
        is_root: bool,
    },
    Photo,
    /// A placeholder line under a folder ("Loading…", "Empty", an error).
    Message,
}

impl Workspace {
    /// Brings the cached rows up to date. Call before rendering the tree.
    pub(crate) fn prepare_file_tree(&mut self) {
        if self.file_tree.roots.is_empty() {
            self.file_tree.load_roots();
        }
        let prefs = &self.settings.folders;
        if self.file_tree.rows_dirty || self.file_tree.rows_built_for.as_ref() != Some(prefs) {
            self.file_tree.rebuild_rows(&prefs.clone());
        }
    }

    fn toggle_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.file_tree.expanded.remove(&path) {
            self.expand_folder(path, cx);
        }
        self.file_tree.rows_dirty = true;
        cx.notify();
    }

    fn expand_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.request_listing(path.clone(), cx);
        self.file_tree.expanded.insert(path);
        self.file_tree.rows_dirty = true;
    }

    /// Lists `dir` on a background thread (network or sleeping drives can
    /// take seconds), unless it's already listed or being listed.
    fn request_listing(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        if self.file_tree.listings.contains_key(&dir) {
            return;
        }
        self.file_tree
            .listings
            .insert(dir.clone(), Listing::Loading);

        let list_dir = dir.clone();
        let task = cx
            .background_executor()
            .spawn(async move { rustyedit_io::list_directory(&list_dir) });

        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                // A refresh may have dropped this request in the meantime.
                if !matches!(this.file_tree.listings.get(&dir), Some(Listing::Loading)) {
                    return;
                }
                let listing = match result {
                    Ok(entries) => Listing::Loaded(Arc::new(entries)),
                    Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                        Listing::Failed("No permission to read this folder".into())
                    }
                    Err(err) => Listing::Failed(format!("Can't read: {err}").into()),
                };
                this.file_tree.listings.insert(dir, listing);
                this.file_tree.rows_dirty = true;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Re-reads drives and every expanded folder, picking up changes made
    /// on disk since they were first listed.
    /// Folds every folder back up to the top-level places.
    fn collapse_file_tree(&mut self, cx: &mut Context<Self>) {
        self.file_tree.expanded.clear();
        self.file_tree.rows_dirty = true;
        cx.notify();
    }

    fn refresh_file_tree(&mut self, cx: &mut Context<Self>) {
        self.file_tree.listings.clear();
        self.file_tree.load_roots();
        for dir in self.file_tree.expanded.clone() {
            self.request_listing(dir, cx);
        }
        cx.notify();
    }

    /// Expands the tree down to `folder` and scrolls it into view, so the
    /// currently open catalog folder is always easy to find.
    pub(crate) fn reveal_folder_in_tree(&mut self, folder: &Path, cx: &mut Context<Self>) {
        if self.file_tree.roots.is_empty() {
            self.file_tree.load_roots();
        }
        // The most specific root containing the folder, e.g. "Pictures"
        // rather than "Home" for ~/Pictures/2024.
        let Some(root) = self
            .file_tree
            .roots
            .iter()
            .map(|(_, path)| path)
            .filter(|root| folder.starts_with(root))
            .max_by_key(|root| root.components().count())
            .cloned()
        else {
            return;
        };

        let mut dirs: Vec<&Path> = folder
            .ancestors()
            .take_while(|dir| dir.starts_with(&root))
            .collect();
        dirs.reverse();
        // Expand every ancestor, but not the folder itself, to keep the
        // tree compact.
        dirs.pop();
        for dir in dirs {
            self.expand_folder(dir.to_path_buf(), cx);
        }
        self.file_tree.pending_reveal = Some(folder.to_path_buf());
        self.file_tree.rows_dirty = true;
        cx.notify();
    }

    fn activate_tree_row(&mut self, row: TreeRow, click_count: usize, cx: &mut Context<Self>) {
        match row.kind {
            RowKind::Folder { .. } if click_count >= 2 => {
                // The first click of the double-click already toggled it;
                // leave it open while the folder imports.
                self.expand_folder(row.path.clone(), cx);
                self.import_folder(row.path, None, cx);
            }
            RowKind::Folder { .. } => self.toggle_folder(row.path, cx),
            RowKind::Photo => self.open_photo(row.path, cx),
            RowKind::Message => {}
        }
    }

    /// Selects `path` if it's in the open catalog; otherwise opens its
    /// folder as the catalog and selects it once the import finishes.
    fn open_photo(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Some(ix) = self.catalog.assets.iter().position(|a| a.path == path) {
            self.select_asset(ix, cx);
        } else if let Some(folder) = path.parent() {
            self.import_folder(folder.to_path_buf(), Some(path.clone()), cx);
        }
    }

    /// The "Folders" section at the top of the Navigator.
    pub(crate) fn render_folder_tree(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let rows = self.file_tree.rows.clone();
        let height = self.settings.folders.tree_height as f32;

        let list = uniform_list(
            "folder-tree",
            rows.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                let entity = cx.entity();
                let theme = this.theme;
                let selected_photo = this.catalog.selected_asset().map(|a| a.path.clone());

                range
                    .map(|ix| {
                        let row = rows[ix].clone();
                        let is_catalog = this.catalog.has_folder(&row.path)
                            && matches!(row.kind, RowKind::Folder { .. });
                        // Folders in the catalog show how many photos they
                        // hold and can be removed from it.
                        let photos = is_catalog.then(|| {
                            this.catalog
                                .assets
                                .iter()
                                .filter(|a| a.path.starts_with(&row.path))
                                .count()
                        });
                        let remove_entity = entity.clone();
                        let remove_path = row.path.clone();
                        let remove: Option<RemoveFolder> = is_catalog.then(|| {
                            Box::new(move |_: &ClickEvent, _w: &mut gpui::Window, cx: &mut gpui::App| {
                                cx.stop_propagation();
                                let path = remove_path.clone();
                                remove_entity.update(cx, |this, cx| {
                                    this.remove_folder_from_catalog(path, cx)
                                });
                            }) as RemoveFolder
                        });
                        let is_selected = selected_photo.as_ref() == Some(&row.path)
                            && row.kind == RowKind::Photo;
                        let entity = entity.clone();
                        tree_row(ix, &row, is_catalog, is_selected, photos, remove, &theme)
                            .on_click(move |event: &ClickEvent, _window, cx| {
                                let row = row.clone();
                                let clicks = event.click_count();
                                entity
                                    .update(cx, |this, cx| this.activate_tree_row(row, clicks, cx));
                            })
                            .into_any_element()
                    })
                    .collect()
            }),
        )
        .track_scroll(self.file_tree.scroll.clone())
        .h(px(height));

        let tool = |id: &'static str, icon: Icon, tip: &'static str| {
            div()
                .id(id)
                .flex()
                .items_center()
                .justify_center()
                .size(sp(22.0))
                .cursor_pointer()
                .hover(|el| el.bg(theme.surface_elevated))
                .tooltip(rustyedit_ui::tip(tip, &theme))
                .child(rustyedit_ui::icon(icon, 13.0, theme.text_muted))
        };
        let toolbar = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .gap_0p5()
            .child(
                tool("folder-tree-add", Icon::Plus, "Add a folder to the catalog")
                    .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.start_import(cx))),
            )
            .child(
                tool("folder-tree-collapse", Icon::Minus, "Collapse all")
                    .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| {
                        this.collapse_file_tree(cx)
                    })),
            )
            .child(
                tool("folder-tree-refresh", Icon::Reset, "Refresh")
                    .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.refresh_file_tree(cx))),
            );

        self.disclosure(
            "folders",
            "Folders",
            vec![
                toolbar.into_any_element(),
                div()
                    .relative()
                    .rounded_md()
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .py_1()
                    .child(list)
                    .child(rustyedit_ui::vertical_scrollbar(
                        "folder-tree-scrollbar",
                        self.file_tree.scroll.0.borrow().base_handle.clone(),
                        &theme,
                    ))
                    .child(self.resize_grip(crate::resize::ResizeTarget::FolderTree, cx))
                    .into_any_element(),
            ],
            cx,
        )
    }
}

/// Size of a folder row's expand chevron.
const CHEVRON_SIZE: f32 = 12.0;

/// Click handler for a catalog folder's remove button.
type RemoveFolder = Box<dyn Fn(&ClickEvent, &mut gpui::Window, &mut gpui::App)>;

fn tree_row(
    ix: usize,
    row: &TreeRow,
    is_catalog: bool,
    is_selected: bool,
    photos: Option<usize>,
    remove: Option<RemoveFolder>,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let theme = *theme;
    let indent = px(6.0 + row.depth as f32 * INDENT);

    let chevron = match row.kind {
        RowKind::Folder { expanded, .. } => Some(if expanded {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        }),
        _ => None,
    };

    let icon: AnyElement = match row.kind {
        RowKind::Folder { is_root, .. } => div()
            .flex_shrink_0()
            .w(sp(12.0))
            .h(sp(9.0))
            .rounded(sp(2.0))
            .bg(if is_catalog {
                theme.accent
            } else if is_root {
                theme.text_muted.opacity(0.8)
            } else {
                theme.star.opacity(0.75)
            })
            .into_any_element(),
        // A tiny "photo": sky fading into warm ground.
        RowKind::Photo => div()
            .flex_shrink_0()
            .size(sp(10.0))
            .rounded(sp(2.0))
            .bg(linear_gradient(
                180.0,
                linear_color_stop(theme.label_blue.opacity(0.8), 0.0),
                linear_color_stop(theme.film_edge_print.opacity(0.8), 1.0),
            ))
            .into_any_element(),
        RowKind::Message => div().into_any_element(),
    };

    let text_color = match row.kind {
        RowKind::Message => theme.text_faint,
        _ if is_catalog => theme.accent,
        RowKind::Folder { is_root: true, .. } => theme.text,
        _ => theme.text_muted,
    };

    div()
        .id(("folder-tree-row", ix))
        .group("folder-tree-row")
        .tooltip(rustyedit_ui::tip(row.path.display().to_string(), &theme))
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        // Full width, so highlights run edge to edge however short the name.
        .w_full()
        .h(px(ROW_HEIGHT))
        .pl(indent)
        .pr_2()
        .text_xs()
        .text_color(text_color)
        .when(row.kind == RowKind::Message, |el| el.italic())
        .when(
            matches!(row.kind, RowKind::Folder { is_root: true, .. }) || is_catalog,
            |el| el.font_weight(gpui::FontWeight::SEMIBOLD),
        )
        .when(is_catalog && !is_selected, |el| {
            el.bg(theme.accent_muted.opacity(0.35))
        })
        .when(is_selected, |el| {
            el.bg(theme.accent_muted).text_color(theme.text)
        })
        .when(row.kind != RowKind::Message, |el| {
            el.cursor_pointer()
                .hover(|el| el.bg(theme.surface_elevated).text_color(theme.text))
        })
        .child(
            div()
                .flex_shrink_0()
                .w(px(CHEVRON_SIZE))
                .children(chevron.map(|chevron| rustyedit_ui::icon(chevron, CHEVRON_SIZE, theme.text_muted))),
        )
        .child(icon)
        .child(
            div()
                .flex_1()
                .min_w(sp(0.0))
                .truncate()
                .child(row.name.clone()),
        )
        .children(photos.map(|count| {
            div()
                .flex_shrink_0()
                .text_size(sp(10.0))
                .text_color(theme.text_faint)
                .child(count.to_string())
        }))
        .children(remove.map(|remove| {
            div()
                .id(("folder-tree-remove", ix))
                .flex_shrink_0()
                .px_1()
                .text_size(sp(10.0))
                .text_color(theme.text_faint)
                .invisible()
                .group_hover("folder-tree-row", |s| s.visible())
                .hover(|el| el.text_color(theme.danger))
                .tooltip(rustyedit_ui::tip(
                    "Remove from catalog (files are not deleted)",
                    &theme,
                ))
                .child(rustyedit_ui::icon(Icon::Close, 10.0, theme.text_muted))
                .on_click(remove)
        }))
}
