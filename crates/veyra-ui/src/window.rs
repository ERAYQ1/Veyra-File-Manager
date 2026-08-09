use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use veyra_filesystem::{FileItem, OperationKind, OperationRequest, VeyraPath};

use crate::operations::OperationEvent;
use crate::state::{AppState, SharedState};
use crate::views::ViewMode;
use crate::widgets::progress_toast::ProgressToastHandles;
use crate::{breadcrumbs, dialogs, fs_async, headerbar, operations, sidebar, statusbar, widgets};

/// Widgets that `navigate_to` needs to refresh after every navigation.
/// Cloning is cheap: every field is a GTK widget handle (internally
/// refcounted), not owned state.
#[derive(Clone)]
struct Chrome {
    back_button: gtk4::Button,
    forward_button: gtk4::Button,
    up_button: gtk4::Button,
    breadcrumbs_box: gtk4::Box,
    title_stack: gtk4::Stack,
    address_entry: gtk4::Entry,
    status_left: gtk4::Label,
    status_right: gtk4::Label,
}

/// A single Copy/Cut clipboard slot (Faz 5 operates on the current
/// selection, not a multi-item marquee — see `AGENTS.md` scope note in the
/// Faz 5 changelog entry). `cut` decides whether `win.paste` runs a Move or
/// a Copy.
#[derive(Clone)]
struct ClipboardEntry {
    path: VeyraPath,
    cut: bool,
}

/// The three views' independent `GtkSingleSelection` chains, so keyboard
/// operations (Copy/Cut/Trash/Delete) can find "the selected item" in
/// whichever view is currently visible.
#[derive(Clone)]
struct ViewSelections {
    icon: gtk4::SingleSelection,
    compact: gtk4::SingleSelection,
    details: gtk4::SingleSelection,
}

impl ViewSelections {
    fn selected(&self, view_stack: &gtk4::Stack) -> Option<FileItem> {
        let selection = match view_stack.visible_child_name().as_deref() {
            Some(name) if name == ViewMode::Compact.stack_name() => &self.compact,
            Some(name) if name == ViewMode::Details.stack_name() => &self.details,
            _ => &self.icon,
        };
        crate::views::selected_item(selection)
    }
}

