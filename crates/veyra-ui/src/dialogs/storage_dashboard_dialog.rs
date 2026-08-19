//! Faz 43: the Smart Storage Dashboard — a single at-a-glance overview of
//! root filesystem fullness, the Home directory's largest folders, the
//! user's most recent files, and any duplicate-file cleanup opportunity.
//!
//! Opens instantly with a spinner: the root filesystem's usage
//! (`devices::query_usage`), a `veyra_filesystem::analyze_directory` walk of
//! Home (its top-level largest folders plus same-size duplicate
//! candidates, both already computed in that one walk — Faz 20), and the
//! Recent Files registry snapshot are all fetched off the GTK main thread
//! via `fs_async::run_blocking` (Rule #11/#12). `RecentManager` itself is
//! main-thread-only, so its snapshot is grabbed before that background
//! thread starts, matching `recent::snapshot_entries`'s own contract.
//!
//! The Duplicates card only *finds* content-confirmed duplicates when the
//! user presses "Scan Duplicates" — running `find_duplicates` over the
//! candidates already produced by the Home walk above needs no second scan
//! of the filesystem, just the (potentially slower) content-hash pass, kept
//! opt-in so opening the dashboard never pays for it.

use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use veyra_filesystem::{
    find_duplicates, format_size, DuplicateGroup, FileItem, OperationControl,
    SameSizeCandidateGroup, UsageEntry, VeyraPath,
};

use crate::devices;
use crate::dialogs;
use crate::fs_async;
use crate::i18n::{t, t_fmt, t_plural};
use crate::recent;
use crate::undo::SharedUndoStack;

/// How many entries the Largest Folders / Recent Files cards show.
const TOP_N: usize = 5;

/// Everything the background pass gathers, handed to `build_loaded_view` in
/// one piece once it lands back on the GTK main thread.
struct DashboardData {
    usage: Option<devices::UsageInfo>,
    largest_folders: Vec<UsageEntry>,
    duplicate_candidates: Vec<SameSizeCandidateGroup>,
    recent_files: Vec<FileItem>,
}

/// Shows the Smart Storage Dashboard, parented to `parent`. `home` is the
/// directory the Largest Folders/Duplicates cards scan (the user's Home
/// directory). `navigate`/`refresh_all`/`undo_stack` are threaded through to
/// the nested Disk Analyzer dialog the "Analyze…"/"Open in Disk Analyzer…"
/// buttons open, exactly as every other Disk Analyzer entry point does.
pub(crate) fn show(
    parent: &impl IsA<gtk4::Widget>,
    home: VeyraPath,
    navigate: Rc<dyn Fn(VeyraPath)>,
    refresh_all: Rc<dyn Fn()>,
    undo_stack: SharedUndoStack,
) {
    let dialog = adw::Dialog::builder()
        .title(t("storage.dialog.title"))
        .content_width(640)
        .content_height(700)
        .build();

    let spinner = gtk4::Spinner::new();
    spinner.set_spinning(true);
    spinner.set_size_request(32, 32);
    spinner.set_halign(gtk4::Align::Center);
    spinner.set_valign(gtk4::Align::Center);
    spinner.set_vexpand(true);
    let status_label = gtk4::Label::new(Some(t("storage.loading")));
    status_label.add_css_class("dim-label");
    let loading_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    loading_box.append(&spinner);
    loading_box.append(&status_label);

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&gtk4::Label::new(Some(t("storage.dialog.title")))));
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&loading_box));
    dialog.set_child(Some(&toolbar_view));

    let control = OperationControl::new();
    {
        let control = control.clone();
        dialog.connect_closed(move |_| control.cancel());
    }

    let recent_entries = recent::snapshot_entries();
    let root_path = VeyraPath::from_local("/");
    let scan_home = home.clone();
    let scan_control = control.clone();
    let dialog_for_result = dialog.clone();
    fs_async::run_blocking(
        move || {
            let usage = devices::query_usage(&root_path);
            let analysis = veyra_filesystem::analyze_directory(&scan_home, &scan_control);
            let recent_files = recent::list_recent_items(recent_entries);
            let (largest_folders, duplicate_candidates) = match analysis {
                Ok(result) => (result.largest_dirs, result.duplicate_candidates),
                Err(_) => (Vec::new(), Vec::new()),
            };
            DashboardData {
                usage,
                largest_folders,
                duplicate_candidates,
                recent_files,
            }
        },
        move |data| {
            build_loaded_view(
                &dialog_for_result,
                &toolbar_view,
                data,
                home,
                navigate,
                refresh_all,
                undo_stack,
            );
        },
    );

    dialog.present(Some(parent));
}

