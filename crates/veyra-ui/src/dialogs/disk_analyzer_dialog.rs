//! Faz 20: the Disk Analyzer — an `AdwDialog` showing a folder-usage
//! breakdown, its largest files, and same-size duplicate candidates.
//!
//! The whole subtree is scanned once, in the background
//! (`veyra_filesystem::analyze_directory`, cancelled if the dialog closes
//! before the scan finishes, per Rule #13), and kept in memory as a
//! `UsageNode` tree. Drilling into a subfolder is then just re-rendering a
//! different node already in that tree — no rescan, no extra I/O — with a
//! breadcrumb trail (`path_indices`, positions into `children` from the
//! root) driving both the current view and "go back up" navigation.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use chrono::{DateTime, Local, Utc};
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use veyra_filesystem::{
    find_duplicates, format_size, AnalysisResult, DuplicateGroup, OperationControl,
    SameSizeCandidateGroup, UsageEntry, UsageNode, VeyraPath,
};

use crate::fs_async;
use crate::i18n::{t, t_fmt, t_plural};
use crate::undo::{SharedUndoStack, UndoableAction};

/// Categorical palette for the breakdown bar/list, cycled in order; the
/// eighth (last) is reserved for the aggregated "Other" segment.
const PALETTE: [(f64, f64, f64); 8] = [
    (0.208, 0.518, 0.894), // blue
    (0.200, 0.820, 0.482), // green
    (0.965, 0.827, 0.176), // yellow
    (1.000, 0.471, 0.000), // orange
    (0.878, 0.106, 0.141), // red
    (0.569, 0.255, 0.675), // purple
    (0.596, 0.416, 0.267), // brown
    (0.612, 0.612, 0.612), // gray ("Other")
];

/// Shows the Disk Analyzer for `root`, parented to `parent`. `navigate` jumps
/// the caller's file browser to a path (used by "Open in Folder" entries in
/// the Largest Files/Duplicate Files tabs). `refresh_all` re-reads every
/// open tab in both panels after a Duplicate Files cleanup actually moves
/// something to Trash; `undo_stack` records that cleanup so `Ctrl+Z` can
/// bring the trashed copies back (Rule #38/#39).
pub(crate) fn show(
    parent: &impl IsA<gtk4::Widget>,
    root: VeyraPath,
    navigate: Rc<dyn Fn(VeyraPath)>,
    refresh_all: Rc<dyn Fn()>,
    undo_stack: SharedUndoStack,
) {
    let dialog = adw::Dialog::builder()
        .title(t("disk_analyzer.title"))
        .content_width(760)
        .content_height(640)
        .build();

    let spinner = gtk4::Spinner::new();
    spinner.set_spinning(true);
    spinner.set_size_request(32, 32);
    spinner.set_halign(gtk4::Align::Center);
    spinner.set_valign(gtk4::Align::Center);
    spinner.set_vexpand(true);

    let status_label = gtk4::Label::new(Some(&t_fmt(
        "disk_analyzer.scanning",
        &[("root", &root.to_string())],
    )));
    status_label.add_css_class("dim-label");

    let loading_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    loading_box.append(&spinner);
    loading_box.append(&status_label);

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&gtk4::Label::new(Some(t("disk_analyzer.title")))));
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&loading_box));
    dialog.set_child(Some(&toolbar_view));

    let control = OperationControl::new();
    {
        let control = control.clone();
        dialog.connect_closed(move |_| control.cancel());
    }

    let scan_root = root.clone();
    let scan_control = control.clone();
    let dialog_for_result = dialog.clone();
    let toolbar_view_for_result = toolbar_view.clone();
    let header_for_result = header.clone();
    fs_async::run_blocking(
        move || veyra_filesystem::analyze_directory(&scan_root, &scan_control),
        move |result| match result {
            Ok(analysis) => {
                build_loaded_view(
                    &dialog_for_result,
                    &toolbar_view_for_result,
                    &header_for_result,
                    analysis,
                    navigate,
                    refresh_all,
                    undo_stack,
                );
            }
            Err(err) => {
                header_for_result
                    .set_title_widget(Some(&gtk4::Label::new(Some(t("disk_analyzer.title")))));
                let error_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
                error_box.set_valign(gtk4::Align::Center);
                error_box.set_vexpand(true);
                let icon = gtk4::Image::from_icon_name("dialog-error-symbolic");
                icon.set_pixel_size(48);
                let label = gtk4::Label::new(Some(&t_fmt(
                    "disk_analyzer.scan_error",
                    &[("error", &err.to_string())],
                )));
                label.set_wrap(true);
                error_box.append(&icon);
                error_box.append(&label);
                toolbar_view_for_result.set_content(Some(&error_box));
            }
        },
    );

    dialog.present(Some(parent));
}