pub(crate) fn build_window(app: &adw::Application, start_dir: VeyraPath) -> adw::ApplicationWindow {
    let state = AppState::new(start_dir.clone());

    let search_query = Rc::new(RefCell::new(String::new()));
    let filter = build_search_filter(&state, search_query.clone());

    let view_stack = gtk4::Stack::new();

    let header = headerbar::build(&view_stack, search_query, filter.clone());
    let status_bar = statusbar::build();
    let progress = widgets::progress_toast::build();
    let clipboard: Rc<RefCell<Option<ClipboardEntry>>> = Rc::new(RefCell::new(None));

    let chrome = Chrome {
        back_button: header.back_button.clone(),
        forward_button: header.forward_button.clone(),
        up_button: header.up_button.clone(),
        breadcrumbs_box: header.breadcrumbs_box.clone(),
        title_stack: header.title_stack.clone(),
        address_entry: header.address_entry.clone(),
        status_left: status_bar.left_label.clone(),
        status_right: status_bar.right_label.clone(),
    };

    let navigate: Rc<dyn Fn(VeyraPath)> = {
        let state = state.clone();
        let chrome = chrome.clone();
        Rc::new(move |path: VeyraPath| navigate_to(&state, &chrome, path, true))
    };

    {
        let state = state.clone();
        let chrome = chrome.clone();
        header
            .back_button
            .connect_clicked(move |_| go_back(&state, &chrome));
    }
    {
        let state = state.clone();
        let chrome = chrome.clone();
        header
            .forward_button
            .connect_clicked(move |_| go_forward(&state, &chrome));
    }
    {
        let state = state.clone();
        let navigate = navigate.clone();
        header
            .up_button
            .connect_clicked(move |_| go_up(&state, &navigate));
    }
    {
        let navigate = navigate.clone();
        header.home_button.connect_clicked(move |_| {
            navigate(VeyraPath::from_local(gtk4::glib::home_dir()));
        });
    }
    {
        let state = state.clone();
        let chrome = chrome.clone();
        header
            .refresh_button
            .connect_clicked(move |_| refresh(&state, &chrome));
    }
    {
        let state = state.clone();
        let chrome = chrome.clone();
        header.address_entry.connect_activate(move |entry| {
            let text = entry.text();
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                navigate_to(
                    &state,
                    &chrome,
                    VeyraPath::from_local(std::path::PathBuf::from(trimmed)),
                    true,
                );
            }
            chrome.title_stack.set_visible_child_name("breadcrumbs");
        });
    }

    let on_open: Rc<dyn Fn(FileItem)> = {
        let navigate = navigate.clone();
        Rc::new(move |item: FileItem| {
            if item.kind().is_directory() {
                navigate(item.path.clone());
            } else {
                std::thread::spawn(move || {
                    if let Err(err) = veyra_filesystem::open(&item.path) {
                        tracing::warn!(path = %item.path, error = %err, "failed to open item");
                    }
                });
            }
        })
    };

    let has_clipboard: Rc<dyn Fn() -> bool> = {
        let clipboard = clipboard.clone();
        Rc::new(move || clipboard.borrow().is_some())
    };

    let selections;
    {
        let model = state.borrow().model.clone();
        let filter = filter.clone();
        let on_open = on_open.clone();

        let (icon_widget, icon_selection) = crate::views::build_icon_view(
            &model,
            &filter,
            {
                let on_open = on_open.clone();
                move |item| on_open(item)
            },
            has_clipboard.clone(),
        );
        view_stack.add_named(&icon_widget, Some(ViewMode::Icon.stack_name()));

        let (compact_widget, compact_selection) = crate::views::build_compact_view(
            &model,
            &filter,
            {
                let on_open = on_open.clone();
                move |item| on_open(item)
            },
            has_clipboard.clone(),
        );
        view_stack.add_named(&compact_widget, Some(ViewMode::Compact.stack_name()));

        let (details_widget, details_selection) = crate::views::build_details_view(
            &model,
            &filter,
            move |item| on_open(item),
            has_clipboard,
        );
        view_stack.add_named(&details_widget, Some(ViewMode::Details.stack_name()));

        view_stack.set_visible_child_name(ViewMode::Icon.stack_name());

        selections = ViewSelections {
            icon: icon_selection,
            compact: compact_selection,
            details: details_selection,
        };
    }

    let content_page = adw::NavigationPage::new(&view_stack, "Files");
    let sidebar_widget = sidebar::build(navigate.clone());
    let sidebar_page = adw::NavigationPage::new(&sidebar_widget, "Sidebar");

    let split_view = adw::NavigationSplitView::builder()
        .sidebar(&sidebar_page)
        .content(&content_page)
        .sidebar_width_fraction(0.22)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header.widget);
    toolbar_view.add_bottom_bar(&progress.widget);
    toolbar_view.add_bottom_bar(&status_bar.widget);
    toolbar_view.set_content(Some(&split_view));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Veyra - Modern Linux File Manager")
        .default_width(1200)
        .default_height(720)
        .content(&toolbar_view)
        .build();

    add_dev_icon_search_path(&window);
    if let Some(app_id) = app.application_id() {
        window.set_icon_name(Some(&app_id));
    }
    setup_shortcuts(
        app,
        &window,
        &state,
        &chrome,
        &navigate,
        &header.address_entry,
    );
    setup_operation_actions(
        app,
        &window,
        &state,
        &chrome,
        &progress,
        &view_stack,
        &selections,
        &clipboard,
    );
    setup_context_menu_actions(
        app,
        &window,
        &state,
        &chrome,
        &view_stack,
        &selections,
        &on_open,
    );

    navigate_to(&state, &chrome, start_dir, false);

    window
}