/// Replaces the loading spinner with the four cards once the background
/// pass finishes.
fn build_loaded_view(
    dialog: &adw::Dialog,
    toolbar_view: &adw::ToolbarView,
    data: DashboardData,
    home: VeyraPath,
    navigate: Rc<dyn Fn(VeyraPath)>,
    refresh_all: Rc<dyn Fn()>,
    undo_stack: SharedUndoStack,
) {
    let page = adw::PreferencesPage::new();

    page.add(&build_gauge_group(data.usage.as_ref()));
    page.add(&build_largest_folders_group(
        &data.largest_folders,
        dialog.clone(),
        home.clone(),
        navigate.clone(),
        refresh_all.clone(),
        undo_stack.clone(),
    ));
    page.add(&build_recent_files_group(
        &data.recent_files,
        dialog.clone(),
        navigate.clone(),
    ));
    page.add(&build_duplicates_group(
        data.duplicate_candidates,
        dialog.clone(),
        home,
        navigate,
        refresh_all,
        undo_stack,
    ));

    toolbar_view.set_content(Some(&page));
}

/// The Storage Gauge card: drive icon/name, a proportional fill bar, and the
/// Used/Free/Total summary line. Shows a plain "unavailable" row instead
/// (rather than a bar stuck at 0%) when the backend can't report filesystem
/// stats (Rule #15/#18).
fn build_gauge_group(usage: Option<&devices::UsageInfo>) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(t("storage.gauge.title"))
        .build();

    let Some(usage) = usage else {
        group.add(&empty_row(t("storage.gauge.unavailable")));
        return group;
    };

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    content.set_margin_top(8);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);

    let header_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    header_row.append(&gtk4::Image::from_icon_name(
        "drive-harddisk-system-symbolic",
    ));
    let name_label = gtk4::Label::new(Some(t("storage.gauge.drive_label")));
    name_label.set_xalign(0.0);
    name_label.set_hexpand(true);
    name_label.add_css_class("heading");
    header_row.append(&name_label);
    let percent_label = gtk4::Label::new(Some(&format!("{}%", gauge_percent(usage))));
    percent_label.add_css_class("dim-label");
    header_row.append(&percent_label);
    content.append(&header_row);

    let bar = gtk4::ProgressBar::new();
    bar.set_fraction(devices::usage_fraction(usage));
    content.append(&bar);

    let stats_label = gtk4::Label::new(Some(&gauge_summary_label(usage)));
    stats_label.set_xalign(0.0);
    stats_label.add_css_class("dim-label");
    content.append(&stats_label);

    group.add(&content);
    group
}

/// Rounds `usage`'s fraction-used to the nearest whole percent, `0` for a
/// filesystem reporting zero total size.
fn gauge_percent(usage: &devices::UsageInfo) -> u32 {
    (devices::usage_fraction(usage) * 100.0).round() as u32
}

/// e.g. `"Used: 720.0 GB — Free: 280.0 GB — Total: 1.0 TB"`.
fn gauge_summary_label(usage: &devices::UsageInfo) -> String {
    t_fmt(
        "storage.gauge.summary",
        &[
            ("used", &format_size(usage.used)),
            ("free", &format_size(usage.free)),
            ("total", &format_size(usage.total)),
        ],
    )
}