/// A re-render callback that redraws the breadcrumbs and Breakdown page for
/// the current `path_indices`, boxed so it can be handed to click handlers
/// (row activation, breadcrumb clicks) that need to trigger it again later.
type RenderFn = Rc<dyn Fn()>;
/// Holds the current `RenderFn` so recursively-built closures (breadcrumb
/// and row click handlers built *while* rendering) can call back into it
/// without a fixed-point/`Y`-combinator dance.
type RenderSlot = Rc<RefCell<Option<RenderFn>>>;

/// Shared, immutable state kept for the lifetime of a loaded Disk Analyzer
/// dialog: the full scanned tree plus the two global (whole-tree) derived
/// lists. Drilling down never touches this again — only `path_indices`
/// (owned by the caller) changes.
struct LoadedState {
    tree: UsageNode,
    largest_files: Vec<UsageEntry>,
    same_size_candidates: Vec<SameSizeCandidateGroup>,
}

/// Replaces the loading spinner with the real tabbed view once the scan
/// finishes, and wires breadcrumb drill-down/navigation.
fn build_loaded_view(
    dialog: &adw::Dialog,
    toolbar_view: &adw::ToolbarView,
    header: &adw::HeaderBar,
    analysis: AnalysisResult,
    navigate: Rc<dyn Fn(VeyraPath)>,
    refresh_all: Rc<dyn Fn()>,
    undo_stack: SharedUndoStack,
) {
    let state = Rc::new(LoadedState {
        tree: analysis.tree,
        largest_files: analysis.largest_files,
        same_size_candidates: analysis.duplicate_candidates,
    });
    let path_indices: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));

    let breadcrumb_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
    header.set_title_widget(Some(&breadcrumb_box));

    let view_stack = adw::ViewStack::new();
    let breakdown_page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    view_stack.add_titled_with_icon(
        &breakdown_page,
        Some("breakdown"),
        "Breakdown",
        "view-grid-symbolic",
    );

    let files_page =
        build_largest_files_page(&state.largest_files, navigate.clone(), dialog.clone());
    view_stack.add_titled_with_icon(
        &files_page,
        Some("files"),
        "Largest Files",
        "document-symbolic",
    );

    let duplicates_page = build_duplicates_page(
        state.same_size_candidates.clone(),
        navigate.clone(),
        dialog.clone(),
        refresh_all,
        undo_stack,
    );
    view_stack.add_titled_with_icon(
        &duplicates_page,
        Some("duplicates"),
        t("dup.tab.title"),
        "edit-copy-symbolic",
    );

    let switcher_bar = adw::ViewSwitcherBar::new();
    switcher_bar.set_stack(Some(&view_stack));
    switcher_bar.set_reveal(true);

    toolbar_view.set_content(Some(&view_stack));
    toolbar_view.add_bottom_bar(&switcher_bar);

    let render: RenderFn = {
        let state = state.clone();
        let path_indices = path_indices.clone();
        let breakdown_page = breakdown_page.clone();
        let breadcrumb_box = breadcrumb_box.clone();
        let render_slot: RenderSlot = Rc::new(RefCell::new(None));
        let render_slot_for_closure = render_slot.clone();
        let closure: RenderFn = Rc::new(move || {
            let node = node_at(&state.tree, &path_indices.borrow());
            render_breadcrumbs(
                &breadcrumb_box,
                &state.tree,
                &path_indices,
                render_slot_for_closure.clone(),
            );
            render_breakdown(
                &breakdown_page,
                node,
                &path_indices,
                render_slot_for_closure.clone(),
            );
        });
        *render_slot.borrow_mut() = Some(closure.clone());
        closure
    };
    render();
}

/// Looks up the node reached by following `indices` (each a position into
/// the previous node's already size-sorted `children`) from `root`.
fn node_at<'a>(root: &'a UsageNode, indices: &[usize]) -> &'a UsageNode {
    let mut node = root;
    for &index in indices {
        match node.children.get(index) {
            Some(child) => node = child,
            None => break,
        }
    }
    node
}

/// Rebuilds the breadcrumb trail (root ... current) as a row of flat
/// buttons; clicking one truncates `path_indices` back to that depth and
/// re-renders.
fn render_breadcrumbs(
    breadcrumb_box: &gtk4::Box,
    tree: &UsageNode,
    path_indices: &Rc<RefCell<Vec<usize>>>,
    render: RenderSlot,
) {
    while let Some(child) = breadcrumb_box.first_child() {
        breadcrumb_box.remove(&child);
    }

    let mut node = tree;
    let mut crumbs: Vec<(String, usize)> = vec![(node.name.clone(), 0)];
    for (depth, &index) in path_indices.borrow().iter().enumerate() {
        let Some(child) = node.children.get(index) else {
            break;
        };
        node = child;
        crumbs.push((node.name.clone(), depth + 1));
    }

    let crumb_count = crumbs.len();
    for (position, (name, depth)) in crumbs.into_iter().enumerate() {
        if position > 0 {
            breadcrumb_box.append(&gtk4::Image::from_icon_name("go-next-symbolic"));
        }
        let button = gtk4::Button::builder()
            .label(name)
            .css_classes(["flat"])
            .build();
        button.set_sensitive(position + 1 != crumb_count);
        {
            let path_indices = path_indices.clone();
            let render = render.clone();
            button.connect_clicked(move |_| {
                path_indices.borrow_mut().truncate(depth);
                if let Some(render) = render.borrow().as_ref() {
                    render();
                }
            });
        }
        breadcrumb_box.append(&button);
    }
}