/// Registers `win.*` actions for the navigation shortcuts and binds their
/// accelerators on `app`. Actions (rather than raw key controllers) so the
/// bindings respect normal GTK focus/shortcut-inhibition rules (e.g. they
/// don't fire while a text entry elsewhere has focus and wants the key).
fn setup_shortcuts(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    state: &SharedState,
    chrome: &Chrome,
    navigate: &Rc<dyn Fn(VeyraPath)>,
    address_entry: &gtk4::Entry,
) {
    let action_back = gio::SimpleAction::new("go-back", None);
    {
        let state = state.clone();
        let chrome = chrome.clone();
        action_back.connect_activate(move |_, _| go_back(&state, &chrome));
    }
    window.add_action(&action_back);
    app.set_accels_for_action("win.go-back", &["<Alt>Left"]);

    let action_forward = gio::SimpleAction::new("go-forward", None);
    {
        let state = state.clone();
        let chrome = chrome.clone();
        action_forward.connect_activate(move |_, _| go_forward(&state, &chrome));
    }
    window.add_action(&action_forward);
    app.set_accels_for_action("win.go-forward", &["<Alt>Right"]);

    let action_up = gio::SimpleAction::new("go-up", None);
    {
        let state = state.clone();
        let navigate = navigate.clone();
        action_up.connect_activate(move |_, _| go_up(&state, &navigate));
    }
    window.add_action(&action_up);
    app.set_accels_for_action("win.go-up", &["<Alt>Up"]);

    let action_refresh = gio::SimpleAction::new("refresh", None);
    {
        let state = state.clone();
        let chrome = chrome.clone();
        action_refresh.connect_activate(move |_, _| refresh(&state, &chrome));
    }
    window.add_action(&action_refresh);
    app.set_accels_for_action("win.refresh", &["F5"]);

    let action_focus_address = gio::SimpleAction::new("focus-address", None);
    {
        let chrome = chrome.clone();
        let address_entry = address_entry.clone();
        action_focus_address.connect_activate(move |_, _| {
            chrome.title_stack.set_visible_child_name("address");
            address_entry.grab_focus();
            address_entry.select_region(0, -1);
        });
    }
    window.add_action(&action_focus_address);
    app.set_accels_for_action("win.focus-address", &["<Primary>l"]);
}