/// The Largest Folders card: up to `TOP_N` entries from the Home walk's
/// already size-sorted `largest_dirs`, plus a card-level "Analyze…" button
/// that opens the full Disk Analyzer on Home for deeper drill-down.
fn build_largest_folders_group(
    entries: &[UsageEntry],
    dialog: adw::Dialog,
    home: VeyraPath,
    navigate: Rc<dyn Fn(VeyraPath)>,
    refresh_all: Rc<dyn Fn()>,
    undo_stack: SharedUndoStack,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(t("storage.largest_folders.title"))
        .build();

    let analyze_button = gtk4::Button::with_label(t("storage.largest_folders.analyze"));
    analyze_button.add_css_class("flat");
    analyze_button.set_valign(gtk4::Align::Center);
    {
        let dialog = dialog.clone();
        let home = home.clone();
        let navigate = navigate.clone();
        let refresh_all = refresh_all.clone();
        let undo_stack = undo_stack.clone();
        analyze_button.connect_clicked(move |_| {
            dialogs::disk_analyzer_dialog::show(
                &dialog,
                home.clone(),
                navigate.clone(),
                refresh_all.clone(),
                undo_stack.clone(),
            );
        });
    }
    group.set_header_suffix(Some(&analyze_button));

    let top = take_top(entries, TOP_N);
    if top.is_empty() {
        group.add(&empty_row(t("storage.largest_folders.empty")));
        return group;
    }

    let navigate = navigate_button_factory(navigate, dialog);
    for entry in top {
        let row = adw::ActionRow::builder()
            .title(&entry.name)
            .subtitle(format_size(entry.size_bytes))
            .build();
        row.set_title_lines(1);
        row.set_subtitle_lines(1);
        row.add_prefix(&gtk4::Image::from_icon_name("folder-symbolic"));
        row.add_suffix(&navigate(entry.path.clone()));
        group.add(&row);
    }

    group
}

/// The Recent Files card: up to `TOP_N` entries from the Recent Files
/// registry, each opening the containing folder on click.
fn build_recent_files_group(
    items: &[FileItem],
    dialog: adw::Dialog,
    navigate: Rc<dyn Fn(VeyraPath)>,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(t("storage.recent_files.title"))
        .build();

    let top = take_top(items, TOP_N);
    if top.is_empty() {
        group.add(&empty_row(t("storage.recent_files.empty")));
        return group;
    }

    let navigate_button = navigate_button_factory(navigate, dialog);
    for item in top {
        let row = adw::ActionRow::builder()
            .title(item.name())
            .subtitle(item.path.to_string())
            .build();
        row.set_title_lines(1);
        row.set_subtitle_lines(1);
        row.add_prefix(&gtk4::Image::from_icon_name("text-x-generic-symbolic"));
        row.add_suffix(&navigate_button(parent_path(&item.path)));
        group.add(&row);
    }

    group
}

/// The Duplicates card: the same-size candidate count up front (free —
/// already computed by the Home walk), and a "Scan Duplicates" button that
/// runs the content-hash pass (`find_duplicates`, Faz 42) in the background
/// on click, replacing the count with the confirmed group count + wasted
/// space. "Open in Disk Analyzer…" always jumps straight to the full
/// interactive cleanup view.
fn build_duplicates_group(
    candidates: Vec<SameSizeCandidateGroup>,
    dialog: adw::Dialog,
    home: VeyraPath,
    navigate: Rc<dyn Fn(VeyraPath)>,
    refresh_all: Rc<dyn Fn()>,
    undo_stack: SharedUndoStack,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(t("storage.duplicates.title"))
        .build();

    let open_button = gtk4::Button::with_label(t("storage.duplicates.open_analyzer"));
    open_button.add_css_class("flat");
    open_button.set_valign(gtk4::Align::Center);
    {
        let dialog = dialog.clone();
        let home = home.clone();
        open_button.connect_clicked(move |_| {
            dialogs::disk_analyzer_dialog::show(
                &dialog,
                home.clone(),
                navigate.clone(),
                refresh_all.clone(),
                undo_stack.clone(),
            );
        });
    }
    group.set_header_suffix(Some(&open_button));

    let status_row = adw::ActionRow::builder()
        .title(t_plural(
            "storage.duplicates.candidates",
            candidates.len() as i64,
            &[("count", candidates.len().to_string().as_str())],
        ))
        .build();
    status_row.add_prefix(&gtk4::Image::from_icon_name("edit-copy-symbolic"));

    let scan_button = gtk4::Button::with_label(t("storage.duplicates.scan"));
    scan_button.set_valign(gtk4::Align::Center);
    status_row.add_suffix(&scan_button);
    group.add(&status_row);

    let status_row_for_scan = status_row.clone();
    let button_for_async = scan_button.clone();
    scan_button.connect_clicked(move |button| {
        button.set_sensitive(false);
        status_row_for_scan.set_title(t("storage.duplicates.scanning"));

        let candidates = candidates.clone();
        let status_row = status_row_for_scan.clone();
        let button = button_for_async.clone();
        fs_async::run_blocking(
            move || {
                let control = OperationControl::new();
                find_duplicates(&candidates, &control)
            },
            move |groups: Vec<DuplicateGroup>| {
                let (count, wasted) = duplicate_summary(&groups);
                if count == 0 {
                    status_row.set_title(t("storage.duplicates.empty_after_scan"));
                } else {
                    status_row.set_title(&t_plural(
                        "storage.duplicates.result",
                        count as i64,
                        &[
                            ("count", count.to_string().as_str()),
                            ("size", format_size(wasted).as_str()),
                        ],
                    ));
                }
                button.set_visible(false);
            },
        );
    });

    group
}