/// Renders the Breakdown tab's content for `node`: a proportional segmented
/// bar plus a scrollable list of its direct children (directories drill
/// down on click; files are informational only, since they're already the
/// deepest level of that branch).
fn render_breakdown(
    container: &gtk4::Box,
    node: &UsageNode,
    path_indices: &Rc<RefCell<Vec<usize>>>,
    render: RenderSlot,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    let summary = gtk4::Label::new(Some(&format!(
        "{} — {} items, {}",
        node.name,
        node.direct_file_count + node.direct_dir_count,
        format_size(node.size_bytes)
    )));
    summary.set_xalign(0.0);
    summary.set_margin_start(16);
    summary.set_margin_end(16);
    summary.set_margin_top(12);
    summary.add_css_class("title-4");
    container.append(&summary);

    if node.children.is_empty() {
        let empty = gtk4::Label::new(Some(t("empty.folder.title")));
        empty.add_css_class("dim-label");
        empty.set_margin_top(24);
        empty.set_valign(gtk4::Align::Start);
        container.append(&empty);
        return;
    }

    let segments = breakdown_segments(node);
    let segment_count = segments.len();
    let bar = gtk4::DrawingArea::new();
    bar.set_content_height(28);
    bar.set_hexpand(true);
    bar.set_margin_start(16);
    bar.set_margin_end(16);
    bar.set_margin_top(12);
    bar.set_margin_bottom(4);
    let total = node.size_bytes.max(1);
    bar.set_draw_func(move |_area, cr, width, height| {
        let width = width as f64;
        let height = height as f64;
        let radius = 6.0;
        cr.new_path();
        cr.arc(
            radius,
            radius,
            radius,
            std::f64::consts::PI,
            1.5 * std::f64::consts::PI,
        );
        cr.arc(
            width - radius,
            radius,
            radius,
            1.5 * std::f64::consts::PI,
            2.0 * std::f64::consts::PI,
        );
        cr.arc(
            width - radius,
            height - radius,
            radius,
            0.0,
            0.5 * std::f64::consts::PI,
        );
        cr.arc(
            radius,
            height - radius,
            radius,
            0.5 * std::f64::consts::PI,
            std::f64::consts::PI,
        );
        cr.close_path();
        cr.clip();

        let mut x = 0.0;
        for (index, (_, size)) in segments.iter().enumerate() {
            let fraction = *size as f64 / total as f64;
            let segment_width = fraction * width;
            let (r, g, b) = PALETTE[index.min(PALETTE.len() - 1)];
            cr.set_source_rgb(r, g, b);
            cr.rectangle(x, 0.0, segment_width, height);
            let _ = cr.fill();
            x += segment_width;
        }
    });
    container.append(&bar);

    let list = gtk4::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk4::SelectionMode::None);
    list.set_margin_start(16);
    list.set_margin_end(16);
    list.set_margin_top(8);
    list.set_margin_bottom(16);

    for (index, child) in node.children.iter().enumerate() {
        let row = adw::ActionRow::builder()
            .title(&child.name)
            .subtitle(format!(
                "{}  ·  {:.1}%",
                format_size(child.size_bytes),
                child.size_bytes as f64 / total as f64 * 100.0
            ))
            .build();
        row.set_title_lines(1);
        row.set_subtitle_lines(2);

        if index < segment_count.min(PALETTE.len()) && child.is_dir {
            row.add_prefix(&color_swatch(PALETTE[index.min(PALETTE.len() - 1)]));
        }

        let icon = gtk4::Image::from_icon_name(if child.is_dir {
            "folder-symbolic"
        } else {
            "text-x-generic-symbolic"
        });
        row.add_prefix(&icon);

        if child.is_dir {
            row.set_activatable(true);
            row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
            let path_indices = path_indices.clone();
            let render = render.clone();
            row.connect_activated(move |_| {
                path_indices.borrow_mut().push(index);
                if let Some(render) = render.borrow().as_ref() {
                    render();
                }
            });
        }

        list.append(&row);
    }
    container.append(&list);

    let scroller = gtk4::ScrolledWindow::new();
    scroller.set_vexpand(true);
    // Re-parent the already-built list under a scroller so a folder with
    // many entries doesn't blow out the dialog's fixed height.
    container.remove(&list);
    scroller.set_child(Some(&list));
    container.append(&scroller);
}