/// Registers the Copy/Cut/Paste/Trash/Delete `win.*` actions and their
/// accelerators. `Delete` only ever trashes (Rule #39: permanent delete
/// must never be one accidental keypress) — permanent delete is
/// `<Shift>Delete`, and always goes through `dialogs::delete_confirm`
/// first (Rule #38).
#[allow(clippy::too_many_arguments)]
fn setup_operation_actions(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    state: &SharedState,
    chrome: &Chrome,
    progress: &ProgressToastHandles,
    view_stack: &gtk4::Stack,
    selections: &ViewSelections,
    clipboard: &Rc<RefCell<Option<ClipboardEntry>>>,
) {
    let action_copy = gio::SimpleAction::new("copy-selection", None);
    {
        let view_stack = view_stack.clone();
        let selections = selections.clone();
        let clipboard = clipboard.clone();
        action_copy.connect_activate(move |_, _| {
            if let Some(item) = selections.selected(&view_stack) {
                *clipboard.borrow_mut() = Some(ClipboardEntry {
                    path: item.path,
                    cut: false,
                });
            }
        });
    }
    window.add_action(&action_copy);
    app.set_accels_for_action("win.copy-selection", &["<Primary>c"]);

    let action_cut = gio::SimpleAction::new("cut-selection", None);
    {
        let view_stack = view_stack.clone();
        let selections = selections.clone();
        let clipboard = clipboard.clone();
        action_cut.connect_activate(move |_, _| {
            if let Some(item) = selections.selected(&view_stack) {
                *clipboard.borrow_mut() = Some(ClipboardEntry {
                    path: item.path,
                    cut: true,
                });
            }
        });
    }
    window.add_action(&action_cut);
    app.set_accels_for_action("win.cut-selection", &["<Primary>x"]);

    let action_paste = gio::SimpleAction::new("paste", None);
    {
        let window = window.clone();
        let state = state.clone();
        let chrome = chrome.clone();
        let progress = progress.clone();
        let clipboard = clipboard.clone();
        action_paste.connect_activate(move |_, _| {
            let Some(entry) = clipboard.borrow_mut().take() else {
                return;
            };
            let destination = state.borrow().current_dir.clone();
            let kind = if entry.cut {
                OperationKind::Move
            } else {
                OperationKind::Copy
            };
            run_bulk_operation(
                &window,
                &state,
                &chrome,
                &progress,
                kind,
                vec![entry.path],
                Some(destination),
            );
        });
    }
    window.add_action(&action_paste);
    app.set_accels_for_action("win.paste", &["<Primary>v"]);

    let action_trash = gio::SimpleAction::new("trash-selection", None);
    {
        let window = window.clone();
        let state = state.clone();
        let chrome = chrome.clone();
        let progress = progress.clone();
        let view_stack = view_stack.clone();
        let selections = selections.clone();
        action_trash.connect_activate(move |_, _| {
            if let Some(item) = selections.selected(&view_stack) {
                run_bulk_operation(
                    &window,
                    &state,
                    &chrome,
                    &progress,
                    OperationKind::Trash,
                    vec![item.path],
                    None,
                );
            }
        });
    }
    window.add_action(&action_trash);
    app.set_accels_for_action("win.trash-selection", &["Delete"]);

    let action_delete = gio::SimpleAction::new("delete-selection", None);
    {
        let window = window.clone();
        let state = state.clone();
        let chrome = chrome.clone();
        let progress = progress.clone();
        let view_stack = view_stack.clone();
        let selections = selections.clone();
        action_delete.connect_activate(move |_, _| {
            let Some(item) = selections.selected(&view_stack) else {
                return;
            };
            let window_for_confirm = window.clone();
            let state = state.clone();
            let chrome = chrome.clone();
            let progress = progress.clone();
            let path = item.path;
            let path_for_delete = path.clone();
            dialogs::delete_confirm::show(&window, std::slice::from_ref(&path), move || {
                run_bulk_operation(
                    &window_for_confirm,
                    &state,
                    &chrome,
                    &progress,
                    OperationKind::Delete,
                    vec![path_for_delete],
                    None,
                );
            });
        });
    }
    window.add_action(&action_delete);
    app.set_accels_for_action("win.delete-selection", &["<Shift>Delete"]);
}

