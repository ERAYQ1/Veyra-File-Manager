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
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use veyra_filesystem::{
    format_size, AnalysisResult, DuplicateGroup, OperationControl, UsageEntry, UsageNode, VeyraPath,
};

use crate::fs_async;

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
/// the Largest Files/Duplicates tabs).
pub(crate) fn show(
    parent: &impl IsA<gtk4::Widget>,
    root: VeyraPath,
    navigate: Rc<dyn Fn(VeyraPath)>,
) {
    let dialog = adw::Dialog::builder()
        .title("Disk Usage")
        .content_width(760)
        .content_height(640)
        .build();

    let spinner = gtk4::Spinner::new();
    spinner.set_spinning(true);
    spinner.set_size_request(32, 32);
    spinner.set_halign(gtk4::Align::Center);
    spinner.set_valign(gtk4::Align::Center);
    spinner.set_vexpand(true);

    let status_label = gtk4::Label::new(Some(&format!("Scanning {root}…")));
    status_label.add_css_class("dim-label");

    let loading_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    loading_box.append(&spinner);
    loading_box.append(&status_label);

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&gtk4::Label::new(Some("Disk Usage"))));
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
                );
            }
            Err(err) => {
                header_for_result.set_title_widget(Some(&gtk4::Label::new(Some("Disk Usage"))));
                let error_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
                error_box.set_valign(gtk4::Align::Center);
                error_box.set_vexpand(true);
                let icon = gtk4::Image::from_icon_name("dialog-error-symbolic");
                icon.set_pixel_size(48);
                let label = gtk4::Label::new(Some(&format!("Couldn't scan this folder: {err}")));
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
    duplicate_candidates: Vec<DuplicateGroup>,
}

/// Replaces the loading spinner with the real tabbed view once the scan
/// finishes, and wires breadcrumb drill-down/navigation.
fn build_loaded_view(
    dialog: &adw::Dialog,
    toolbar_view: &adw::ToolbarView,
    header: &adw::HeaderBar,
    analysis: AnalysisResult,
    navigate: Rc<dyn Fn(VeyraPath)>,
) {
    let state = Rc::new(LoadedState {
        tree: analysis.tree,
        largest_files: analysis.largest_files,
        duplicate_candidates: analysis.duplicate_candidates,
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
        &state.duplicate_candidates,
        navigate.clone(),
        dialog.clone(),
    );
    view_stack.add_titled_with_icon(
        &duplicates_page,
        Some("duplicates"),
        "Duplicates",
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
        let empty = gtk4::Label::new(Some("This folder is empty."));
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
        segments.push(("Other".to_string(), other));
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
        return empty_state("No files found.");
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

/// Builds the Duplicates tab: one expander row per same-size group, each
/// listing its member paths with their own "Open in Folder" action.
fn build_duplicates_page(
    groups: &[DuplicateGroup],
    navigate: Rc<dyn Fn(VeyraPath)>,
    dialog: adw::Dialog,
) -> gtk4::Widget {
    if groups.is_empty() {
        return empty_state("No duplicate-size candidates found.");
    }

    let list = gtk4::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk4::SelectionMode::None);
    list.set_margin_top(12);
    list.set_margin_bottom(12);
    list.set_margin_start(16);
    list.set_margin_end(16);

    for group in groups {
        let expander = adw::ExpanderRow::builder()
            .title(format!(
                "{} files, {} each",
                group.paths.len(),
                format_size(group.size_bytes)
            ))
            .subtitle(format!(
                "{} total",
                format_size(group.size_bytes * group.paths.len() as u64)
            ))
            .build();
        expander.set_title_lines(1);
        expander.set_subtitle_lines(2);
        expander.add_prefix(&gtk4::Image::from_icon_name("edit-copy-symbolic"));

        for path in &group.paths {
            let member_row = adw::ActionRow::builder()
                .title(path.file_name().unwrap_or_else(|| path.to_string()))
                .subtitle(path.to_string())
                .build();
            member_row.set_title_lines(1);
            member_row.set_subtitle_lines(2);
            member_row.add_suffix(&open_in_folder_button(
                path.clone(),
                navigate.clone(),
                dialog.clone(),
            ));
            expander.add_row(&member_row);
        }

        list.append(&expander);
    }

    let scroller = gtk4::ScrolledWindow::new();
    scroller.set_vexpand(true);
    scroller.set_child(Some(&list));
    scroller.upcast()
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
    button.set_tooltip_text(Some("Open in Folder"));
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