/// Groups `node`'s children into up-to-7 named segments plus an aggregated
/// "Other" segment for the rest, for both the proportional bar and the
/// per-row color swatches. Files (which have no further breakdown) are
/// folded into "Other" rather than getting their own segment, since the bar
/// exists to compare *folders*, matching the Faz 20 spec's example
/// (Videos/Projects/Downloads/Pictures/Other).
fn breakdown_segments(node: &UsageNode) -> Vec<(String, u64)> {
    let dirs: Vec<&UsageNode> = node.children.iter().filter(|c| c.is_dir).collect();
    let mut segments: Vec<(String, u64)> = Vec::new();
    let take = dirs.len().min(7);
    for dir in dirs.iter().take(take) {
        segments.push((dir.name.clone(), dir.size_bytes));
    }
    let shown: u64 = segments.iter().map(|(_, s)| s).sum();
    let other = node.size_bytes.saturating_sub(shown);
    if other > 0 {
        segments.push((t("storage.other_segment").to_string(), other));
    }
    segments
}

/// Builds the Largest Files tab: a flat, already-sorted list with an "Open
/// in Folder" action per row.
fn build_largest_files_page(
    entries: &[UsageEntry],
    navigate: Rc<dyn Fn(VeyraPath)>,
    dialog: adw::Dialog,
) -> gtk4::Widget {
    if entries.is_empty() {
        return empty_state(t("storage.no_files_found"));
    }

    let list = gtk4::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk4::SelectionMode::None);
    list.set_margin_top(12);
    list.set_margin_bottom(12);
    list.set_margin_start(16);
    list.set_margin_end(16);

    for entry in entries {
        let row = adw::ActionRow::builder()
            .title(&entry.name)
            .subtitle(format!(
                "{}  ·  {}",
                format_size(entry.size_bytes),
                entry.path
            ))
            .build();
        row.set_title_lines(1);
        row.set_subtitle_lines(2);
        row.add_prefix(&gtk4::Image::from_icon_name("text-x-generic-symbolic"));
        row.add_suffix(&open_in_folder_button(
            entry.path.clone(),
            navigate.clone(),
            dialog.clone(),
        ));
        list.append(&row);
    }

    let scroller = gtk4::ScrolledWindow::new();
    scroller.set_vexpand(true);
    scroller.set_child(Some(&list));
    scroller.upcast()
}

/// One confirmed duplicate file, paired with its modified time (fetched
/// alongside the content hash, off the GTK thread) so the tab can render
/// it and the "Keep Newest"/"Keep Oldest" helpers can compare it without
/// any further I/O.
#[derive(Clone)]
struct DupFileEntry {
    path: VeyraPath,
    modified: Option<DateTime<Utc>>,
}

/// A content-hash-confirmed duplicate group, augmented with per-file
/// modified times — the UI-side counterpart to
/// `veyra_filesystem::DuplicateGroup`.
#[derive(Clone)]
struct DuplicateGroupView {
    size_per_file: u64,
    wasted_size: u64,
    files: Vec<DupFileEntry>,
}

/// Builds the Duplicate Files tab: starts the (potentially slow) content
/// hash pass in the background immediately (Rule #11/#12), showing a
/// spinner until it lands, then an interactive view — checkbox per file,
/// "Select All Copies (Keep Newest/Oldest)" helpers, and a "Move Selected
/// to Trash" action gated behind the all-copies-protection rule and an
/// explicit confirmation (Rule #38/#39).
fn build_duplicates_page(
    candidates: Vec<SameSizeCandidateGroup>,
    navigate: Rc<dyn Fn(VeyraPath)>,
    dialog: adw::Dialog,
    refresh_all: Rc<dyn Fn()>,
    undo_stack: SharedUndoStack,
) -> gtk4::Widget {
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    container.set_vexpand(true);

    let spinner = gtk4::Spinner::new();
    spinner.set_spinning(true);
    spinner.set_size_request(32, 32);
    spinner.set_halign(gtk4::Align::Center);
    spinner.set_valign(gtk4::Align::Center);
    spinner.set_vexpand(true);
    let status_label = gtk4::Label::new(Some(t("dup.loading")));
    status_label.add_css_class("dim-label");
    let loading_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    loading_box.append(&spinner);
    loading_box.append(&status_label);
    container.append(&loading_box);

    let control = OperationControl::new();
    {
        let control = control.clone();
        dialog.connect_closed(move |_| control.cancel());
    }

    let container_for_result = container.clone();
    fs_async::run_blocking(
        move || compute_duplicate_groups(&candidates, &control),
        move |groups| {
            container_for_result.remove(&loading_box);
            if groups.is_empty() {
                container_for_result.append(&empty_state(t("dup.empty")));
                return;
            }
            let interactive = build_duplicates_interactive_view(
                groups,
                navigate,
                dialog,
                refresh_all,
                undo_stack,
            );
            container_for_result.append(&interactive);
        },
    );

    container.upcast()
}