/// Registers the Faz 6 context-menu `win.*` actions: item actions (Open,
/// Open With…, Open in New Window, Rename, Copy Path, Copy Location) and
/// background actions (New Folder, New Document), plus the shared
/// `win.not-implemented` disabled action every not-yet-built entry
/// (Compress/Extract/Open Terminal Here/Properties/Open in New Tab) binds
/// to. Copy/Cut/Paste/Trash/Delete are already registered by
/// `setup_operation_actions` and are reused as-is by the context menus.
#[allow(clippy::too_many_arguments)]
fn setup_context_menu_actions(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    state: &SharedState,
    chrome: &Chrome,
    view_stack: &gtk4::Stack,
    selections: &ViewSelections,
    on_open: &Rc<dyn Fn(FileItem)>,
) {
    let action_not_implemented = gio::SimpleAction::new("not-implemented", None);
    action_not_implemented.set_enabled(false);
    window.add_action(&action_not_implemented);

    let action_open = gio::SimpleAction::new("open-selected", None);
    {
        let view_stack = view_stack.clone();
        let selections = selections.clone();
        let on_open = on_open.clone();
        action_open.connect_activate(move |_, _| {
            if let Some(item) = selections.selected(&view_stack) {
                on_open(item);
            }
        });
    }
    window.add_action(&action_open);

    let action_open_with = gio::SimpleAction::new("open-with-selected", None);
    {
        let window = window.clone();
        let view_stack = view_stack.clone();
        let selections = selections.clone();
        action_open_with.connect_activate(move |_, _| {
            let Some(item) = selections.selected(&view_stack) else {
                return;
            };
            show_open_with_dialog(&window, &item.path);
        });
    }
    window.add_action(&action_open_with);

    let action_open_new_window = gio::SimpleAction::new("open-in-new-window-selected", None);
    {
        let view_stack = view_stack.clone();
        let selections = selections.clone();
        action_open_new_window.connect_activate(move |_, _| {
            if let Some(item) = selections.selected(&view_stack) {
                if item.kind().is_directory() {
                    spawn_new_window(&item.path);
                }
            }
        });
    }
    window.add_action(&action_open_new_window);

    let action_rename = gio::SimpleAction::new("rename-selected", None);
    {
        let window = window.clone();
        let state = state.clone();
        let chrome = chrome.clone();
        let view_stack = view_stack.clone();
        let selections = selections.clone();
        action_rename.connect_activate(move |_, _| {
            let Some(item) = selections.selected(&view_stack) else {
                return;
            };
            let current_name = item.name().to_string();
            let path = item.path.clone();
            let state = state.clone();
            let chrome = chrome.clone();
            let previous_name = current_name.clone();
            dialogs::rename_dialog::show(&window, &current_name, move |new_name| {
                if new_name.is_empty() || new_name == previous_name {
                    return;
                }
                let state = state.clone();
                let chrome = chrome.clone();
                fs_async::run_blocking(
                    move || veyra_filesystem::rename(&path, &new_name),
                    move |result| match result {
                        Ok(_) => refresh(&state, &chrome),
                        Err(err) => {
                            tracing::warn!(error = %err, "rename failed");
                            chrome
                                .status_left
                                .set_label(&format!("Rename failed: {err}"));
                        }
                    },
                );
            });
        });
    }
    window.add_action(&action_rename);
    app.set_accels_for_action("win.rename-selected", &["F2"]);

    let action_copy_path = gio::SimpleAction::new("copy-path-selected", None);
    {
        let window = window.clone();
        let view_stack = view_stack.clone();
        let selections = selections.clone();
        action_copy_path.connect_activate(move |_, _| {
            if let Some(item) = selections.selected(&view_stack) {
                window.clipboard().set_text(&item.path.to_string());
            }
        });
    }
    window.add_action(&action_copy_path);

    let action_copy_location = gio::SimpleAction::new("copy-location-selected", None);
    {
        let window = window.clone();
        let view_stack = view_stack.clone();
        let selections = selections.clone();
        action_copy_location.connect_activate(move |_, _| {
            if let Some(item) = selections.selected(&view_stack) {
                window.clipboard().set_text(&parent_display(&item.path));
            }
        });
    }
    window.add_action(&action_copy_location);

    let action_create_folder = gio::SimpleAction::new("create-folder", None);
    {
        let state = state.clone();
        let chrome = chrome.clone();
        action_create_folder.connect_activate(move |_, _| {
            create_child_entry(&state, &chrome, "New Folder", true);
        });
    }
    window.add_action(&action_create_folder);

    let action_create_document = gio::SimpleAction::new("create-document", None);
    {
        let state = state.clone();
        let chrome = chrome.clone();
        action_create_document.connect_activate(move |_, _| {
            create_child_entry(&state, &chrome, "New Document", false);
        });
    }
    window.add_action(&action_create_document);
}