/// `(confirmed group count, total wasted size)` across `groups`.
fn duplicate_summary(groups: &[DuplicateGroup]) -> (usize, u64) {
    let wasted = groups.iter().map(|g| g.wasted_size).sum();
    (groups.len(), wasted)
}

/// The first `n` entries of `items` (or all of them, if fewer).
fn take_top<T: Clone>(items: &[T], n: usize) -> Vec<T> {
    items.iter().take(n).cloned().collect()
}

/// Builds a reusable "navigate here and close the dashboard" icon-button
/// factory closing over `navigate`/`dialog`, so each card's loop just calls
/// it per row instead of repeating the click-wiring boilerplate.
fn navigate_button_factory(
    navigate: Rc<dyn Fn(VeyraPath)>,
    dialog: adw::Dialog,
) -> impl Fn(VeyraPath) -> gtk4::Button {
    move |target: VeyraPath| {
        let button = gtk4::Button::from_icon_name("folder-open-symbolic");
        button.add_css_class("flat");
        button.set_valign(gtk4::Align::Center);
        button.set_tooltip_text(Some(t("storage.open_in_folder")));
        button.update_property(&[gtk4::accessible::Property::Label(t(
            "storage.open_in_folder",
        ))]);
        let navigate = navigate.clone();
        let dialog = dialog.clone();
        button.connect_clicked(move |_| {
            navigate(target.clone());
            dialog.close();
        });
        button
    }
}

fn empty_row(message: &str) -> adw::ActionRow {
    adw::ActionRow::builder().title(message).build()
}

/// The containing directory of `path` (falls back to `path` itself at the
/// filesystem/URI root) — matches `disk_analyzer_dialog`'s own
/// `parent_path` helper.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(total: u64, used: u64, free: u64) -> devices::UsageInfo {
        devices::UsageInfo {
            total,
            used,
            free,
            fs_type: None,
        }
    }

    #[test]
    fn gauge_percent_rounds_to_nearest_whole_percent() {
        assert_eq!(gauge_percent(&usage(1000, 250, 750)), 25);
        assert_eq!(gauge_percent(&usage(3, 1, 2)), 33);
        assert_eq!(gauge_percent(&usage(3, 2, 1)), 67);
    }

    #[test]
    fn gauge_percent_zero_total_is_zero() {
        assert_eq!(gauge_percent(&usage(0, 0, 0)), 0);
    }

    #[test]
    fn gauge_summary_label_formats_used_free_total() {
        let label = gauge_summary_label(&usage(
            1024 * 1024 * 1024,
            512 * 1024 * 1024,
            512 * 1024 * 1024,
        ));
        assert!(label.contains("512.0 MB"));
        assert!(label.contains("1.0 GB"));
    }

    #[test]
    fn take_top_truncates_to_n() {
        let items = vec![1, 2, 3, 4, 5, 6];
        assert_eq!(take_top(&items, 5), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn take_top_returns_fewer_when_input_is_shorter() {
        let items = vec![1, 2];
        assert_eq!(take_top(&items, 5), vec![1, 2]);
    }

    #[test]
    fn duplicate_summary_sums_group_counts_and_wasted_size() {
        let groups = vec![
            DuplicateGroup {
                hash: "a".into(),
                size_per_file: 100,
                wasted_size: 100,
                files: vec![],
            },
            DuplicateGroup {
                hash: "b".into(),
                size_per_file: 200,
                wasted_size: 400,
                files: vec![],
            },
        ];
        assert_eq!(duplicate_summary(&groups), (2, 500));
    }

    #[test]
    fn duplicate_summary_empty_is_zero() {
        assert_eq!(duplicate_summary(&[]), (0, 0));
    }
}