/// Runs the Faz 42 content-hash engine over `candidates`, then fetches each
/// confirmed duplicate's modified time in the same background pass (a
/// `stat` call is itself blocking I/O, so it must never run on the GTK
/// thread — Rule #14). A file whose `stat` fails (removed mid-scan) keeps
/// its entry with `modified: None` rather than being dropped, since it's
/// still a legitimate, selectable duplicate (Rule #16).
fn compute_duplicate_groups(
    candidates: &[SameSizeCandidateGroup],
    control: &OperationControl,
) -> Vec<DuplicateGroupView> {
    find_duplicates(candidates, control)
        .into_iter()
        .map(|group: DuplicateGroup| DuplicateGroupView {
            size_per_file: group.size_per_file,
            wasted_size: group.wasted_size,
            files: group
                .files
                .into_iter()
                .map(|path| {
                    let modified = veyra_filesystem::stat(&path)
                        .ok()
                        .and_then(|item| item.metadata.modified);
                    DupFileEntry { path, modified }
                })
                .collect(),
        })
        .collect()
}

/// Shared, mutable widgets/state the Duplicate Files tab's helper buttons
/// (quick-select, clear, trash) all act on.
struct DuplicatesUi {
    groups: RefCell<Vec<DuplicateGroupView>>,
    selected: RefCell<HashSet<VeyraPath>>,
    checkboxes: RefCell<HashMap<VeyraPath, gtk4::CheckButton>>,
    list: gtk4::ListBox,
    summary_label: gtk4::Label,
    status_label: gtk4::Label,
    trash_button: gtk4::Button,
    dialog: adw::Dialog,
    navigate: Rc<dyn Fn(VeyraPath)>,
    refresh_all: Rc<dyn Fn()>,
    undo_stack: SharedUndoStack,
}

/// Builds the fully interactive Duplicate Files view once the background
/// hash pass has produced at least one confirmed group.
fn build_duplicates_interactive_view(
    groups: Vec<DuplicateGroupView>,
    navigate: Rc<dyn Fn(VeyraPath)>,
    dialog: adw::Dialog,
    refresh_all: Rc<dyn Fn()>,
    undo_stack: SharedUndoStack,
) -> gtk4::Widget {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    let summary_label = gtk4::Label::new(None);
    summary_label.add_css_class("dim-label");
    summary_label.set_xalign(0.0);
    summary_label.set_margin_start(16);
    summary_label.set_margin_end(16);
    summary_label.set_margin_top(12);
    root.append(&summary_label);

    let helpers = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    helpers.set_margin_start(16);
    helpers.set_margin_end(16);
    helpers.set_margin_top(8);
    let keep_newest_button = gtk4::Button::with_label(t("dup.select_keep_newest"));
    let keep_oldest_button = gtk4::Button::with_label(t("dup.select_keep_oldest"));
    let clear_button = gtk4::Button::with_label(t("dup.clear_selection"));
    helpers.append(&keep_newest_button);
    helpers.append(&keep_oldest_button);
    helpers.append(&clear_button);
    root.append(&helpers);

    let list = gtk4::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk4::SelectionMode::None);
    list.set_margin_top(8);
    list.set_margin_bottom(12);
    list.set_margin_start(16);
    list.set_margin_end(16);

    let scroller = gtk4::ScrolledWindow::new();
    scroller.set_vexpand(true);
    scroller.set_child(Some(&list));
    root.append(&scroller);

    let bottom_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    bottom_bar.set_margin_start(16);
    bottom_bar.set_margin_end(16);
    bottom_bar.set_margin_top(8);
    bottom_bar.set_margin_bottom(12);
    let status_label = gtk4::Label::new(Some(t("dup.selection_status.empty")));
    status_label.set_hexpand(true);
    status_label.set_xalign(0.0);
    let trash_button = gtk4::Button::with_label(t("dup.move_to_trash"));
    trash_button.add_css_class("destructive-action");
    trash_button.set_sensitive(false);
    bottom_bar.append(&status_label);
    bottom_bar.append(&trash_button);
    root.append(&bottom_bar);

    update_summary_label(&summary_label, &groups);

    let ui = Rc::new(DuplicatesUi {
        groups: RefCell::new(groups),
        selected: RefCell::new(HashSet::new()),
        checkboxes: RefCell::new(HashMap::new()),
        list,
        summary_label,
        status_label,
        trash_button: trash_button.clone(),
        dialog,
        navigate,
        refresh_all,
        undo_stack,
    });

    render_duplicate_groups(&ui);

    {
        let ui = ui.clone();
        keep_newest_button.connect_clicked(move |_| select_all_copies(&ui, true));
    }
    {
        let ui = ui.clone();
        keep_oldest_button.connect_clicked(move |_| select_all_copies(&ui, false));
    }
    {
        let ui = ui.clone();
        clear_button.connect_clicked(move |_| clear_selection(&ui));
    }
    {
        let ui = ui.clone();
        trash_button.connect_clicked(move |_| on_trash_selected_clicked(&ui));
    }

    root.upcast()
}