/// Creates a new folder or empty file named `base_name` (auto-suffixed with
/// `(2)`, `(3)`, ... on collision, per `veyra_filesystem::suggest_name`)
/// inside the current directory, then refreshes the listing.
fn create_child_entry(state: &SharedState, chrome: &Chrome, base_name: &str, is_dir: bool) {
    let dir = state.borrow().current_dir.clone();
    let name = unique_child_name(&dir, base_name);
    let path = child_path(&dir, &name);

    let state = state.clone();
    let chrome = chrome.clone();
    fs_async::run_blocking(
        move || {
            if is_dir {
                veyra_filesystem::create_dir(&path)
            } else {
                veyra_filesystem::create_file(&path)
            }
        },
        move |result| match result {
            Ok(()) => refresh(&state, &chrome),
            Err(err) => {
                tracing::warn!(error = %err, "failed to create new entry");
                chrome
                    .status_left
                    .set_label(&format!("Create failed: {err}"));
            }
        },
    );
}

/// `base_name` if free in `dir`, otherwise the first free `base_name (N)`
/// variant. A quick single `query_exists` stat per candidate on the GTK main
/// thread — the same trade-off `dialogs::conflict_dialog::sibling_exists`
/// already makes for the same reason (negligible cost, no async round trip
/// needed just to name a new folder).
fn unique_child_name(dir: &VeyraPath, base_name: &str) -> String {
    let exists = |candidate: &str| {
        child_path(dir, candidate)
            .to_gio_file()
            .query_exists(gio::Cancellable::NONE)
    };
    if !exists(base_name) {
        base_name.to_string()
    } else {
        veyra_filesystem::suggest_name(base_name, exists)
    }
}

fn child_path(dir: &VeyraPath, name: &str) -> VeyraPath {
    match dir {
        VeyraPath::Local(path) => VeyraPath::from_local(path.join(name)),
        VeyraPath::Uri(uri) => VeyraPath::from_uri(format!("{}/{name}", uri.trim_end_matches('/'))),
    }
}

/// The containing directory of `path`, as a display string (falls back to
/// `path` itself if it has no parent, e.g. the filesystem root).
fn parent_display(path: &VeyraPath) -> String {
    match path {
        VeyraPath::Local(local) => local
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| path.to_string()),
        VeyraPath::Uri(uri) => uri
            .trim_end_matches('/')
            .rsplit_once('/')
            .map(|(parent, _)| parent.to_string())
            .unwrap_or_else(|| path.to_string()),
    }
}

/// Shows the system application-chooser dialog for `path` and launches the
/// chosen application on confirmation. A single `GAppInfo` launch (a fork +
/// D-Bus activation, not filesystem I/O) is fast enough to run directly on
/// the GTK main thread, unlike the bulk Copy/Move/Trash/Delete operations.
fn show_open_with_dialog(window: &adw::ApplicationWindow, path: &VeyraPath) {
    let file = path.to_gio_file();
    let dialog = gtk4::AppChooserDialog::new(Some(window), gtk4::DialogFlags::MODAL, &file);
    dialog.connect_response(move |dialog, response| {
        if response == gtk4::ResponseType::Ok {
            if let Some(app_info) = dialog.app_info() {
                if let Err(err) =
                    app_info.launch(std::slice::from_ref(&file), gio::AppLaunchContext::NONE)
                {
                    tracing::warn!(error = %err, "failed to launch chosen application");
                }
            }
        }
        dialog.close();
    });
    dialog.present();
}

/// Relaunches the Veyra binary pointed at `path`, standing in for "Open in
/// New Window" until Faz 7 gives real in-process tabs/windows sharing one
/// app instance's state.
fn spawn_new_window(path: &VeyraPath) {
    let Ok(exe) = std::env::current_exe() else {
        tracing::warn!("failed to resolve current executable for new window");
        return;
    };
    if let Err(err) = std::process::Command::new(exe)
        .arg(path.to_string())
        .spawn()
    {
        tracing::warn!(error = %err, "failed to open new window");
    }
}

/// Starts `request` on a background thread and drives its event stream:
/// progress updates the bottom progress bar, conflicts open the modal
/// resolution dialog, and completion refreshes the current directory
/// listing (files changed on disk) and surfaces any errors on the status
/// bar.
fn run_bulk_operation(
    window: &adw::ApplicationWindow,
    state: &SharedState,
    chrome: &Chrome,
    progress: &ProgressToastHandles,
    kind: OperationKind,
    sources: Vec<VeyraPath>,
    destination: Option<VeyraPath>,
) {
    if sources.is_empty() {
        return;
    }

    let request = OperationRequest {
        kind,
        sources,
        destination,
    };
    let (control, receiver) = operations::spawn(request);
    widgets::progress_toast::begin(progress, &control, kind);

    let window = window.clone();
    let state = state.clone();
    let chrome = chrome.clone();
    let progress = progress.clone();
    glib::spawn_future_local(async move {
        while let Ok(event) = receiver.recv().await {
            match event {
                OperationEvent::Progress(p) => widgets::progress_toast::update(&progress, &p),
                OperationEvent::Conflict(conflict, answer_tx) => {
                    dialogs::conflict_dialog::show(&window, &conflict, move |decision| {
                        let _ = answer_tx.send_blocking(decision);
                    });
                }
                OperationEvent::Done(outcome) => {
                    widgets::progress_toast::finish(&progress);
                    refresh(&state, &chrome);
                    if !outcome.errors.is_empty() {
                        for (path, err) in &outcome.errors {
                            tracing::warn!(path = %path, error = %err, "bulk operation error");
                        }
                        chrome.status_left.set_label(&format!(
                            "{} error(s) during operation",
                            outcome.errors.len()
                        ));
                    }
                    break;
                }
            }
        }
    });
}

/// Navigates to the current directory's parent, if any (no-op at the
/// filesystem root).
fn go_up(state: &SharedState, navigate: &Rc<dyn Fn(VeyraPath)>) {
    let parent = state
        .borrow()
        .current_dir
        .as_local_path()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf());
    if let Some(parent) = parent {
        navigate(VeyraPath::from_local(parent));
    }
}

/// Re-reads the current directory without touching navigation history.
fn refresh(state: &SharedState, chrome: &Chrome) {
    let path = state.borrow().current_dir.clone();
    load_directory(state, chrome, path);
}

fn build_search_filter(state: &SharedState, query: Rc<RefCell<String>>) -> gtk4::CustomFilter {
    let state = state.clone();
    gtk4::CustomFilter::new(move |object| {
        let needle = query.borrow();
        if needle.is_empty() {
            return true;
        }
        let Some(boxed) = object.downcast_ref::<gtk4::glib::BoxedAnyObject>() else {
            return true;
        };
        let item = boxed.borrow::<FileItem>();
        let _ = &state; // reserved for future scope (e.g. content search)
        item.name().to_lowercase().contains(&needle.to_lowercase())
    })
}

/// Navigates to `path`, pushing the previous location onto the back stack
/// when `push_history` is true (false is used for the initial load and for
/// back/forward navigation, which manage the stacks themselves).
fn navigate_to(state: &SharedState, chrome: &Chrome, path: VeyraPath, push_history: bool) {
    {
        let mut state_mut = state.borrow_mut();
        if push_history {
            let previous = state_mut.current_dir.clone();
            state_mut.history.record(previous);
        }
        state_mut.current_dir = path.clone();
    }

    update_chrome(state, chrome);
    load_directory(state, chrome, path);
}

fn go_back(state: &SharedState, chrome: &Chrome) {
    let target = {
        let mut state_mut = state.borrow_mut();
        let current = state_mut.current_dir.clone();
        let Some(previous) = state_mut.history.go_back(current) else {
            return;
        };
        state_mut.current_dir = previous.clone();
        previous
    };
    update_chrome(state, chrome);
    load_directory(state, chrome, target);
}