/// Rebuilds `ui.list`'s rows from `ui.groups`'s current contents — called
/// once up front and again after a successful cleanup, since trashed files
/// (and any group they collapse below 2 members) must disappear from the
/// view without a full tab reload.
fn render_duplicate_groups(ui: &Rc<DuplicatesUi>) {
    while let Some(child) = ui.list.first_child() {
        ui.list.remove(&child);
    }
    ui.checkboxes.borrow_mut().clear();

    for group in ui.groups.borrow().iter() {
        let expander = adw::ExpanderRow::builder()
            .title(t_plural(
                "dup.group.copies",
                group.files.len() as i64,
                &[
                    ("count", group.files.len().to_string().as_str()),
                    ("size", format_size(group.size_per_file).as_str()),
                ],
            ))
            .subtitle(t_fmt(
                "dup.group.total",
                &[(
                    "size",
                    &format_size(group.size_per_file * group.files.len() as u64),
                )],
            ))
            .build();
        expander.set_title_lines(1);
        expander.set_subtitle_lines(2);
        expander.add_prefix(&gtk4::Image::from_icon_name("edit-copy-symbolic"));

        for entry in &group.files {
            let member_row = adw::ActionRow::builder()
                .title(
                    entry
                        .path
                        .file_name()
                        .unwrap_or_else(|| entry.path.to_string()),
                )
                .subtitle(format!(
                    "{}  ·  {}",
                    entry.path,
                    entry
                        .modified
                        .map(|dt| dt
                            .with_timezone(&Local)
                            .format("%Y-%m-%d %H:%M:%S")
                            .to_string())
                        .unwrap_or_else(|| t("dev.metadata.unknown").to_string())
                ))
                .build();
            member_row.set_title_lines(1);
            member_row.set_subtitle_lines(2);

            let checkbox = gtk4::CheckButton::new();
            checkbox.set_valign(gtk4::Align::Center);
            checkbox.set_active(ui.selected.borrow().contains(&entry.path));
            {
                let ui = ui.clone();
                let path = entry.path.clone();
                checkbox.connect_toggled(move |checkbox| {
                    if checkbox.is_active() {
                        ui.selected.borrow_mut().insert(path.clone());
                    } else {
                        ui.selected.borrow_mut().remove(&path);
                    }
                    update_selection_status(&ui);
                });
            }
            member_row.add_prefix(&checkbox);
            member_row.add_suffix(&reveal_in_folder_button(
                entry.path.clone(),
                ui.navigate.clone(),
                ui.dialog.clone(),
            ));

            ui.checkboxes
                .borrow_mut()
                .insert(entry.path.clone(), checkbox);
            expander.add_row(&member_row);
        }

        ui.list.append(&expander);
    }

    update_selection_status(ui);
}

fn update_summary_label(label: &gtk4::Label, groups: &[DuplicateGroupView]) {
    let group_count = groups.len();
    let file_count: usize = groups.iter().map(|g| g.files.len()).sum();
    let wasted: u64 = groups.iter().map(|g| g.wasted_size).sum();
    let text = format!(
        "{} • {} • {}",
        t_plural(
            "dup.summary.groups",
            group_count as i64,
            &[("count", group_count.to_string().as_str())]
        ),
        t_plural(
            "dup.summary.files",
            file_count as i64,
            &[("count", file_count.to_string().as_str())]
        ),
        t_fmt("dup.summary.wasted", &[("size", &format_size(wasted))]),
    );
    label.set_text(&text);
}

/// Updates the bottom status label and "Move Selected to Trash" button
/// sensitivity to match `ui.selected`'s current size — called after every
/// checkbox toggle and after any programmatic selection change.
fn update_selection_status(ui: &Rc<DuplicatesUi>) {
    let selected = ui.selected.borrow();
    let count = selected.len();
    if count == 0 {
        ui.status_label.set_text(t("dup.selection_status.empty"));
        ui.trash_button.set_sensitive(false);
        return;
    }

    let size_by_path: HashMap<VeyraPath, u64> = ui
        .groups
        .borrow()
        .iter()
        .flat_map(|g| {
            g.files
                .iter()
                .map(move |f| (f.path.clone(), g.size_per_file))
        })
        .collect();
    let total: u64 = selected
        .iter()
        .map(|p| size_by_path.get(p).copied().unwrap_or(0))
        .sum();

    ui.status_label.set_text(&t_plural(
        "dup.selection_status",
        count as i64,
        &[
            ("count", count.to_string().as_str()),
            ("size", format_size(total).as_str()),
        ],
    ));
    ui.trash_button.set_sensitive(true);
}

/// "Select All Copies (Keep Newest/Oldest)": per group, picks the file to
/// *keep* (the newest, or oldest, by `modified`; a file with unknown
/// `modified` never wins that comparison, and the first file is kept if
/// every member's `modified` is unknown) and checks every other member.
/// Never touches the kept file's own checkbox, so a group is never fully
/// selected by this helper alone.
fn select_all_copies(ui: &Rc<DuplicatesUi>, keep_newest: bool) {
    let checkboxes = ui.checkboxes.borrow();
    for group in ui.groups.borrow().iter() {
        if group.files.is_empty() {
            continue;
        }
        let known = group
            .files
            .iter()
            .enumerate()
            .filter_map(|(index, f)| f.modified.map(|dt| (index, dt)));
        let keep_index = if keep_newest {
            known.max_by_key(|(_, dt)| *dt)
        } else {
            known.min_by_key(|(_, dt)| *dt)
        }
        .map(|(index, _)| index)
        .unwrap_or(0);

        for (index, entry) in group.files.iter().enumerate() {
            if index == keep_index {
                continue;
            }
            if let Some(checkbox) = checkboxes.get(&entry.path) {
                checkbox.set_active(true);
            }
        }
    }
}

fn clear_selection(ui: &Rc<DuplicatesUi>) {
    for checkbox in ui.checkboxes.borrow().values() {
        if checkbox.is_active() {
            checkbox.set_active(false);
        }
    }
}

/// "Move Selected to Trash": enforces the all-copies-delete-prevention rule
/// (Rule #38 — at least one file per duplicate group must survive), then
/// shows the confirmation dialog, then actually trashes on confirmation.
fn on_trash_selected_clicked(ui: &Rc<DuplicatesUi>) {
    let selected = ui.selected.borrow().clone();
    if selected.is_empty() {
        return;
    }

    let fully_selected_groups: Vec<String> = ui
        .groups
        .borrow()
        .iter()
        .filter(|g| g.files.iter().all(|f| selected.contains(&f.path)))
        .map(|g| {
            g.files[0]
                .path
                .file_name()
                .unwrap_or_else(|| g.files[0].path.to_string())
        })
        .collect();

    if !fully_selected_groups.is_empty() {
        let dialog = adw::AlertDialog::builder()
            .heading(t("dup.block_all_copies.title"))
            .body(t_fmt(
                "dup.block_all_copies.body",
                &[("names", &fully_selected_groups.join(", "))],
            ))
            .build();
        dialog.add_responses(&[("ok", t("dup.block_all_copies.close"))]);
        dialog.set_default_response(Some("ok"));
        dialog.set_close_response("ok");
        dialog.present(Some(&ui.dialog));
        return;
    }

    let size_by_path: HashMap<VeyraPath, u64> = ui
        .groups
        .borrow()
        .iter()
        .flat_map(|g| {
            g.files
                .iter()
                .map(move |f| (f.path.clone(), g.size_per_file))
        })
        .collect();
    let total_size: u64 = selected
        .iter()
        .map(|p| size_by_path.get(p).copied().unwrap_or(0))
        .sum();
    let count = selected.len();

    let confirm = adw::AlertDialog::builder()
        .heading(t_plural(
            "dup.confirm.title",
            count as i64,
            &[("count", count.to_string().as_str())],
        ))
        .body(t_fmt(
            "dup.confirm.body",
            &[
                ("count", count.to_string().as_str()),
                ("size", format_size(total_size).as_str()),
            ],
        ))
        .build();
    confirm.add_responses(&[
        ("cancel", t("dup.confirm.cancel")),
        ("trash", t("dup.confirm.confirm")),
    ]);
    confirm.set_response_appearance("trash", adw::ResponseAppearance::Destructive);
    confirm.set_default_response(Some("cancel"));
    confirm.set_close_response("cancel");

    confirm.present(Some(&ui.dialog));
    let ui = ui.clone();
    confirm.connect_response(None, move |_, response| {
        if response == "trash" {
            trash_selected(&ui, selected.clone());
        }
    });
}

/// Actually moves every path in `selected` to Trash (background thread,
/// Rule #11/#14), records the batch as one undo step, refreshes the
/// caller's open panels, and re-renders the tab's list with the trashed
/// files (and any group they collapsed below 2 members) removed. A file
/// that fails to trash (permission denied, already gone) is skipped rather
/// than aborting the whole batch (Rule #18).
/// `(succeeded, failed)` from a batch `trash_tracked` pass: `succeeded` as
/// `(original, trashed)` pairs (the shape `UndoableAction::Trash` records),
/// `failed` as `(original, error)`.
type TrashBatchResult = (
    Vec<(VeyraPath, VeyraPath)>,
    Vec<(VeyraPath, veyra_filesystem::FsError)>,
);