fn go_forward(state: &SharedState, chrome: &Chrome) {
    let target = {
        let mut state_mut = state.borrow_mut();
        let current = state_mut.current_dir.clone();
        let Some(next) = state_mut.history.go_forward(current) else {
            return;
        };
        state_mut.current_dir = next.clone();
        next
    };
    update_chrome(state, chrome);
    load_directory(state, chrome, target);
}

fn update_chrome(state: &SharedState, chrome: &Chrome) {
    let state_ref = state.borrow();
    chrome.back_button.set_sensitive(state_ref.can_go_back());
    chrome
        .forward_button
        .set_sensitive(state_ref.can_go_forward());
    chrome.up_button.set_sensitive(state_ref.can_go_up());
    chrome
        .address_entry
        .set_text(&state_ref.current_dir.to_string());

    let navigate: Rc<dyn Fn(VeyraPath)> = {
        let state = state.clone();
        let chrome = chrome.clone();
        Rc::new(move |path: VeyraPath| navigate_to(&state, &chrome, path, true))
    };
    breadcrumbs::rebuild(&chrome.breadcrumbs_box, &state_ref.current_dir, navigate);
}

fn load_directory(state: &SharedState, chrome: &Chrome, path: VeyraPath) {
    chrome.status_left.set_label("Loading…");

    let state_for_done = state.clone();
    let chrome_for_done = chrome.clone();
    fs_async::run_blocking(
        move || veyra_filesystem::read_dir(&path),
        move |result| on_directory_loaded(&state_for_done, &chrome_for_done, result),
    );
}

fn on_directory_loaded(
    state: &SharedState,
    chrome: &Chrome,
    result: Result<Vec<FileItem>, veyra_filesystem::FsError>,
) {
    match result {
        Ok(items) => {
            let count = items.len();
            let model = state.borrow().model.clone();
            model.remove_all();
            for item in items {
                model.append(&gtk4::glib::BoxedAnyObject::new(item));
            }
            let label = if count == 1 {
                "1 item".to_string()
            } else {
                format!("{count} items")
            };
            chrome.status_left.set_label(&label);
            update_free_space(state, chrome);
        }
        Err(err) => {
            tracing::warn!(error = %err, "failed to read directory");
            chrome.status_left.set_label(&format!("Error: {err}"));
            chrome.status_right.set_label("");
        }
    }
}

fn update_free_space(state: &SharedState, chrome: &Chrome) {
    let path = state.borrow().current_dir.clone();
    let chrome = chrome.clone();
    fs_async::run_blocking(
        move || query_free_space(&path),
        move |free_space| {
            chrome
                .status_right
                .set_label(&free_space.unwrap_or_default());
        },
    );
}

fn query_free_space(path: &VeyraPath) -> Option<String> {
    let info = path
        .to_gio_file()
        .query_filesystem_info("filesystem::free", gio::Cancellable::NONE)
        .ok()?;
    if !info.has_attribute("filesystem::free") {
        return None;
    }
    let free_bytes = info.attribute_uint64("filesystem::free");
    Some(format!(
        "{} free",
        veyra_filesystem::format_size(free_bytes)
    ))
}

/// In development builds, add `data/icons` (relative to the workspace root)
/// to the icon theme search path so `io.github.erayq1.Veyra` and any other
/// bundled icons resolve without an `install`ed data directory.
fn add_dev_icon_search_path(window: &adw::ApplicationWindow) {
    if !cfg!(debug_assertions) {
        return;
    }
    let display = gtk4::prelude::WidgetExt::display(window);
    let icon_theme = gtk4::IconTheme::for_display(&display);
    let dev_icons_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/icons");
    if dev_icons_dir.is_dir() {
        icon_theme.add_search_path(&dev_icons_dir);
    }
}