fn trash_selected(ui: &Rc<DuplicatesUi>, selected: HashSet<VeyraPath>) {
    let paths: Vec<VeyraPath> = selected.into_iter().collect();
    let ui = ui.clone();
    fs_async::run_blocking(
        move || {
            let mut succeeded = Vec::new();
            let mut failed = Vec::new();
            for path in paths {
                match veyra_filesystem::trash_tracked(&path) {
                    Ok(trashed) => succeeded.push((path, trashed)),
                    Err(err) => failed.push((path, err)),
                }
            }
            (succeeded, failed)
        },
        move |(succeeded, failed): TrashBatchResult| {
            if !succeeded.is_empty() {
                ui.undo_stack
                    .borrow_mut()
                    .record(UndoableAction::Trash(succeeded.clone()));
                let trashed_paths: HashSet<VeyraPath> = succeeded
                    .iter()
                    .map(|(original, _)| original.clone())
                    .collect();

                let mut groups = ui.groups.borrow_mut();
                for group in groups.iter_mut() {
                    group.files.retain(|f| !trashed_paths.contains(&f.path));
                    group.wasted_size = group
                        .size_per_file
                        .saturating_mul(group.files.len().saturating_sub(1) as u64);
                }
                groups.retain(|g| g.files.len() >= 2);
                ui.selected
                    .borrow_mut()
                    .retain(|p| !trashed_paths.contains(p));

                update_summary_label(&ui.summary_label, &groups);
                drop(groups);
                render_duplicate_groups(&ui);
                (ui.refresh_all)();
            }

            if !failed.is_empty() {
                let dialog = adw::AlertDialog::builder()
                    .heading(t("dup.trash_error.title"))
                    .body(
                        failed
                            .iter()
                            .map(|(path, err)| format!("{path}: {err}"))
                            .collect::<Vec<_>>()
                            .join("\n"),
                    )
                    .build();
                dialog.add_responses(&[("ok", t("dup.block_all_copies.close"))]);
                dialog.set_default_response(Some("ok"));
                dialog.set_close_response("ok");
                dialog.present(Some(&ui.dialog));
            }
        },
    );
}

/// A small "Open in Folder" icon button: navigates the caller's file browser
/// to `path`'s parent directory and closes the analyzer.
fn open_in_folder_button(
    path: VeyraPath,
    navigate: Rc<dyn Fn(VeyraPath)>,
    dialog: adw::Dialog,
) -> gtk4::Button {
    let button = gtk4::Button::from_icon_name("folder-open-symbolic");
    button.add_css_class("flat");
    button.set_valign(gtk4::Align::Center);
    button.set_tooltip_text(Some(t("storage.open_in_folder")));
    button.update_property(&[gtk4::accessible::Property::Label("Open in Folder")]);
    button.connect_clicked(move |_| {
        navigate(parent_path(&path));
        dialog.close();
    });
    button
}

/// Same as `open_in_folder_button`, but with the Duplicate Files tab's own
/// "Reveal in Folder" (Faz 42) label rather than "Open in Folder" — the
/// exact wording differs, the behavior (navigate the caller's browser
/// there, then close the analyzer) is identical.
fn reveal_in_folder_button(
    path: VeyraPath,
    navigate: Rc<dyn Fn(VeyraPath)>,
    dialog: adw::Dialog,
) -> gtk4::Button {
    let button = gtk4::Button::from_icon_name("folder-open-symbolic");
    button.add_css_class("flat");
    button.set_valign(gtk4::Align::Center);
    button.set_tooltip_text(Some(t("dup.reveal_in_folder")));
    button.update_property(&[gtk4::accessible::Property::Label(t("dup.reveal_in_folder"))]);
    button.connect_clicked(move |_| {
        navigate(parent_path(&path));
        dialog.close();
    });
    button
}

/// A small solid-color rounded square used as the breakdown list's
/// per-directory color key, matching the segment it corresponds to in the
/// proportional bar above.
fn color_swatch(color: (f64, f64, f64)) -> gtk4::DrawingArea {
    let swatch = gtk4::DrawingArea::new();
    swatch.set_content_width(12);
    swatch.set_content_height(12);
    swatch.set_valign(gtk4::Align::Center);
    swatch.set_draw_func(move |_area, cr, width, height| {
        let (r, g, b) = color;
        cr.set_source_rgb(r, g, b);
        let radius = 3.0;
        let width = width as f64;
        let height = height as f64;
        cr.new_path();
        cr.arc(
            radius,
            radius,
            radius,
            std::f64::consts::PI,
            1.5 * std::f64::consts::PI,
        );
        cr.arc(
            width - radius,
            radius,
            radius,
            1.5 * std::f64::consts::PI,
            2.0 * std::f64::consts::PI,
        );
        cr.arc(
            width - radius,
            height - radius,
            radius,
            0.0,
            0.5 * std::f64::consts::PI,
        );
        cr.arc(
            radius,
            height - radius,
            radius,
            0.5 * std::f64::consts::PI,
            std::f64::consts::PI,
        );
        cr.close_path();
        let _ = cr.fill();
    });
    swatch
}

fn empty_state(message: &str) -> gtk4::Widget {
    let label = gtk4::Label::new(Some(message));
    label.add_css_class("dim-label");
    label.set_valign(gtk4::Align::Center);
    label.set_vexpand(true);
    label.upcast()
}

/// The containing directory of `path` (falls back to `path` itself at the
/// filesystem/URI root).
fn parent_path(path: &VeyraPath) -> VeyraPath {
    match path {
        VeyraPath::Local(local) => local
            .parent()
            .map(VeyraPath::from_local)
            .unwrap_or_else(|| path.clone()),
        VeyraPath::Uri(uri) => uri
            .trim_end_matches('/')
            .rsplit_once('/')
            .map(|(parent, _)| VeyraPath::from_uri(parent.to_string()))
            .unwrap_or_else(|| path.clone()),
    }
}
