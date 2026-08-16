use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use veyra_filesystem::{FileItem, OperationControl, OperationKind, OperationRequest, VeyraPath};
use veyra_search::SearchIndex;

use crate::dnd;
use crate::operations::OperationEvent;
use crate::preview::{self, PreviewPanelHandles};
use crate::split_view::{self, Chrome, Panel, PanelId, Panels};
use crate::state::{AppState, SharedState};
use crate::tab_page::{active_tab, TabPage, TabRegistry, ViewSelections};
use crate::undo::{SharedUndoStack, UndoStack, UndoableAction};
use crate::views::ViewMode;
use crate::widgets::progress_toast::ProgressToastHandles;
use crate::{
    archive_ops, breadcrumbs, dialogs, fs_async, headerbar, open_with, operations, recent,
    shortcuts, sidebar, terminal, trash, undo, widgets,
};

/// A single Copy/Cut clipboard slot, shared across both panels and all their
/// tabs. Holds every path from the selection active when Copy/Cut ran (Ara
/// Faz: Multi-Selection) — a single-item selection just produces a
/// one-element `paths`. `cut` decides whether `win.paste` runs a Move or a
/// Copy.
#[derive(Clone)]
struct ClipboardEntry {
    paths: Vec<VeyraPath>,
    cut: bool,
}

pub(crate) fn build_window(
    app: &adw::Application,
    start_dir: VeyraPath,
    cache_dir: &Path,
    settings: crate::config::SharedSettings,
) -> adw::ApplicationWindow {
    // Faz 15: app-wide privacy toggle, shared by both panels' Recent Files
    // banners and by `open_item` (which reads it before deciding whether to
    // record an opened file into the XDG recent-files registry).
    let privacy_mode: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));

    let left = split_view::build_panel(PanelId::Left, privacy_mode.clone(), settings.clone());
    let right = split_view::build_panel(PanelId::Right, privacy_mode.clone(), settings.clone());
    right.frame.set_visible(false);
    let panels = Panels { left, right };

    let focused: Rc<RefCell<PanelId>> = Rc::new(RefCell::new(PanelId::Left));

    let clipboard: Rc<RefCell<Option<ClipboardEntry>>> = Rc::new(RefCell::new(None));
    let undo_stack: SharedUndoStack = Rc::new(RefCell::new(UndoStack::new()));
    let has_clipboard: Rc<dyn Fn() -> bool> = {
        let clipboard = clipboard.clone();
        Rc::new(move || clipboard.borrow().is_some())
    };
    let split_active: Rc<dyn Fn() -> bool> = {
        let right_frame = panels.right.frame.clone();
        Rc::new(move || right_frame.is_visible())
    };

    let navigate: Rc<dyn Fn(VeyraPath)> = {
        let panels = panels.clone();
        let focused = focused.clone();
        Rc::new(move |path: VeyraPath| navigate_focused(&panels, &focused, path))
    };

    let preview = preview::build(settings.clone());
    let refresh_preview: Rc<dyn Fn()> = {
        let panels = panels.clone();
        let focused = focused.clone();
        let preview = preview.clone();
        Rc::new(move || sync_preview(&panels, &focused, &preview))
    };

    let search_index = Arc::new(open_search_index(cache_dir));
    if settings.borrow().enable_fts_index {
        veyra_search::spawn_background_index(search_index.clone(), glib::home_dir());
    }
    // Faz 34: the Preferences dialog's "Rebuild Search Index" button (and
    // re-enabling "Enable Fast Search Indexer" after it was off) both just
    // need to kick off another one-shot walk — `spawn_background_index`
    // already re-indexes any path it revisits (`INSERT OR REPLACE`), so
    // running it again is always safe, not just on first launch.
    let rebuild_search_index: Rc<dyn Fn()> = {
        let search_index = search_index.clone();
        Rc::new(move || {
            veyra_search::spawn_background_index(search_index.clone(), glib::home_dir());
        })
    };

    let thumbnails = crate::thumbnails::ThumbnailService::new(
        cache_dir.join("thumbnails"),
        settings.borrow().thumbnail_cache_capacity,
    );

    let header = headerbar::build(
        &panels,
        focused.clone(),
        search_index,
        navigate.clone(),
        refresh_preview.clone(),
        settings.clone(),
    );
    let progress = widgets::progress_toast::build();

    // Faz 26: the window is built here (ahead of the sidebar/content that
    // normally motivate constructing it) purely so `dnd_execute` — and
    // therefore the very first tab's `DndWiring` — has a parented window to
    // drive `run_bulk_operation`/`create_links`/error dialogs with. It is
    // filled in with `.content()` further below, once the sidebar (which
    // itself needs this same `window` reference) is built.
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Veyra - Modern Linux File Manager")
        .default_width(1200)
        .default_height(720)
        .build();
    let dnd_execute = build_dnd_executor(&window, &panels, &progress, &undo_stack);

    wire_panel(&panels.left, &focused, &header, &refresh_preview);
    wire_panel(&panels.right, &focused, &header, &refresh_preview);
    attach_focus_gesture(&panels.left, &panels, &focused, &header, &refresh_preview);
    attach_focus_gesture(&panels.right, &panels, &focused, &header, &refresh_preview);
    attach_panel_focus_key(&panels.left, &panels, &focused, &header, &refresh_preview);
    attach_panel_focus_key(&panels.right, &panels, &focused, &header, &refresh_preview);

    // Faz 34: "Restore Previous Tabs on Startup" — a non-empty saved session
    // (from the last `window.connect_close_request` below) replaces the
    // usual single `start_dir` tab with one tab per saved path, per panel;
    // an empty/missing session (first launch, or the toggle was off last
    // time) falls back to today's single-tab behavior unchanged.
    let restored_session = if settings.borrow().restore_tabs_on_startup {
        Some(crate::session::Session::load())
    } else {
        None
    };
    let restored_left = restored_session
        .as_ref()
        .map(crate::session::Session::left_paths)
        .filter(|paths| !paths.is_empty());
    let restored_right = restored_session
        .as_ref()
        .map(crate::session::Session::right_paths)
        .filter(|paths| !paths.is_empty());

    for path in restored_left.unwrap_or_else(|| vec![start_dir]) {
        open_tab(
            &panels.left.tab_view,
            &panels.left.registry,
            &panels.left.chrome,
            has_clipboard.clone(),
            split_active.clone(),
            refresh_preview.clone(),
            thumbnails.clone(),
            dnd_execute.clone(),
            path,
        );
    }
    if let Some(right_paths) = restored_right {
        for path in right_paths {
            open_tab(
                &panels.right.tab_view,
                &panels.right.registry,
                &panels.right.chrome,
                has_clipboard.clone(),
                split_active.clone(),
                refresh_preview.clone(),
                thumbnails.clone(),
                dnd_execute.clone(),
                path,
            );
        }
        panels.right.frame.set_visible(true);
    }
    if let Some(tab) = active_tab(&panels.left.tab_view, &panels.left.registry) {
        sync_view_switcher(&header.view_switcher_buttons, &tab);
    }
    set_active_panel_visuals(&panels, PanelId::Left, false);

    let paned = gtk4::Paned::new(gtk4::Orientation::Horizontal);
    paned.set_start_child(Some(&panels.left.frame));
    paned.set_end_child(Some(&panels.right.frame));
    paned.set_resize_start_child(true);
    paned.set_resize_end_child(true);
    paned.set_shrink_start_child(false);
    paned.set_shrink_end_child(false);
    paned.set_position(640);

    preview
        .widget
        .set_visible(settings.borrow().enable_preview_panel);
    let content_paned = gtk4::Paned::new(gtk4::Orientation::Horizontal);
    content_paned.set_start_child(Some(&paned));
    content_paned.set_end_child(Some(&preview.widget));
    content_paned.set_resize_start_child(true);
    content_paned.set_resize_end_child(false);
    content_paned.set_shrink_start_child(false);
    content_paned.set_shrink_end_child(true);
    content_paned.set_position(880);

    // `window` was already built above (see the Faz 26 note by
    // `dnd_execute`), without `.content()` yet: Faz 16's Bookmarks sidebar
    // section needs a window reference (to parent its Rename Bookmark
    // dialog) just like `dnd_execute` does, so `set_content` only fills it
    // in below, once the sidebar and content panes exist.
    let open_in_new_tab: Rc<dyn Fn(VeyraPath)> = {
        let panels = panels.clone();
        let focused = focused.clone();
        let has_clipboard = has_clipboard.clone();
        let split_active = split_active.clone();
        let refresh_preview = refresh_preview.clone();
        let thumbnails = thumbnails.clone();
        let dnd_execute = dnd_execute.clone();
        Rc::new(move |path: VeyraPath| {
            let panel = panels.get(*focused.borrow()).clone();
            open_tab(
                &panel.tab_view,
                &panel.registry,
                &panel.chrome,
                has_clipboard.clone(),
                split_active.clone(),
                refresh_preview.clone(),
                thumbnails.clone(),
                dnd_execute.clone(),
                path,
            );
        })
    };

    let content_page = adw::NavigationPage::new(&content_paned, "Files");
    let sidebar_widget = sidebar::build(
        &window,
        navigate.clone(),
        open_in_new_tab,
        thumbnails.clone(),
        dnd_execute.clone(),
    );
    let sidebar_page = adw::NavigationPage::new(&sidebar_widget, "Sidebar");

    let sidebar_split = adw::NavigationSplitView::builder()
        .sidebar(&sidebar_page)
        .content(&content_page)
        .sidebar_width_fraction(0.22)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header.widget);
    toolbar_view.add_bottom_bar(&progress.widget);
    toolbar_view.set_content(Some(&sidebar_split));
    window.set_content(Some(&toolbar_view));

    add_dev_icon_search_path(&window);
    if let Some(app_id) = app.application_id() {
        window.set_icon_name(Some(&app_id));
    }
    split_view::install_panel_css(&gtk4::prelude::WidgetExt::display(&window));

    setup_navigation_shortcuts(app, &window, &panels, &focused);
    setup_search_shortcut(app, &window, &header);
    setup_tab_actions(
        app,
        &window,
        &panels,
        &focused,
        has_clipboard.clone(),
        split_active.clone(),
        refresh_preview.clone(),
        thumbnails.clone(),
        dnd_execute.clone(),
    );
    setup_split_view_actions(
        app,
        &window,
        &panels,
        &focused,
        &header,
        &progress,
        has_clipboard.clone(),
        refresh_preview.clone(),
        thumbnails.clone(),
        dnd_execute.clone(),
        &undo_stack,
    );
    setup_operation_actions(
        app,
        &window,
        &panels,
        &focused,
        &progress,
        &clipboard,
        &undo_stack,
    );
    setup_archive_actions(&window, &panels, &focused, &progress);
    setup_context_menu_actions(
        app,
        &window,
        &panels,
        &focused,
        has_clipboard,
        split_active,
        refresh_preview.clone(),
        thumbnails.clone(),
        dnd_execute.clone(),
        &undo_stack,
    );
    setup_properties_actions(app, &window, &panels, &focused, thumbnails.clone());
    setup_preview_actions(app, &window, &preview, &header, refresh_preview.clone());
    setup_recent_actions(&window, &panels, privacy_mode);
    setup_trash_actions(app, &window, &panels, &focused, &undo_stack);
    setup_undo_actions(app, &window, &panels, &undo_stack);
    setup_disk_analyzer_actions(app, &window, &panels, &focused, navigate.clone());
    setup_terminal_actions(app, &window, &panels, &focused);
    setup_terminal_as_root_actions(app, &window, &panels, &focused);
    setup_network_actions(&window, navigate);
    setup_command_palette_actions(app, &window, &panels, &focused, &header, refresh_preview);
    setup_file_associations_action(&window);

    // Faz 25: loaded/applied last so a user's `~/.config/veyra/shortcuts.json`
    // customization (or, absent one, `default_shortcuts()`, which mirrors
    // every `set_accels_for_action` call above) is the final word on every
    // action's accelerator.
    let shortcut_map: Rc<RefCell<shortcuts::ShortcutMap>> =
        Rc::new(RefCell::new(shortcuts::ShortcutMap::load()));
    shortcut_map.borrow().apply_to_application(app);
    setup_shortcuts_actions(app, &window, shortcut_map);

    setup_preferences_actions(
        app,
        &window,
        &panels,
        &preview,
        thumbnails.clone(),
        rebuild_search_index,
        settings.clone(),
    );

    // Faz 34: "Restore Previous Tabs on Startup" is only useful if a session
    // actually gets written somewhere — save on every close regardless of
    // whether the toggle is currently on, so turning it on later has real
    // data to restore instead of nothing.
    {
        let panels = panels.clone();
        window.connect_close_request(move |_| {
            let session = crate::session::Session {
                left_tabs: collect_tab_paths(&panels.left),
                right_tabs: collect_tab_paths(&panels.right),
            };
            if let Err(err) = session.save() {
                tracing::warn!(error = %err, "failed to save tab session");
            }
            glib::Propagation::Proceed
        });
    }

    window
}

/// Registers the Faz 25 `win.show-shortcuts-help` (`Ctrl+?`) and
/// `win.reset-shortcuts` actions. Resetting writes `default_shortcuts()`
/// back to `~/.config/veyra/shortcuts.json`, re-applies it to `app`, and
/// updates the shared `shortcut_map` so any UI reading it afterwards (the
/// help window, a future shortcut editor) sees the reset immediately.
fn setup_shortcuts_actions(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    shortcut_map: Rc<RefCell<shortcuts::ShortcutMap>>,
) {
    let action_show_help = gio::SimpleAction::new("show-shortcuts-help", None);
    {
        let window = window.clone();
        action_show_help.connect_activate(move |_, _| {
            dialogs::shortcuts_help_dialog::show(&window);
        });
    }
    // Its accelerator comes from `shortcut_map.apply_to_application`
    // (called just before this function), not a hardcoded call here — it's
    // already in `default_shortcuts()`, and a second hardcoded call here
    // would silently clobber a user's customization for this one action.
    window.add_action(&action_show_help);

    let action_reset = gio::SimpleAction::new("reset-shortcuts", None);
    {
        let app = app.clone();
        action_reset.connect_activate(move |_, _| {
            let defaults = shortcuts::default_shortcuts();
            if let Err(err) = defaults.save() {
                tracing::warn!(error = %err, "failed to persist reset shortcuts.json");
            }
            defaults.apply_to_application(&app);
            *shortcut_map.borrow_mut() = defaults;
        });
    }
    window.add_action(&action_reset);
}

/// Registers the Faz 34 `win.show-preferences` action (`Ctrl+,`), opening
/// the Preferences dialog. Everything the dialog can change live (theme,
/// icon size, click policy, thumbnail cache capacity, search index) is
/// handed in as a callback or shared handle rather than the dialog reaching
/// back into `window.rs` internals directly.
fn setup_preferences_actions(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    panels: &Panels,
    preview: &PreviewPanelHandles,
    thumbnails: Rc<crate::thumbnails::ThumbnailService>,
    rebuild_search_index: Rc<dyn Fn()>,
    settings: crate::config::SharedSettings,
) {
    let action = gio::SimpleAction::new("show-preferences", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let preview_widget = preview.widget.clone();
        action.connect_activate(move |_, _| {
            dialogs::preferences_dialog::show(
                &window,
                settings.clone(),
                thumbnails.clone(),
                rebuild_search_index.clone(),
                {
                    let panels = panels.clone();
                    Rc::new(move || refresh_all_tabs(&panels))
                },
                preview_widget.clone(),
            );
        });
    }
    window.add_action(&action);
    app.set_accels_for_action("win.show-preferences", &["<Primary>comma"]);
}

/// Registers the Faz 27 `win.manage-file-associations` action, which opens
/// the File Associations dialog listing all registered MIME types and their
/// default handlers.
fn setup_file_associations_action(window: &adw::ApplicationWindow) {
    let action = gio::SimpleAction::new("manage-file-associations", None);
    {
        let window = window.clone();
        action.connect_activate(move |_, _| {
            dialogs::file_associations_dialog::show(&window);
        });
    }
    window.add_action(&action);
}

/// Wires a panel's own close/detach/tab-switch bookkeeping and its five
/// navigation buttons + address entry — all of it scoped to `panel` alone,
/// since a panel's back/forward/up/home/refresh always act on that panel
/// regardless of which one currently has focus.
fn wire_panel(
    panel: &Panel,
    focused: &Rc<RefCell<PanelId>>,
    header: &headerbar::HeaderBarHandles,
    refresh_preview: &Rc<dyn Fn()>,
) {
    // Vetoes closing a panel's last remaining tab (Ctrl+W and the tab's own
    // "x" both route through this signal), so each panel always keeps at
    // least one tab open; closing any other tab proceeds immediately.
    panel.tab_view.connect_close_page(move |view, page| {
        let confirm = view.n_pages() > 1;
        view.close_page_finish(page, confirm);
        glib::Propagation::Stop
    });
    {
        let registry = panel.registry.clone();
        panel
            .tab_view
            .connect_page_detached(move |_, page, _position| {
                registry.borrow_mut().remove(page);
            });
    }
    {
        let panel = panel.clone();
        let focused = focused.clone();
        let header = header.clone();
        let refresh_preview = refresh_preview.clone();
        panel.tab_view.connect_selected_page_notify(move |view| {
            if let Some(tab) = active_tab(view, &panel.registry) {
                update_chrome(&tab, &panel.chrome);
                if *focused.borrow() == panel.id {
                    sync_view_switcher(&header.view_switcher_buttons, &tab);
                }
            }
            refresh_preview();
        });
    }
    {
        let panel = panel.clone();
        panel.chrome.back_button.clone().connect_clicked(move |_| {
            if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
                go_back(&tab, &panel.chrome);
            }
        });
    }
    {
        let panel = panel.clone();
        panel
            .chrome
            .forward_button
            .clone()
            .connect_clicked(move |_| {
                if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
                    go_forward(&tab, &panel.chrome);
                }
            });
    }
    {
        let panel = panel.clone();
        panel.chrome.up_button.clone().connect_clicked(move |_| {
            if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
                go_up(&tab, &panel.chrome);
            }
        });
    }
    {
        let panel = panel.clone();
        panel.chrome.home_button.clone().connect_clicked(move |_| {
            if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
                navigate_to(
                    &tab,
                    &panel.chrome,
                    VeyraPath::from_local(glib::home_dir()),
                    true,
                );
            }
        });
    }
    {
        let panel = panel.clone();
        panel
            .chrome
            .refresh_button
            .clone()
            .connect_clicked(move |_| {
                if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
                    refresh(&tab.state, &panel.chrome);
                }
            });
    }
    {
        let panel = panel.clone();
        panel
            .chrome
            .address_entry
            .clone()
            .connect_activate(move |entry| {
                let text = entry.text();
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
                        navigate_to(
                            &tab,
                            &panel.chrome,
                            VeyraPath::from_local(std::path::PathBuf::from(trimmed)),
                            true,
                        );
                    }
                }
                panel
                    .chrome
                    .title_stack
                    .set_visible_child_name("breadcrumbs");
            });
    }
}

/// Click-anywhere-in-a-panel focus tracking, matching the classic dual-pane
/// file manager convention (Dolphin/Krusader/Total Commander). Uses the
/// capture phase so it never claims the event — child widgets (the file
/// views, buttons, entries) still get the click normally.
fn attach_focus_gesture(
    panel: &Panel,
    panels: &Panels,
    focused: &Rc<RefCell<PanelId>>,
    header: &headerbar::HeaderBarHandles,
    refresh_preview: &Rc<dyn Fn()>,
) {
    let gesture = gtk4::GestureClick::new();
    gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let panel_id = panel.id;
    let panels = panels.clone();
    let focused = focused.clone();
    let header = header.clone();
    let refresh_preview = refresh_preview.clone();
    gesture.connect_pressed(move |_, _, _, _| {
        focus_panel(panel_id, &panels, &focused, &header, &refresh_preview);
    });
    panel.frame.add_controller(gesture);
}

/// `Tab` switches focus to the other panel, but only when the split view is
/// actually showing two panels, and only when the user isn't currently
/// typing in a text field — otherwise `Tab` keeps its normal job of moving
/// focus to the next widget (e.g. out of the address entry, or between
/// dialog buttons). Scoped to each panel's own `frame` (bubble phase)
/// instead of a window-wide accelerator, precisely so it never intercepts
/// `Tab` before a focused entry/dialog gets a chance to handle it.
fn attach_panel_focus_key(
    panel: &Panel,
    panels: &Panels,
    focused: &Rc<RefCell<PanelId>>,
    header: &headerbar::HeaderBarHandles,
    refresh_preview: &Rc<dyn Fn()>,
) {
    let key = gtk4::EventControllerKey::new();
    key.set_propagation_phase(gtk4::PropagationPhase::Bubble);
    let panel_id = panel.id;
    let panels = panels.clone();
    let focused = focused.clone();
    let header = header.clone();
    let refresh_preview = refresh_preview.clone();
    key.connect_key_pressed(move |controller, keyval, _, _| {
        if keyval != gtk4::gdk::Key::Tab || !panels.right.frame.is_visible() {
            return glib::Propagation::Proceed;
        }
        let editing_text = controller
            .widget()
            .and_then(|widget| widget.root())
            .and_then(|root| root.focus())
            .is_some_and(|focus_widget| focus_widget.is::<gtk4::Editable>());
        if editing_text {
            return glib::Propagation::Proceed;
        }
        focus_panel(
            panel_id.other(),
            &panels,
            &focused,
            &header,
            &refresh_preview,
        );
        glib::Propagation::Stop
    });
    panel.frame.add_controller(key);
}

/// Makes `id` the focused panel: updates the active-panel highlight border
/// and syncs the window header's view-mode switcher to that panel's active
/// tab. No-op if `id` is already focused.
fn focus_panel(
    id: PanelId,
    panels: &Panels,
    focused: &Rc<RefCell<PanelId>>,
    header: &headerbar::HeaderBarHandles,
    refresh_preview: &Rc<dyn Fn()>,
) {
    if *focused.borrow() == id {
        return;
    }
    *focused.borrow_mut() = id;
    set_active_panel_visuals(panels, id, panels.right.frame.is_visible());
    if let Some(tab) = split_view::focused_tab(panels, focused) {
        sync_view_switcher(&header.view_switcher_buttons, &tab);
    }
    refresh_preview();
}

/// Rebuilds the preview panel from whichever item is currently selected in
/// the focused panel's active tab (`None` if nothing is selected, or no tab
/// is open yet), showing its empty state. Called any time that answer could
/// have changed: a selection changed, the active view/tab changed, panel
/// focus changed, or the panel toggled visible.
fn sync_preview(panels: &Panels, focused: &Rc<RefCell<PanelId>>, preview: &PreviewPanelHandles) {
    let item = split_view::focused_tab(panels, focused)
        .and_then(|tab| tab.selections.selected(&tab.view_stack));
    preview::show(preview, item);
}

/// The active-panel highlight only makes sense once a second panel exists
/// to contrast against; with the split view off, neither panel is
/// highlighted.
fn set_active_panel_visuals(panels: &Panels, focused_id: PanelId, split_active: bool) {
    panels
        .left
        .set_highlighted(split_active && focused_id == PanelId::Left);
    panels
        .right
        .set_highlighted(split_active && focused_id == PanelId::Right);
}

fn sync_view_switcher(buttons: &[(ViewMode, gtk4::ToggleButton)], tab: &TabPage) {
    let active_name = tab.view_stack.visible_child_name();
    for (mode, button) in buttons {
        let should_be_active = active_name.as_deref() == Some(mode.stack_name());
        if button.is_active() != should_be_active {
            button.set_active(should_be_active);
        }
    }
}

/// Navigates whichever tab is active in the currently focused panel — safe
/// for callers like the sidebar that only ever want to act on "the visible
/// location the user is looking at right now".
fn navigate_focused(panels: &Panels, focused: &Rc<RefCell<PanelId>>, path: VeyraPath) {
    let panel = panels.get(*focused.borrow()).clone();
    if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
        navigate_to(&tab, &panel.chrome, path, true);
    }
}

/// Registers `win.*` actions for the navigation shortcuts and binds their
/// accelerators on `app`. All act on whichever tab is active in the
/// currently focused panel.
/// Opens the Faz 9 search index at `<cache_dir>/search_index.db`, falling
/// back to an ephemeral in-memory index if the on-disk one can't be opened
/// (permissions, disk full, corrupt file, ...). Search staying unavailable
/// is a degraded experience, never a reason to crash the whole application
/// (Rule #15).
fn open_search_index(cache_dir: &Path) -> SearchIndex {
    let db_path = veyra_search::default_db_path(cache_dir);
    match SearchIndex::open(&db_path) {
        Ok(index) => index,
        Err(err) => {
            tracing::warn!(
                path = %db_path.display(),
                error = %err,
                "failed to open on-disk search index, falling back to in-memory"
            );
            SearchIndex::open_in_memory().unwrap_or_else(|err| {
                // An in-memory SQLite connection failing to open means
                // SQLite itself is unusable in this process; there is no
                // sane fallback left, so this is one of the rare spots
                // where surfacing failure loudly (rather than silently
                // disabling search) is the right call.
                panic!("failed to open in-memory search index: {err}")
            })
        }
    }
}

/// Registers `win.toggle-search` (`Ctrl+F`): clicking the header search
/// button and pressing Ctrl+F both toggle the same search bar, per Faz 9
/// requirement A.
fn setup_search_shortcut(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    header: &headerbar::HeaderBarHandles,
) {
    let action_toggle_search = gio::SimpleAction::new("toggle-search", None);
    {
        let search_toggle = header.search_toggle.clone();
        action_toggle_search.connect_activate(move |_, _| {
            search_toggle.set_active(!search_toggle.is_active());
        });
    }
    window.add_action(&action_toggle_search);
    app.set_accels_for_action("win.toggle-search", &["<Primary>f"]);
}

fn setup_navigation_shortcuts(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    panels: &Panels,
    focused: &Rc<RefCell<PanelId>>,
) {
    let action_back = gio::SimpleAction::new("go-back", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        action_back.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
                go_back(&tab, &panel.chrome);
            }
        });
    }
    window.add_action(&action_back);
    app.set_accels_for_action("win.go-back", &["<Alt>Left"]);

    let action_forward = gio::SimpleAction::new("go-forward", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        action_forward.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
                go_forward(&tab, &panel.chrome);
            }
        });
    }
    window.add_action(&action_forward);
    app.set_accels_for_action("win.go-forward", &["<Alt>Right"]);

    let action_up = gio::SimpleAction::new("go-up", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        action_up.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
                go_up(&tab, &panel.chrome);
            }
        });
    }
    window.add_action(&action_up);
    app.set_accels_for_action("win.go-up", &["<Alt>Up"]);

    let action_refresh = gio::SimpleAction::new("refresh", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        action_refresh.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
                refresh(&tab.state, &panel.chrome);
            }
        });
    }
    window.add_action(&action_refresh);
    app.set_accels_for_action("win.refresh", &["F5"]);

    let action_focus_address = gio::SimpleAction::new("focus-address", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        action_focus_address.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            panel.chrome.title_stack.set_visible_child_name("address");
            panel.chrome.address_entry.grab_focus();
            panel.chrome.address_entry.select_region(0, -1);
        });
    }
    window.add_action(&action_focus_address);
    app.set_accels_for_action("win.focus-address", &["<Primary>l"]);

    // Faz 14: hidden files (dotfiles + `.hidden`-listed entries, both
    // already folded into `FileMetadata::is_hidden` by GIO) are shown/hidden
    // per tab, matching Faz 7's isolation (Kural #51).
    let action_toggle_hidden = gio::SimpleAction::new("toggle-hidden-files", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        action_toggle_hidden.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
                let mut show_hidden = tab.show_hidden.borrow_mut();
                *show_hidden = !*show_hidden;
                drop(show_hidden);
                tab.refresh_filter();
            }
        });
    }
    window.add_action(&action_toggle_hidden);
    app.set_accels_for_action("win.toggle-hidden-files", &["<Primary>h"]);

    // Ara Faz (Multi-Selection): every view's selection model is now a
    // `GtkMultiSelection`, so `Ctrl+A` selects every item currently shown
    // in whichever view is active, instead of only ever jumping to the
    // first one.
    let action_select_all = gio::SimpleAction::new("select-all", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        action_select_all.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
                let selection = tab.selections.active(&tab.view_stack);
                selection.select_all();
                if let Some(widget) = tab.view_stack.visible_child() {
                    widget.grab_focus();
                }
            }
        });
    }
    window.add_action(&action_select_all);
    app.set_accels_for_action("win.select-all", &["<Primary>a"]);

    // Ara Faz (Multi-Selection) requirement A.3: `Escape` clears whatever is
    // currently selected in the focused panel's active view.
    let action_deselect_all = gio::SimpleAction::new("deselect-all", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        action_deselect_all.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
                tab.selections.active(&tab.view_stack).unselect_all();
            }
        });
    }
    window.add_action(&action_deselect_all);
    app.set_accels_for_action("win.deselect-all", &["Escape"]);
}

/// Registers the Faz 7 tab-management `win.*` actions and their
/// accelerators: `Ctrl+T` new tab, `Ctrl+W` close active tab, `Ctrl+Tab`/
/// `Ctrl+Shift+Tab` cycle tabs — all scoped to the currently focused panel.
#[allow(clippy::too_many_arguments)]
fn setup_tab_actions(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    panels: &Panels,
    focused: &Rc<RefCell<PanelId>>,
    has_clipboard: Rc<dyn Fn() -> bool>,
    split_active: Rc<dyn Fn() -> bool>,
    refresh_preview: Rc<dyn Fn()>,
    thumbnails: Rc<crate::thumbnails::ThumbnailService>,
    dnd_execute: dnd::DropExecutor,
) {
    let action_new_tab = gio::SimpleAction::new("new-tab", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        let refresh_preview = refresh_preview.clone();
        let thumbnails = thumbnails.clone();
        let dnd_execute = dnd_execute.clone();
        action_new_tab.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let start_dir = active_tab(&panel.tab_view, &panel.registry)
                .map(|tab| tab.state.borrow().current_dir.clone())
                .unwrap_or_else(|| VeyraPath::from_local(glib::home_dir()));
            open_tab(
                &panel.tab_view,
                &panel.registry,
                &panel.chrome,
                has_clipboard.clone(),
                split_active.clone(),
                refresh_preview.clone(),
                thumbnails.clone(),
                dnd_execute.clone(),
                start_dir,
            );
        });
    }
    window.add_action(&action_new_tab);
    app.set_accels_for_action("win.new-tab", &["<Primary>t"]);

    let action_close_tab = gio::SimpleAction::new("close-tab", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        action_close_tab.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow());
            if let Some(page) = panel.tab_view.selected_page() {
                panel.tab_view.close_page(&page);
            }
        });
    }
    window.add_action(&action_close_tab);
    app.set_accels_for_action("win.close-tab", &["<Primary>w"]);

    let action_next_tab = gio::SimpleAction::new("next-tab", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        action_next_tab.connect_activate(move |_, _| {
            panels.get(*focused.borrow()).tab_view.select_next_page();
        });
    }
    window.add_action(&action_next_tab);
    app.set_accels_for_action("win.next-tab", &["<Primary>Tab"]);

    let action_previous_tab = gio::SimpleAction::new("previous-tab", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        action_previous_tab.connect_activate(move |_, _| {
            panels
                .get(*focused.borrow())
                .tab_view
                .select_previous_page();
        });
    }
    window.add_action(&action_previous_tab);
    app.set_accels_for_action("win.previous-tab", &["<Primary><Shift>Tab"]);
}

/// Registers the Faz 8 split-view `win.*` actions: `F3` toggles the right
/// panel on/off (lazily opening its first tab, mirroring the left panel's
/// current location, the first time it's shown), and "Copy/Move to Other
/// Panel" transfer the focused panel's selection to the other panel's
/// current directory.
#[allow(clippy::too_many_arguments)]
fn setup_split_view_actions(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    panels: &Panels,
    focused: &Rc<RefCell<PanelId>>,
    header: &headerbar::HeaderBarHandles,
    progress: &ProgressToastHandles,
    has_clipboard: Rc<dyn Fn() -> bool>,
    refresh_preview: Rc<dyn Fn()>,
    thumbnails: Rc<crate::thumbnails::ThumbnailService>,
    dnd_execute: dnd::DropExecutor,
    undo_stack: &SharedUndoStack,
) {
    let action_toggle_split = gio::SimpleAction::new("toggle-split-view", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        let header = header.clone();
        let refresh_preview = refresh_preview.clone();
        let thumbnails = thumbnails.clone();
        let dnd_execute = dnd_execute.clone();
        action_toggle_split.connect_activate(move |_, _| {
            let showing = !panels.right.frame.is_visible();
            if showing && panels.right.tab_view.n_pages() == 0 {
                let start_dir = active_tab(&panels.left.tab_view, &panels.left.registry)
                    .map(|tab| tab.state.borrow().current_dir.clone())
                    .unwrap_or_else(|| VeyraPath::from_local(glib::home_dir()));
                let right_frame = panels.right.frame.clone();
                let split_active: Rc<dyn Fn() -> bool> = Rc::new(move || right_frame.is_visible());
                open_tab(
                    &panels.right.tab_view,
                    &panels.right.registry,
                    &panels.right.chrome,
                    has_clipboard.clone(),
                    split_active,
                    refresh_preview.clone(),
                    thumbnails.clone(),
                    dnd_execute.clone(),
                    start_dir,
                );
            }
            panels.right.frame.set_visible(showing);
            header.split_toggle_button.set_active(showing);
            if !showing {
                *focused.borrow_mut() = PanelId::Left;
            }
            set_active_panel_visuals(&panels, *focused.borrow(), showing);
            if let Some(tab) = split_view::focused_tab(&panels, &focused) {
                sync_view_switcher(&header.view_switcher_buttons, &tab);
            }
            refresh_preview();
        });
    }
    window.add_action(&action_toggle_split);
    app.set_accels_for_action("win.toggle-split-view", &["F3"]);

    let action_copy_other = gio::SimpleAction::new("copy-to-other-panel-selected", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        let progress = progress.clone();
        let undo_stack = undo_stack.clone();
        action_copy_other.connect_activate(move |_, _| {
            transfer_to_other_panel(
                &window,
                &panels,
                &focused,
                &progress,
                OperationKind::Copy,
                &undo_stack,
            );
        });
    }
    window.add_action(&action_copy_other);
    app.set_accels_for_action("win.copy-to-other-panel-selected", &["<Primary><Shift>o"]);

    let action_move_other = gio::SimpleAction::new("move-to-other-panel-selected", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        let progress = progress.clone();
        let undo_stack = undo_stack.clone();
        action_move_other.connect_activate(move |_, _| {
            transfer_to_other_panel(
                &window,
                &panels,
                &focused,
                &progress,
                OperationKind::Move,
                &undo_stack,
            );
        });
    }
    window.add_action(&action_move_other);
    app.set_accels_for_action("win.move-to-other-panel-selected", &["<Primary><Shift>m"]);
}

/// Copies or moves the focused panel's selected item straight into whatever
/// directory the *other* panel currently has open — the core Faz 8
/// panel-to-panel operation. A no-op if the split view isn't showing a
/// second panel, or if either panel has nothing selected/open.
fn transfer_to_other_panel(
    window: &adw::ApplicationWindow,
    panels: &Panels,
    focused: &Rc<RefCell<PanelId>>,
    progress: &ProgressToastHandles,
    kind: OperationKind,
    undo_stack: &SharedUndoStack,
) {
    if !panels.right.frame.is_visible() {
        return;
    }
    let source_id = *focused.borrow();
    let source = panels.get(source_id).clone();
    let destination_panel = panels.get(source_id.other()).clone();

    let Some(source_tab) = active_tab(&source.tab_view, &source.registry) else {
        return;
    };
    let items = source_tab.selections.selected_items(&source_tab.view_stack);
    if items.is_empty() {
        return;
    }
    let Some(dest_tab) = active_tab(&destination_panel.tab_view, &destination_panel.registry)
    else {
        return;
    };
    let destination = dest_tab.state.borrow().current_dir.clone();

    run_bulk_operation(
        window,
        vec![
            (source_tab.state.clone(), source.chrome.clone()),
            (dest_tab.state.clone(), destination_panel.chrome.clone()),
        ],
        progress,
        kind,
        items.into_iter().map(|item| item.path).collect(),
        Some(destination),
        undo_stack,
    );
}

/// Registers the Copy/Cut/Paste/Trash/Delete `win.*` actions and their
/// accelerators. All act on whichever tab is active in the currently
/// focused panel. `Delete` only ever trashes (Rule #39: permanent delete
/// must never be one accidental keypress) — permanent delete is
/// `<Shift>Delete`, and always goes through `dialogs::delete_confirm` first
/// (Rule #38).
fn setup_operation_actions(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    panels: &Panels,
    focused: &Rc<RefCell<PanelId>>,
    progress: &ProgressToastHandles,
    clipboard: &Rc<RefCell<Option<ClipboardEntry>>>,
    undo_stack: &SharedUndoStack,
) {
    let action_copy = gio::SimpleAction::new("copy-selection", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        let clipboard = clipboard.clone();
        action_copy.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let paths: Vec<VeyraPath> = tab
                .selections
                .selected_items(&tab.view_stack)
                .into_iter()
                .map(|item| item.path)
                .collect();
            if !paths.is_empty() {
                *clipboard.borrow_mut() = Some(ClipboardEntry { paths, cut: false });
            }
        });
    }
    window.add_action(&action_copy);
    app.set_accels_for_action("win.copy-selection", &["<Primary>c"]);

    let action_cut = gio::SimpleAction::new("cut-selection", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        let clipboard = clipboard.clone();
        action_cut.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let paths: Vec<VeyraPath> = tab
                .selections
                .selected_items(&tab.view_stack)
                .into_iter()
                .map(|item| item.path)
                .collect();
            if !paths.is_empty() {
                *clipboard.borrow_mut() = Some(ClipboardEntry { paths, cut: true });
            }
        });
    }
    window.add_action(&action_cut);
    app.set_accels_for_action("win.cut-selection", &["<Primary>x"]);

    let action_paste = gio::SimpleAction::new("paste", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        let progress = progress.clone();
        let clipboard = clipboard.clone();
        let undo_stack = undo_stack.clone();
        action_paste.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let Some(entry) = clipboard.borrow_mut().take() else {
                return;
            };
            let destination = tab.state.borrow().current_dir.clone();
            let kind = if entry.cut {
                OperationKind::Move
            } else {
                OperationKind::Copy
            };
            run_bulk_operation(
                &window,
                vec![(tab.state.clone(), panel.chrome.clone())],
                &progress,
                kind,
                entry.paths,
                Some(destination),
                &undo_stack,
            );
        });
    }
    window.add_action(&action_paste);
    app.set_accels_for_action("win.paste", &["<Primary>v"]);

    let action_trash = gio::SimpleAction::new("trash-selection", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        let progress = progress.clone();
        let undo_stack = undo_stack.clone();
        action_trash.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let paths: Vec<VeyraPath> = tab
                .selections
                .selected_items(&tab.view_stack)
                .into_iter()
                .map(|item| item.path)
                .collect();
            if !paths.is_empty() {
                run_bulk_operation(
                    &window,
                    vec![(tab.state.clone(), panel.chrome.clone())],
                    &progress,
                    OperationKind::Trash,
                    paths,
                    None,
                    &undo_stack,
                );
            }
        });
    }
    window.add_action(&action_trash);
    app.set_accels_for_action("win.trash-selection", &["Delete"]);

    let action_delete = gio::SimpleAction::new("delete-selection", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        let progress = progress.clone();
        let undo_stack = undo_stack.clone();
        action_delete.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let paths: Vec<VeyraPath> = tab
                .selections
                .selected_items(&tab.view_stack)
                .into_iter()
                .map(|item| item.path)
                .collect();
            if paths.is_empty() {
                return;
            }
            let window_for_confirm = window.clone();
            let state = tab.state.clone();
            let chrome = panel.chrome.clone();
            let progress = progress.clone();
            let paths_for_delete = paths.clone();
            let undo_stack = undo_stack.clone();
            let confirm_permanent_delete = panel.chrome.settings.borrow().confirm_permanent_delete;
            let run_delete = move || {
                run_bulk_operation(
                    &window_for_confirm,
                    vec![(state, chrome)],
                    &progress,
                    OperationKind::Delete,
                    paths_for_delete,
                    None,
                    &undo_stack,
                );
            };
            // Faz 34: "Confirm Before Permanently Deleting Files" — off
            // skips straight to the (still-undoable, Kural #38/#39) delete.
            if confirm_permanent_delete {
                dialogs::delete_confirm::show(&window, &paths, run_delete);
            } else {
                run_delete();
            }
        });
    }
    window.add_action(&action_delete);
    app.set_accels_for_action("win.delete-selection", &["<Shift>Delete"]);
}

/// Registers Faz 19's compress/extract `win.*` actions, all resolving the
/// focused panel's active tab and its single selection dynamically, same as
/// `setup_operation_actions`. "Extract Here" and "Extract to…" only ever
/// get bound to an archive in the context menu (`context_menu.rs` gates
/// their visibility on the file name), but each action re-checks the
/// format itself so a stray activation never tries to extract a non-archive.
fn setup_archive_actions(
    window: &adw::ApplicationWindow,
    panels: &Panels,
    focused: &Rc<RefCell<PanelId>>,
    progress: &ProgressToastHandles,
) {
    let action_compress = gio::SimpleAction::new("compress-selected", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        let progress = progress.clone();
        action_compress.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let items = tab.selections.selected_items(&tab.view_stack);
            if items.is_empty() {
                return;
            }
            let Some(default_stem) = items[0].path.file_name() else {
                return;
            };
            let current_dir = tab.state.borrow().current_dir.clone();
            let state = tab.state.clone();
            let chrome = panel.chrome.clone();
            let progress = progress.clone();
            let sources: Vec<VeyraPath> = items.into_iter().map(|item| item.path).collect();
            let window_for_confirm = window.clone();
            dialogs::compress_dialog::show(
                &window,
                &default_stem,
                veyra_filesystem::ArchiveFormat::Zip,
                move |name, format| {
                    let output_path = child_path(&current_dir, &name);
                    let (control, receiver) =
                        archive_ops::spawn_compress(output_path, sources, format);
                    run_archive_operation(
                        &window_for_confirm,
                        vec![(state, chrome)],
                        &progress,
                        "Compressing",
                        control,
                        receiver,
                    );
                },
            );
        });
    }
    window.add_action(&action_compress);

    let action_extract_here = gio::SimpleAction::new("extract-here-selected", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        let progress = progress.clone();
        action_extract_here.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let Some(item) = tab.selections.selected(&tab.view_stack) else {
                return;
            };
            let Some(name) = item.path.file_name() else {
                return;
            };
            let Some(format) = veyra_filesystem::ArchiveFormat::from_name(&name) else {
                return;
            };
            let stem = name
                .strip_suffix(format.extension())
                .unwrap_or(&name)
                .to_string();
            let current_dir = tab.state.borrow().current_dir.clone();
            let destination = child_path(&current_dir, &stem);

            let (control, receiver) = archive_ops::spawn_extract(item.path, destination);
            run_archive_operation(
                &window,
                vec![(tab.state.clone(), panel.chrome.clone())],
                &progress,
                "Extracting",
                control,
                receiver,
            );
        });
    }
    window.add_action(&action_extract_here);

    let action_extract_to = gio::SimpleAction::new("extract-to-selected", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        let progress = progress.clone();
        action_extract_to.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let Some(item) = tab.selections.selected(&tab.view_stack) else {
                return;
            };
            let state = tab.state.clone();
            let chrome = panel.chrome.clone();
            let progress = progress.clone();
            let archive_path = item.path;

            let file_dialog = gtk4::FileDialog::builder().title("Extract to…").build();
            let window_for_dialog = window.clone();
            let window_for_result = window.clone();
            file_dialog.select_folder(
                Some(&window_for_dialog),
                gio::Cancellable::NONE,
                move |result| {
                    let Ok(folder) = result else {
                        return;
                    };
                    let Some(path) = folder.path() else {
                        return;
                    };
                    let destination = VeyraPath::from_local(path);
                    let (control, receiver) = archive_ops::spawn_extract(archive_path, destination);
                    run_archive_operation(
                        &window_for_result,
                        vec![(state, chrome)],
                        &progress,
                        "Extracting",
                        control,
                        receiver,
                    );
                },
            );
        });
    }
    window.add_action(&action_extract_to);
}

/// Registers the Faz 6/7/8 context-menu `win.*` actions: item actions
/// (Open, Open With…, Open in New Tab/Window, Rename, Copy Path, Copy
/// Location) and background actions (New Folder, New Document), plus the
/// shared `win.not-implemented` disabled action every not-yet-built entry
/// (Compress/Extract/Open Terminal Here/Properties) binds to. All resolve
/// the focused panel dynamically — a context menu only ever acts on the
/// visible (i.e. focused) panel's active tab. Copy/Cut/Paste/Trash/Delete
/// and Copy/Move-to-Other-Panel are already registered by
/// `setup_operation_actions`/`setup_split_view_actions` and are reused
/// as-is by the context menus.
#[allow(clippy::too_many_arguments)]
fn setup_context_menu_actions(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    panels: &Panels,
    focused: &Rc<RefCell<PanelId>>,
    has_clipboard: Rc<dyn Fn() -> bool>,
    split_active: Rc<dyn Fn() -> bool>,
    refresh_preview: Rc<dyn Fn()>,
    thumbnails: Rc<crate::thumbnails::ThumbnailService>,
    dnd_execute: dnd::DropExecutor,
    undo_stack: &SharedUndoStack,
) {
    let action_not_implemented = gio::SimpleAction::new("not-implemented", None);
    action_not_implemented.set_enabled(false);
    window.add_action(&action_not_implemented);

    let action_open = gio::SimpleAction::new("open-selected", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        action_open.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            if let Some(item) = tab.selections.selected(&tab.view_stack) {
                open_item(&tab, &panel.chrome, item);
            }
        });
    }
    window.add_action(&action_open);

    let action_open_with = gio::SimpleAction::new("open-with-selected", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        action_open_with.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            if let Some(item) = tab.selections.selected(&tab.view_stack) {
                show_open_with_dialog(&window, &item);
            }
        });
    }
    window.add_action(&action_open_with);

    // Faz 22: activated from the right-click "Open With" submenu's
    // recommended-app entries (`context_menu::build_item_menu`), each bound
    // to this one action with the chosen app's desktop id as its string
    // parameter — cheaper than registering a `SimpleAction` per app per
    // menu build.
    let action_open_with_app =
        gio::SimpleAction::new("open-with-app", Some(&String::static_variant_type()));
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        action_open_with_app.connect_activate(move |_, parameter| {
            let Some(app_id) = parameter.and_then(glib::Variant::str).map(str::to_string) else {
                return;
            };
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let Some(item) = tab.selections.selected(&tab.view_stack) else {
                return;
            };
            let Some(app) = gio::DesktopAppInfo::new(&app_id) else {
                show_error_dialog(
                    &window,
                    "Unable to Open File",
                    "The selected application is no longer available.",
                );
                return;
            };
            let file = item.path.to_gio_file();
            if let Err(err) = open_with::launch(&app.upcast(), &file, &window) {
                show_error_dialog(&window, "Unable to Open File", &err.to_string());
            }
        });
    }
    window.add_action(&action_open_with_app);

    let action_open_new_window = gio::SimpleAction::new("open-in-new-window-selected", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        action_open_new_window.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            if let Some(item) = tab.selections.selected(&tab.view_stack) {
                if item.kind().is_directory() {
                    spawn_new_window(&item.path);
                }
            }
        });
    }
    window.add_action(&action_open_new_window);

    let action_open_new_tab = gio::SimpleAction::new("open-in-new-tab-selected", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        let has_clipboard = has_clipboard.clone();
        let split_active = split_active.clone();
        let refresh_preview = refresh_preview.clone();
        let thumbnails = thumbnails.clone();
        let dnd_execute = dnd_execute.clone();
        action_open_new_tab.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let Some(item) = tab.selections.selected(&tab.view_stack) else {
                return;
            };
            if item.kind().is_directory() {
                open_tab(
                    &panel.tab_view,
                    &panel.registry,
                    &panel.chrome,
                    has_clipboard.clone(),
                    split_active.clone(),
                    refresh_preview.clone(),
                    thumbnails.clone(),
                    dnd_execute.clone(),
                    item.path,
                );
            }
        });
    }
    window.add_action(&action_open_new_tab);

    let action_rename = gio::SimpleAction::new("rename-selected", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        let undo_stack = undo_stack.clone();
        action_rename.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let Some(item) = tab.selections.selected(&tab.view_stack) else {
                return;
            };
            let current_name = item.name().to_string();
            let path = item.path.clone();
            let state = tab.state.clone();
            let chrome = panel.chrome.clone();
            let previous_name = current_name.clone();
            let undo_stack = undo_stack.clone();
            dialogs::rename_dialog::show(&window, &current_name, move |new_name| {
                if new_name.is_empty() || new_name == previous_name {
                    return;
                }
                let state = state.clone();
                let chrome = chrome.clone();
                let old_path = path.clone();
                let undo_stack = undo_stack.clone();
                fs_async::run_blocking(
                    move || veyra_filesystem::rename(&path, &new_name),
                    move |result| match result {
                        Ok(new_path) => {
                            undo_stack
                                .borrow_mut()
                                .record(UndoableAction::Rename { old_path, new_path });
                            refresh(&state, &chrome);
                        }
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

    let action_add_bookmark = gio::SimpleAction::new("add-to-bookmarks-selected", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        action_add_bookmark.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let Some(item) = tab.selections.selected(&tab.view_stack) else {
                return;
            };
            if item.kind().is_directory() {
                if let Err(err) = crate::bookmarks::add(&item.path, None) {
                    tracing::warn!(error = %err, "failed to add bookmark");
                    panel
                        .chrome
                        .status_left
                        .set_label(&format!("Add to Bookmarks failed: {err}"));
                }
            }
        });
    }
    window.add_action(&action_add_bookmark);

    let action_copy_path = gio::SimpleAction::new("copy-path-selected", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        action_copy_path.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            if let Some(item) = tab.selections.selected(&tab.view_stack) {
                window.clipboard().set_text(&item.path.to_string());
            }
        });
    }
    window.add_action(&action_copy_path);

    let action_copy_location = gio::SimpleAction::new("copy-location-selected", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        action_copy_location.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            if let Some(item) = tab.selections.selected(&tab.view_stack) {
                window.clipboard().set_text(&parent_display(&item.path));
            }
        });
    }
    window.add_action(&action_copy_location);

    let action_create_folder = gio::SimpleAction::new("create-folder", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        let undo_stack = undo_stack.clone();
        action_create_folder.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
                create_child_entry(&tab.state, &panel.chrome, "New Folder", true, &undo_stack);
            }
        });
    }
    window.add_action(&action_create_folder);
    app.set_accels_for_action("win.create-folder", &["<Primary><Shift>n"]);

    let action_create_document = gio::SimpleAction::new("create-document", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        let undo_stack = undo_stack.clone();
        action_create_document.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
                create_child_entry(
                    &tab.state,
                    &panel.chrome,
                    "New Document",
                    false,
                    &undo_stack,
                );
            }
        });
    }
    window.add_action(&action_create_document);
}

/// Registers the Faz 12 `win.properties-selected` (context menu item entry /
/// `Alt+Enter`) and `win.properties-current` (background context menu)
/// actions: they open the Properties dialog for the focused panel's
/// selection or its current directory, respectively. The former needs no
/// I/O (the `FileItem` is already loaded in the view's model); the latter
/// does a quick background `stat` first since the open directory is never
/// itself an entry in its own listing.
fn setup_properties_actions(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    panels: &Panels,
    focused: &Rc<RefCell<PanelId>>,
    thumbnails: Rc<crate::thumbnails::ThumbnailService>,
) {
    let action_properties_selected = gio::SimpleAction::new("properties-selected", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        let thumbnails = thumbnails.clone();
        action_properties_selected.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let Some(item) = tab.selections.selected(&tab.view_stack) else {
                return;
            };
            let state = tab.state.clone();
            let chrome = panel.chrome.clone();
            dialogs::properties_dialog::show(
                &window,
                item,
                thumbnails.clone(),
                Rc::new(move || refresh(&state, &chrome)),
            );
        });
    }
    window.add_action(&action_properties_selected);
    app.set_accels_for_action("win.properties-selected", &["<Alt>Return"]);

    let action_properties_current = gio::SimpleAction::new("properties-current", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        action_properties_current.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let path = tab.state.borrow().current_dir.clone();
            let window = window.clone();
            let thumbnails = thumbnails.clone();
            let state = tab.state.clone();
            let chrome = panel.chrome.clone();
            fs_async::run_blocking(
                move || veyra_filesystem::stat(&path),
                move |result| match result {
                    Ok(item) => {
                        dialogs::properties_dialog::show(
                            &window,
                            item,
                            thumbnails,
                            Rc::new(move || refresh(&state, &chrome)),
                        );
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "failed to stat current directory for properties");
                        chrome
                            .status_left
                            .set_label(&format!("Properties failed: {err}"));
                    }
                },
            );
        });
    }
    window.add_action(&action_properties_current);
}

/// Registers the Faz 10 `win.toggle-preview` action (`F9`): shows/hides the
/// preview sidebar and, when revealing it, immediately populates it from the
/// focused panel's current selection (it may be stale from before the panel
/// was hidden).
fn setup_preview_actions(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    preview: &PreviewPanelHandles,
    header: &headerbar::HeaderBarHandles,
    refresh_preview: Rc<dyn Fn()>,
) {
    let action_toggle_preview = gio::SimpleAction::new("toggle-preview", None);
    {
        let preview_widget = preview.widget.clone();
        let preview_toggle_button = header.preview_toggle_button.clone();
        action_toggle_preview.connect_activate(move |_, _| {
            let showing = !preview_widget.is_visible();
            preview_widget.set_visible(showing);
            preview_toggle_button.set_active(showing);
            if showing {
                refresh_preview();
            }
        });
    }
    window.add_action(&action_toggle_preview);
    app.set_accels_for_action("win.toggle-preview", &["F9"]);
}

/// Registers the Faz 20 `win.analyze-disk-selected` (context menu entry for
/// a selected folder) and `win.analyze-disk-current` (`Ctrl+Shift+U`, the
/// focused panel's own current directory) actions, both opening the Disk
/// Analyzer dialog. `navigate` is threaded through so the dialog's "Open in
/// Folder" entries can jump the caller's file browser straight to a result.
fn setup_disk_analyzer_actions(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    panels: &Panels,
    focused: &Rc<RefCell<PanelId>>,
    navigate: Rc<dyn Fn(VeyraPath)>,
) {
    let action_analyze_selected = gio::SimpleAction::new("analyze-disk-selected", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        let navigate = navigate.clone();
        action_analyze_selected.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let Some(item) = tab.selections.selected(&tab.view_stack) else {
                return;
            };
            if !item.kind().is_directory() {
                return;
            }
            dialogs::disk_analyzer_dialog::show(&window, item.path, navigate.clone());
        });
    }
    window.add_action(&action_analyze_selected);

    let action_analyze_current = gio::SimpleAction::new("analyze-disk-current", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        action_analyze_current.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let path = tab.state.borrow().current_dir.clone();
            dialogs::disk_analyzer_dialog::show(&window, path, navigate.clone());
        });
    }
    window.add_action(&action_analyze_current);
    app.set_accels_for_action("win.analyze-disk-current", &["<Primary><Shift>u"]);
}

/// Registers the Faz 23 `win.open-terminal-here-selected` (context menu
/// entry for a selected item) and `win.open-terminal-here-current` (context
/// menu on empty space, plus `F4` and `Ctrl+Alt+T`, both the Dolphin/GNOME
/// standard shortcuts) actions. Both resolve the focused panel's terminal
/// via `terminal::open_terminal`, which never runs a shell string (Rule
/// #19) and never hardcodes a single emulator (Rule #25).
fn setup_terminal_actions(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    panels: &Panels,
    focused: &Rc<RefCell<PanelId>>,
) {
    let action_terminal_selected = gio::SimpleAction::new("open-terminal-here-selected", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        action_terminal_selected.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let Some(item) = tab.selections.selected(&tab.view_stack) else {
                return;
            };
            if let Err(err) = terminal::open_terminal(&item.path) {
                show_error_dialog(&window, "Unable to Open Terminal", &err.to_string());
            }
        });
    }
    window.add_action(&action_terminal_selected);

    let action_terminal_current = gio::SimpleAction::new("open-terminal-here-current", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        action_terminal_current.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let path = tab.state.borrow().current_dir.clone();
            if let Err(err) = terminal::open_terminal(&path) {
                show_error_dialog(&window, "Unable to Open Terminal", &err.to_string());
            }
        });
    }
    window.add_action(&action_terminal_current);
    app.set_accels_for_action("win.open-terminal-here-current", &["F4", "<Primary><Alt>t"]);
}

/// Faz 28: Registers `win.open-terminal-as-root-selected` and
/// `win.open-terminal-as-root-current`, which spawn a terminal as root via
/// pkexec for the selected item's directory or the currently-open directory.
fn setup_terminal_as_root_actions(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    panels: &Panels,
    focused: &Rc<RefCell<PanelId>>,
) {
    let action_terminal_root_selected =
        gio::SimpleAction::new("open-terminal-as-root-selected", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        action_terminal_root_selected.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let Some(item) = tab.selections.selected(&tab.view_stack) else {
                return;
            };
            let target_dir = if item.kind().is_directory() {
                item.path.clone()
            } else if let Some(local) = item.path.as_local_path() {
                if let Some(parent) = local.parent() {
                    VeyraPath::from_local(parent)
                } else {
                    item.path.clone()
                }
            } else {
                item.path.clone()
            };
            let window = window.clone();
            fs_async::run_blocking(
                move || {
                    crate::privileged::open_terminal_as_root(target_dir.as_local_path().unwrap())
                },
                move |result| {
                    if let Err(err) = result {
                        show_error_dialog(
                            &window,
                            "Unable to Open Terminal as Root",
                            &err.to_string(),
                        );
                    }
                },
            );
        });
    }
    window.add_action(&action_terminal_root_selected);

    let action_terminal_root_current =
        gio::SimpleAction::new("open-terminal-as-root-current", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        action_terminal_root_current.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let path = tab.state.borrow().current_dir.clone();
            let window = window.clone();
            fs_async::run_blocking(
                move || crate::privileged::open_terminal_as_root(path.as_local_path().unwrap()),
                move |result| {
                    if let Err(err) = result {
                        show_error_dialog(
                            &window,
                            "Unable to Open Terminal as Root",
                            &err.to_string(),
                        );
                    }
                },
            );
        });
    }
    window.add_action(&action_terminal_root_current);
    app.set_accels_for_action(
        "win.open-terminal-as-root-current",
        &["<Primary><Shift><Alt>t"],
    );
}

/// Registers Faz 21's `win.connect-to-server`, activated by the sidebar's
/// Network section "+ Connect to Server…" button. Shows the dialog; a
/// successful connection navigates the focused panel into the new mount.
fn setup_network_actions(window: &adw::ApplicationWindow, navigate: Rc<dyn Fn(VeyraPath)>) {
    let action_connect = gio::SimpleAction::new("connect-to-server", None);
    {
        let window = window.clone();
        action_connect.connect_activate(move |_, _| {
            dialogs::connect_server_dialog::show(&window, navigate.clone());
        });
    }
    window.add_action(&action_connect);
}

/// Registers the Faz 24 Command Palette actions: `win.command-palette`
/// (`Ctrl+K` / `Ctrl+Shift+P`) opens the dialog; `win.set-view-mode` and
/// `win.sort-by` (both string-parameterized) back the palette's View and
/// Sort & Filter entries, since those choices previously only existed as
/// header-bar toggle button / popover check-button handlers with no
/// standalone `win.*` action to search for or trigger by shortcut.
/// `win.set-view-mode` reuses `sync_view_switcher` so the header bar's own
/// toggle buttons stay in sync when a view mode is set from the palette;
/// `win.sort-by` mirrors the Sort & Filter popover's per-key check-button
/// handler in `headerbar.rs`, which re-reads `tab.sort_config` fresh every
/// time it's opened, so no equivalent sync-back is needed there.
fn setup_command_palette_actions(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    panels: &Panels,
    focused: &Rc<RefCell<PanelId>>,
    header: &headerbar::HeaderBarHandles,
    refresh_preview: Rc<dyn Fn()>,
) {
    let action_command_palette = gio::SimpleAction::new("command-palette", None);
    {
        let window = window.clone();
        action_command_palette.connect_activate(move |_, _| {
            dialogs::command_palette_dialog::show(&window);
        });
    }
    window.add_action(&action_command_palette);
    app.set_accels_for_action("win.command-palette", &["<Primary>k", "<Primary><Shift>p"]);

    let action_set_view_mode =
        gio::SimpleAction::new("set-view-mode", Some(&String::static_variant_type()));
    {
        let panels = panels.clone();
        let focused = focused.clone();
        let header = header.clone();
        action_set_view_mode.connect_activate(move |_, parameter| {
            let Some(mode_name) = parameter.and_then(glib::Variant::str) else {
                return;
            };
            let mode = match mode_name {
                "icon" => ViewMode::Icon,
                "compact" => ViewMode::Compact,
                "details" => ViewMode::Details,
                _ => return,
            };
            let Some(tab) = split_view::focused_tab(&panels, &focused) else {
                return;
            };
            tab.view_stack.set_visible_child_name(mode.stack_name());
            sync_view_switcher(&header.view_switcher_buttons, &tab);
            refresh_preview();
        });
    }
    window.add_action(&action_set_view_mode);
    app.set_accels_for_action("win.set-view-mode::icon", &["<Primary>1"]);
    app.set_accels_for_action("win.set-view-mode::compact", &["<Primary>2"]);
    app.set_accels_for_action("win.set-view-mode::details", &["<Primary>3"]);

    let action_sort_by = gio::SimpleAction::new("sort-by", Some(&String::static_variant_type()));
    {
        let panels = panels.clone();
        let focused = focused.clone();
        action_sort_by.connect_activate(move |_, parameter| {
            let Some(key_name) = parameter.and_then(glib::Variant::str) else {
                return;
            };
            let key = match key_name {
                "name" => crate::sorting::SortKey::Name,
                "size" => crate::sorting::SortKey::Size,
                "modified" => crate::sorting::SortKey::Modified,
                "type" => crate::sorting::SortKey::Type,
                _ => return,
            };
            let Some(tab) = split_view::focused_tab(&panels, &focused) else {
                return;
            };
            tab.sort_config.borrow_mut().key = key;
            tab.resort();
        });
    }
    window.add_action(&action_sort_by);
}

/// Registers the Faz 15 `win.clear-recent-history` (behind an `AdwAlertDialog`
/// confirmation, per Rule #38/#39) and `win.toggle-privacy-mode` actions.
/// Both act window-wide, not per-panel: the recent-files registry and the
/// privacy toggle are process-global state, not per-tab.
fn setup_recent_actions(
    window: &adw::ApplicationWindow,
    panels: &Panels,
    privacy_mode: Rc<RefCell<bool>>,
) {
    let action_clear_history = gio::SimpleAction::new("clear-recent-history", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        action_clear_history.connect_activate(move |_, _| {
            let panels = panels.clone();
            dialogs::clear_recent_confirm::show(&window, move || {
                recent::clear_history();
                refresh_recent_panels(&panels);
            });
        });
    }
    window.add_action(&action_clear_history);
    for panel in [&panels.left, &panels.right] {
        panel
            .chrome
            .recent_banner
            .clear_button
            .set_action_name(Some("win.clear-recent-history"));
    }

    let action_toggle_privacy = gio::SimpleAction::new("toggle-privacy-mode", None);
    {
        let panels = panels.clone();
        action_toggle_privacy.connect_activate(move |_, _| {
            let new_value = {
                let mut mode = privacy_mode.borrow_mut();
                *mode = !*mode;
                *mode
            };
            for panel in [&panels.left, &panels.right] {
                panel
                    .chrome
                    .recent_banner
                    .privacy_switch
                    .set_active(new_value);
            }
        });
    }
    window.add_action(&action_toggle_privacy);
}

/// Reloads every panel whose focused tab is currently showing `recent:///`,
/// after `win.clear-recent-history` empties the registry.
fn refresh_recent_panels(panels: &Panels) {
    for panel in [&panels.left, &panels.right] {
        if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
            if recent::is_recent_location(&tab.state.borrow().current_dir) {
                refresh(&tab.state, &panel.chrome);
            }
        }
    }
}

/// Registers the Faz 18 `win.empty-trash` (behind
/// `dialogs::empty_trash_confirm`, Rule #38/#39) and `win.restore-selected`
/// actions. Both act on whichever tab is active in the currently focused
/// panel — `empty-trash` because there is only ever one home trash to empty
/// regardless of which panel's banner triggered it, `restore-selected`
/// because it targets the focused panel's current selection, same as every
/// other selection-scoped `win.*` action in `setup_operation_actions`.
fn setup_trash_actions(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    panels: &Panels,
    focused: &Rc<RefCell<PanelId>>,
    undo_stack: &SharedUndoStack,
) {
    let action_empty_trash = gio::SimpleAction::new("empty-trash", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        action_empty_trash.connect_activate(move |_, _| {
            let window_for_confirm = window.clone();
            let panels = panels.clone();
            let confirm_trash_empty = panels.left.chrome.settings.borrow().confirm_trash_empty;
            let run_empty = move || {
                let panels_for_done = panels.clone();
                fs_async::run_blocking(veyra_filesystem::empty_trash, move |result| {
                    if let Err(err) = result {
                        tracing::warn!(error = %err, "failed to empty trash");
                        show_error_dialog(
                            &window_for_confirm,
                            "Unable to Empty Trash",
                            &err.to_string(),
                        );
                    }
                    refresh_trash_panels(&panels_for_done);
                });
            };
            // Faz 34: "Confirm Before Emptying Trash".
            if confirm_trash_empty {
                dialogs::empty_trash_confirm::show(&window, run_empty);
            } else {
                run_empty();
            }
        });
    }
    window.add_action(&action_empty_trash);
    for panel in [&panels.left, &panels.right] {
        panel
            .chrome
            .trash_banner
            .empty_button
            .set_action_name(Some("win.empty-trash"));
    }

    let action_restore = gio::SimpleAction::new("restore-selected", None);
    {
        let window = window.clone();
        let panels = panels.clone();
        let focused = focused.clone();
        let undo_stack = undo_stack.clone();
        action_restore.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let Some(item) = tab.selections.selected(&tab.view_stack) else {
                return;
            };
            let window = window.clone();
            let state = tab.state.clone();
            let chrome = panel.chrome.clone();
            let trashed_path = item.path.clone();
            let undo_stack = undo_stack.clone();
            fs_async::run_blocking(
                move || veyra_filesystem::restore_from_trash(&item.path),
                move |result| match result {
                    Ok(restored) => {
                        undo_stack
                            .borrow_mut()
                            .record(UndoableAction::Restore(vec![(restored, trashed_path)]));
                        refresh(&state, &chrome);
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "failed to restore item from trash");
                        show_error_dialog(&window, "Unable to Restore Item", &err.to_string());
                    }
                },
            );
        });
    }
    window.add_action(&action_restore);
    app.set_accels_for_action("win.restore-selected", &["<Primary><Shift>r"]);
}

/// Registers Faz 33's `win.undo` (`Ctrl+Z`) and `win.redo`
/// (`Ctrl+Shift+Z`/`Ctrl+Y`) actions: pop one step off the shared
/// `UndoStack`, run its inverse filesystem call on a background thread
/// (Rule #11/#14), and — on success — push the step that was actually
/// executed back onto the opposite stack so it can be replayed. A missing/
/// moved-since path is reported gracefully, never panics (Rule #15/#16).
/// Refreshes every open tab across both panels afterwards, like the Faz 26
/// DND executor does — an undo/redo step's affected directory can be
/// anywhere, not just the currently focused tab.
fn setup_undo_actions(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    panels: &Panels,
    undo_stack: &SharedUndoStack,
) {
    let action_undo = gio::SimpleAction::new("undo", None);
    {
        let panels = panels.clone();
        let undo_stack = undo_stack.clone();
        action_undo.connect_activate(move |_, _| {
            let Some(action) = undo_stack.borrow_mut().take_undo() else {
                return;
            };
            let panels_for_done = panels.clone();
            let undo_stack = undo_stack.clone();
            fs_async::run_blocking(
                move || undo::perform_undo(action),
                move |result| {
                    if result.error_count > 0 {
                        tracing::warn!(errors = result.error_count, "undo had failures");
                    }
                    if let Some(resulting) = result.resulting_action {
                        undo_stack.borrow_mut().record_redo(resulting);
                    }
                    report_and_refresh(&panels_for_done, result.message);
                },
            );
        });
    }
    window.add_action(&action_undo);
    app.set_accels_for_action("win.undo", &["<Primary>z"]);

    let action_redo = gio::SimpleAction::new("redo", None);
    {
        let panels = panels.clone();
        let undo_stack = undo_stack.clone();
        action_redo.connect_activate(move |_, _| {
            let Some(action) = undo_stack.borrow_mut().take_redo() else {
                return;
            };
            let panels_for_done = panels.clone();
            let undo_stack = undo_stack.clone();
            fs_async::run_blocking(
                move || undo::perform_redo(action),
                move |result| {
                    if result.error_count > 0 {
                        tracing::warn!(errors = result.error_count, "redo had failures");
                    }
                    if let Some(resulting) = result.resulting_action {
                        undo_stack.borrow_mut().record_undo(resulting);
                    }
                    report_and_refresh(&panels_for_done, result.message);
                },
            );
        });
    }
    window.add_action(&action_redo);
    app.set_accels_for_action("win.redo", &["<Primary><Shift>z", "<Primary>y"]);
}

/// Refreshes every open tab across both panels (an undo/redo step's affected
/// directory, like a DND drop's, can be in either panel or any tab), then
/// shows `message` — e.g. "Undid: Renamed 'report_v2.pdf' back to
/// 'report.pdf'" — on both panels' status bar. There is no toast/overlay
/// widget in this window yet, so this reuses the same status-bar feedback
/// every other `win.*` action already gives; the message is set with a
/// short delay so the refresh's own "Loading…"/item-count status updates
/// don't immediately overwrite it.
fn report_and_refresh(panels: &Panels, message: String) {
    for (state, chrome) in all_tab_refresh_targets(panels) {
        refresh(&state, &chrome);
    }
    let panels = panels.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
        for panel in [&panels.left, &panels.right] {
            panel.chrome.status_left.set_label(&message);
        }
    });
}

/// Reloads every panel whose focused tab is currently showing `trash:///`,
/// after `win.empty-trash` empties it.
fn refresh_trash_panels(panels: &Panels) {
    for panel in [&panels.left, &panels.right] {
        if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
            if trash::is_trash_location(&tab.state.borrow().current_dir) {
                refresh(&tab.state, &panel.chrome);
            }
        }
    }
}

/// Surfaces a hard operation failure (disk full, permission denied, a
/// restore destination conflict, a corrupt/unrecognized archive, ...) as an
/// `AdwAlertDialog` rather than a silent no-op or a panic (Rule #15/#18).
/// Shared by Trash and Faz 19 archive (compress/extract) operations.
fn show_error_dialog(parent: &adw::ApplicationWindow, heading: &str, body: &str) {
    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .build();
    dialog.add_responses(&[("ok", "OK")]);
    dialog.set_default_response(Some("ok"));
    dialog.set_close_response("ok");
    dialog.present(Some(parent));
}

/// Builds a new tab rooted at `start_dir` inside the panel identified by
/// `tab_view`/`registry`, registers it, and switches to it. Every tab owns
/// an independent `AppState` (location, history, item model), its own
/// Icon/Compact/Details view stack, per-view selections, and its own search
/// query/filter — the isolation Faz 7 requires, per panel (Faz 8).
#[allow(clippy::too_many_arguments)]
fn open_tab(
    tab_view: &adw::TabView,
    registry: &TabRegistry,
    chrome: &Chrome,
    has_clipboard: Rc<dyn Fn() -> bool>,
    split_active: Rc<dyn Fn() -> bool>,
    refresh_preview: Rc<dyn Fn()>,
    thumbnails: Rc<crate::thumbnails::ThumbnailService>,
    dnd_execute: dnd::DropExecutor,
    start_dir: VeyraPath,
) -> TabPage {
    let state = AppState::new(start_dir.clone());
    // Faz 26: every view this tab builds shares this wiring — `current_dir`
    // is polled live (not captured once) so a background drop always lands
    // in whichever directory the tab has navigated to by the time the drop
    // happens, not the directory it started at.
    let dnd_wiring = {
        let state = state.clone();
        dnd::DndWiring {
            current_dir: Rc::new(move || state.borrow().current_dir.clone()),
            execute: dnd_execute,
        }
    };
    let dnd_execute_for_tab = dnd_wiring.execute.clone();
    let search_query = Rc::new(RefCell::new(String::new()));
    let quick_filter = Rc::new(RefCell::new(crate::sorting::QuickFilter::default()));
    // Faz 34: a new tab's initial hidden-files/sort defaults come from the
    // Preferences dialog's Files & Display page — once open, Ctrl+H and the
    // Sort & Filter menu still isolate this tab from every other (Kural
    // #51), matching how `default_view_mode` below only picks this tab's
    // starting view, not a retroactive one.
    let initial_settings = chrome.settings.borrow();
    let show_hidden = Rc::new(RefCell::new(initial_settings.show_hidden));
    let filter = build_combined_filter(
        search_query.clone(),
        quick_filter.clone(),
        show_hidden.clone(),
    );
    let sort_config = Rc::new(RefCell::new(crate::sorting::SortConfig {
        folders_first: initial_settings.folders_first,
        ..crate::sorting::SortConfig::default()
    }));
    let initial_view_mode = initial_settings.default_view_mode.to_view_mode();
    drop(initial_settings);
    let sorter = crate::sorting::build_sorter(sort_config.clone());
    let sort_sync_guard = Rc::new(std::cell::Cell::new(false));
    let view_stack = gtk4::Stack::new();

    // `on_open` is wired into the views before this tab's own `TabPage`
    // exists (the views need it at construction time); the slot is filled
    // in once the tab is fully built, and `on_open` is only ever invoked
    // later in response to a user double-click.
    let self_slot: Rc<RefCell<Option<TabPage>>> = Rc::new(RefCell::new(None));
    let on_open: Rc<dyn Fn(FileItem)> = {
        let self_slot = self_slot.clone();
        let chrome = chrome.clone();
        Rc::new(move |item: FileItem| {
            if let Some(tab) = self_slot.borrow().clone() {
                open_item(&tab, &chrome, item);
            }
        })
    };

    let is_trash: Rc<dyn Fn() -> bool> = {
        let state = state.clone();
        Rc::new(move || trash::is_trash_location(&state.borrow().current_dir))
    };

    let selections;
    let details_column_view_slot;
    let details_sort_columns_slot;
    {
        let model = state.borrow().model.clone();
        let filter = filter.clone();
        let on_open = on_open.clone();

        let (icon_widget, icon_selection) = crate::views::build_icon_view(
            &model,
            &filter,
            &sorter,
            {
                let on_open = on_open.clone();
                move |item| on_open(item)
            },
            has_clipboard.clone(),
            split_active.clone(),
            is_trash.clone(),
            thumbnails.clone(),
            dnd_wiring.clone(),
            chrome.settings.clone(),
        );
        view_stack.add_named(&icon_widget, Some(ViewMode::Icon.stack_name()));

        let (compact_widget, compact_selection) = crate::views::build_compact_view(
            &model,
            &filter,
            &sorter,
            {
                let on_open = on_open.clone();
                move |item| on_open(item)
            },
            has_clipboard.clone(),
            split_active.clone(),
            is_trash.clone(),
            thumbnails.clone(),
            dnd_wiring.clone(),
            chrome.settings.clone(),
        );
        view_stack.add_named(&compact_widget, Some(ViewMode::Compact.stack_name()));

        let details = crate::views::build_details_view(
            &model,
            &filter,
            crate::views::DetailsSortWiring {
                sort_config: sort_config.clone(),
                sorter: sorter.clone(),
                sync_guard: sort_sync_guard.clone(),
            },
            move |item| on_open(item),
            has_clipboard,
            split_active,
            is_trash,
            thumbnails,
            dnd_wiring,
            chrome.settings.clone(),
        );
        let details_selection = details.selection;
        view_stack.add_named(&details.widget, Some(ViewMode::Details.stack_name()));
        details_column_view_slot = details.column_view;
        details_sort_columns_slot = details.sort_columns;

        view_stack.set_visible_child_name(initial_view_mode.stack_name());

        for selection in [&icon_selection, &compact_selection, &details_selection] {
            let refresh_preview = refresh_preview.clone();
            selection.connect_selection_changed(move |_, _, _| refresh_preview());
        }

        selections = ViewSelections {
            icon: icon_selection,
            compact: compact_selection,
            details: details_selection,
        };
    }

    let adw_page = tab_view.append(&view_stack);

    let tab = TabPage {
        state,
        view_stack,
        selections,
        filter,
        search_query,
        sort_config,
        quick_filter,
        show_hidden,
        sorter,
        details_column_view: details_column_view_slot,
        details_sort_columns: details_sort_columns_slot,
        sort_sync_guard,
        adw_page: adw_page.clone(),
        dnd_execute: dnd_execute_for_tab,
    };
    *self_slot.borrow_mut() = Some(tab.clone());
    registry.borrow_mut().insert(adw_page.clone(), tab.clone());

    tab_view.set_selected_page(&adw_page);
    navigate_to(&tab, chrome, start_dir, false);

    tab
}

/// Opens `item` in-place: navigates the owning tab for directories, or
/// launches the system default application off the GTK main thread for
/// files.
fn open_item(tab: &TabPage, chrome: &Chrome, item: FileItem) {
    if item.kind().is_directory() {
        navigate_to(tab, chrome, item.path.clone(), true);
    } else {
        // Faz 15: record into the XDG recent-files registry before handing
        // off to the background thread — `RecentManager` is GTK-main-thread
        // only, so this must happen here, not inside the spawned thread.
        // Faz 34: "Remember Recently Used Files" — off means never record,
        // same as privacy mode but a persistent rather than session choice.
        if chrome.settings.borrow().store_recent_files {
            let uri = item.path.to_gio_file().uri();
            recent::record_opened(&uri, &chrome.privacy_mode);
        }
        std::thread::spawn(move || {
            if let Err(err) = veyra_filesystem::open(&item.path) {
                tracing::warn!(path = %veyra_core::security::log_path(&item.path), error = %err, "failed to open item");
            }
        });
    }
}

/// Creates a new folder or empty file named `base_name` (auto-suffixed with
/// `(2)`, `(3)`, ... on collision, per `veyra_filesystem::suggest_name`)
/// inside the current directory, then refreshes the listing.
fn create_child_entry(
    state: &SharedState,
    chrome: &Chrome,
    base_name: &str,
    is_dir: bool,
    undo_stack: &SharedUndoStack,
) {
    let dir = state.borrow().current_dir.clone();
    let name = unique_child_name(&dir, base_name);
    let path = child_path(&dir, &name);

    let state = state.clone();
    let chrome = chrome.clone();
    let undo_stack = undo_stack.clone();
    let path_for_create = path.clone();
    fs_async::run_blocking(
        move || {
            if is_dir {
                veyra_filesystem::create_dir(&path_for_create)
            } else {
                veyra_filesystem::create_file(&path_for_create)
            }
        },
        move |result| match result {
            Ok(()) => {
                let action = if is_dir {
                    UndoableAction::CreateFolder(path)
                } else {
                    UndoableAction::CreateFile(path)
                };
                undo_stack.borrow_mut().record(action);
                refresh(&state, &chrome);
            }
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

/// Shows the Faz 22 Open With dialog for `item` (search/recommend/launch via
/// `open_with`/`dialogs::open_with_dialog`), replacing the previous
/// deprecated-`AppChooserDialog` placeholder.
fn show_open_with_dialog(window: &adw::ApplicationWindow, item: &FileItem) {
    let window_err = window.clone();
    dialogs::open_with_dialog::show(
        window,
        &item.path,
        &item.metadata.mime_type,
        move |heading, body| {
            show_error_dialog(&window_err, heading, body);
        },
    );
}

/// Relaunches the Veyra binary pointed at `path`, standing in for "Open in
/// New Window" until a later phase gives real in-process multi-window state
/// sharing.
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
/// resolution dialog, and completion refreshes every `(state, chrome)` in
/// `refresh_targets` (files changed on disk) and surfaces any errors on the
/// first target's status bar. Faz 8's panel-to-panel transfers pass two
/// targets (source panel + destination panel); every other caller passes
/// exactly one.
fn run_bulk_operation(
    window: &adw::ApplicationWindow,
    refresh_targets: Vec<(SharedState, Chrome)>,
    progress: &ProgressToastHandles,
    kind: OperationKind,
    sources: Vec<VeyraPath>,
    destination: Option<VeyraPath>,
    undo_stack: &SharedUndoStack,
) {
    if sources.is_empty() {
        return;
    }

    let request = OperationRequest {
        kind,
        sources,
        destination: destination.clone(),
    };
    let (control, receiver) = operations::spawn(request);
    let op_id = widgets::progress_toast::begin(progress, &control, kind);

    let window = window.clone();
    let progress = progress.clone();
    let undo_stack = undo_stack.clone();
    glib::spawn_future_local(async move {
        while let Ok(event) = receiver.recv().await {
            match event {
                OperationEvent::Progress(p) => {
                    widgets::progress_toast::update(&progress, op_id, &p)
                }
                OperationEvent::Conflict(conflict, answer_tx) => {
                    dialogs::conflict_dialog::show(&window, &conflict, move |decision| {
                        let _ = answer_tx.send_blocking(decision);
                    });
                }
                OperationEvent::Done(outcome) => {
                    widgets::progress_toast::finish(&progress, op_id);
                    for (state, chrome) in &refresh_targets {
                        refresh(state, chrome);
                    }
                    // Faz 33: Move/Copy/Trash successes become an undo
                    // record; a permanent delete instead purges any pending
                    // record referencing what it just removed (Rule #39 — a
                    // permanent delete is never itself undoable, and a
                    // stale record pointing at a now-gone path must not
                    // linger to fail later).
                    match kind {
                        OperationKind::Move if !outcome.moved.is_empty() => {
                            undo_stack
                                .borrow_mut()
                                .record(UndoableAction::Move(outcome.moved.clone()));
                        }
                        OperationKind::Copy if !outcome.copied.is_empty() => {
                            undo_stack
                                .borrow_mut()
                                .record(UndoableAction::Copy(outcome.copied.clone()));
                        }
                        OperationKind::Trash if !outcome.trashed.is_empty() => {
                            undo_stack
                                .borrow_mut()
                                .record(UndoableAction::Trash(outcome.trashed.clone()));
                        }
                        OperationKind::Delete if !outcome.completed.is_empty() => {
                            undo_stack.borrow_mut().purge(&outcome.completed);
                        }
                        _ => {}
                    }
                    if !outcome.errors.is_empty() {
                        for (path, err) in &outcome.errors {
                            tracing::warn!(path = %veyra_core::security::log_path(path), error = %err, "bulk operation error");
                        }
                        // Faz 28: If all errors are permission-denied and pkexec is available,
                        // offer retry-as-admin.
                        let all_permission_denied = outcome.errors.iter().all(|(_, e)| {
                            matches!(e, veyra_filesystem::FsError::PermissionDenied(_))
                        });

                        if all_permission_denied && crate::privileged::is_available() {
                            show_bulk_retry_admin_dialog(
                                &window,
                                kind,
                                destination.clone(),
                                &outcome,
                                refresh_targets.clone(),
                            );
                        } else if let Some((_, chrome)) = refresh_targets.first() {
                            chrome.status_left.set_label(&format!(
                                "{} error(s) during operation",
                                outcome.errors.len()
                            ));
                        }
                    }
                    break;
                }
            }
        }
    });
}

/// Faz 28: Shows retry-as-admin dialog for bulk operations that failed with
/// permission-denied errors, allowing the user to retry the failed items with
/// elevated privileges.
fn show_bulk_retry_admin_dialog(
    window: &adw::ApplicationWindow,
    kind: OperationKind,
    destination: Option<VeyraPath>,
    outcome: &veyra_filesystem::OperationOutcome,
    refresh_targets: Vec<(SharedState, Chrome)>,
) {
    if outcome.errors.is_empty() {
        return;
    }

    let heading = match kind {
        OperationKind::Delete => "Delete Permission Denied",
        OperationKind::Move => "Move Permission Denied",
        OperationKind::Copy => "Copy Permission Denied",
        OperationKind::Trash => return, // Don't offer root for trash
    };

    let failed_paths: Vec<VeyraPath> = outcome
        .errors
        .iter()
        .map(|(path, _)| path.clone())
        .collect();

    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(format!(
            "{} item(s) couldn't be {} due to insufficient permissions.",
            failed_paths.len(),
            match kind {
                OperationKind::Delete => "deleted",
                OperationKind::Move => "moved",
                OperationKind::Copy => "copied",
                OperationKind::Trash => "trashed",
            }
        ))
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("retry", "Retry as Administrator")]);
    dialog.set_response_appearance("retry", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("retry"));

    let dialog_window = window.clone();
    dialog.connect_response(None, move |_, response| {
        if response != "retry" {
            return;
        }
        let failed_paths = failed_paths.clone();
        let destination = destination.clone();
        let refresh_targets = refresh_targets.clone();
        let window = dialog_window.clone();
        fs_async::run_blocking(
            move || {
                let dest_local = destination.as_ref().and_then(VeyraPath::as_local_path);
                let mut succeeded = 0u64;
                let mut failed = 0u64;
                for path in &failed_paths {
                    let Some(local_path) = path.as_local_path() else {
                        failed += 1;
                        continue;
                    };
                    let result = match kind {
                        OperationKind::Delete => crate::privileged::remove(local_path, true),
                        OperationKind::Move | OperationKind::Copy => {
                            let (Some(dest_dir), Some(name)) = (dest_local, path.file_name())
                            else {
                                failed += 1;
                                continue;
                            };
                            let target = dest_dir.join(name);
                            if kind == OperationKind::Move {
                                crate::privileged::r#move(local_path, &target)
                            } else {
                                crate::privileged::copy(local_path, &target, true)
                            }
                        }
                        OperationKind::Trash => continue,
                    };
                    match result {
                        Ok(()) => succeeded += 1,
                        Err(err) => {
                            tracing::warn!(path = %veyra_core::security::log_path(path), error = %err, "privileged retry failed");
                            failed += 1;
                        }
                    }
                }
                (succeeded, failed)
            },
            move |(succeeded, failed)| {
                for (state, chrome) in &refresh_targets {
                    refresh(state, chrome);
                }
                if let Some((_, chrome)) = refresh_targets.first() {
                    chrome.status_left.set_label(&format!(
                        "Administrator retry: {succeeded} succeeded, {failed} failed"
                    ));
                }
                if failed > 0 {
                    show_error_dialog(
                        &window,
                        "Some Items Still Failed",
                        &format!("{failed} item(s) could not be completed even with administrator privileges."),
                    );
                }
            },
        );
    });

    dialog.present(Some(window));
}

/// Builds the Faz 26 DND execution closure shared by every drag-and-drop
/// target in the window (view backgrounds, folder rows, breadcrumbs,
/// sidebar bookmarks): resolves a Copy/Move through the same
/// `run_bulk_operation` bridge every clipboard/context-menu action already
/// uses (Faz 5 `OperationQueue`, progress toast, conflict dialog included),
/// and a Create-Link through the simpler synchronous `dnd::create_links`.
/// Refreshes every open tab in both panels afterwards, since a drop's
/// source and destination directories can each be in either panel or tab.
fn build_dnd_executor(
    window: &adw::ApplicationWindow,
    panels: &Panels,
    progress: &ProgressToastHandles,
    undo_stack: &SharedUndoStack,
) -> dnd::DropExecutor {
    let window = window.clone();
    let panels = panels.clone();
    let progress = progress.clone();
    let undo_stack = undo_stack.clone();
    Rc::new(
        move |sources: Vec<VeyraPath>, destination: VeyraPath, action: dnd::DndAction| match action
        {
            dnd::DndAction::Move => run_bulk_operation(
                &window,
                all_tab_refresh_targets(&panels),
                &progress,
                OperationKind::Move,
                sources,
                Some(destination),
                &undo_stack,
            ),
            dnd::DndAction::Copy => run_bulk_operation(
                &window,
                all_tab_refresh_targets(&panels),
                &progress,
                OperationKind::Copy,
                sources,
                Some(destination),
                &undo_stack,
            ),
            dnd::DndAction::Link => {
                let panels = panels.clone();
                dnd::create_links(&window, sources, destination, move || {
                    for (state, chrome) in all_tab_refresh_targets(&panels) {
                        refresh(&state, &chrome);
                    }
                });
            }
        },
    )
}

/// Every currently open tab across both panels, paired with the panel
/// `Chrome` `refresh` needs to update it — the DND executor's refresh
/// target list, since a drop's source and destination tabs may each be in
/// either panel, or in a tab other than the one currently visible.
fn all_tab_refresh_targets(panels: &Panels) -> Vec<(SharedState, Chrome)> {
    let mut targets = Vec::new();
    for panel in [&panels.left, &panels.right] {
        for tab in panel.registry.borrow().values() {
            targets.push((tab.state.clone(), panel.chrome.clone()));
        }
    }
    targets
}

/// Drives a Faz 19 compress/extract event stream, the archive-ops
/// counterpart to `run_bulk_operation`: progress updates the same bottom
/// progress bar (relabeled via `verb`), and completion refreshes every
/// `(state, chrome)` in `refresh_targets` and surfaces errors/skips on the
/// first target's status bar. There is no conflict round trip here —
/// `extract_archive` never prompts, it just skips unsafe/symlink entries
/// (Rule #21/#22) and reports them in `ArchiveOutcome::skipped`.
fn run_archive_operation(
    window: &adw::ApplicationWindow,
    refresh_targets: Vec<(SharedState, Chrome)>,
    progress: &ProgressToastHandles,
    verb: &'static str,
    control: veyra_filesystem::OperationControl,
    receiver: async_channel::Receiver<archive_ops::ArchiveEvent>,
) {
    let op_id = widgets::progress_toast::begin_with_verb(progress, &control, verb);

    let window = window.clone();
    let progress = progress.clone();
    glib::spawn_future_local(async move {
        while let Ok(event) = receiver.recv().await {
            match event {
                archive_ops::ArchiveEvent::Progress(p) => {
                    widgets::progress_toast::update(&progress, op_id, &p)
                }
                archive_ops::ArchiveEvent::Done(result) => {
                    widgets::progress_toast::finish(&progress, op_id);
                    for (state, chrome) in &refresh_targets {
                        refresh(state, chrome);
                    }
                    let Some((_, chrome)) = refresh_targets.first() else {
                        break;
                    };
                    match result {
                        Ok(outcome) if outcome.cancelled => {
                            chrome.status_left.set_label("Cancelled");
                        }
                        Ok(outcome) => {
                            for (name, err) in &outcome.errors {
                                tracing::warn!(entry = %name, error = %err, "archive operation error");
                            }
                            if !outcome.errors.is_empty() {
                                chrome.status_left.set_label(&format!(
                                    "{} error(s) during archive operation",
                                    outcome.errors.len()
                                ));
                            } else if !outcome.skipped.is_empty() {
                                chrome.status_left.set_label(&format!(
                                    "{} unsafe entr{} skipped",
                                    outcome.skipped.len(),
                                    if outcome.skipped.len() == 1 {
                                        "y"
                                    } else {
                                        "ies"
                                    }
                                ));
                            }
                        }
                        Err(err) => {
                            tracing::warn!(error = %err, "archive operation failed");
                            chrome.status_left.set_label("Archive failed");
                            show_error_dialog(&window, &format!("{verb} Failed"), &err.to_string());
                        }
                    }
                    break;
                }
            }
        }
    });
}

/// Navigates `tab` to its current directory's parent, if any (no-op at the
/// filesystem root).
fn go_up(tab: &TabPage, chrome: &Chrome) {
    let parent = tab
        .state
        .borrow()
        .current_dir
        .as_local_path()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf());
    if let Some(parent) = parent {
        navigate_to(tab, chrome, VeyraPath::from_local(parent), true);
    }
}

/// Re-reads the current directory without touching navigation history.
fn refresh(state: &SharedState, chrome: &Chrome) {
    let path = state.borrow().current_dir.clone();
    load_directory(state, chrome, path);
}

/// Re-reads every open tab in both panels (Faz 34): the Preferences
/// dialog's "Icon Size" and "Directory Stream Chunk Size" settings both need
/// every already-open tab's listing rebuilt for the new value to actually
/// show — this reuses the exact reload path `win.refresh`/`F5` already
/// drives per-tab, just applied to every tab at once.
pub(crate) fn refresh_all_tabs(panels: &Panels) {
    for panel in [&panels.left, &panels.right] {
        for tab in panel.registry.borrow().values() {
            refresh(&tab.state, &panel.chrome);
        }
    }
}

/// Snapshots every open tab's current directory in `panel`, in tab order
/// (`AdwTabView::pages()`'s own model, not the unordered `TabRegistry`
/// `HashMap`) — backs Faz 34's session-on-close save for "Restore Previous
/// Tabs on Startup".
fn collect_tab_paths(panel: &Panel) -> Vec<String> {
    let pages = panel.tab_view.pages();
    let registry = panel.registry.borrow();
    (0..pages.n_items())
        .filter_map(|i| pages.item(i))
        .filter_map(|obj| obj.downcast::<adw::TabPage>().ok())
        .filter_map(|page| registry.get(&page).cloned())
        .map(|tab| tab.state.borrow().current_dir.to_string())
        .collect()
}

/// ANDs the free-text search query, the tab's active `QuickFilter` (Faz 13),
/// and the hidden-files toggle (Faz 14): an item must pass all three to
/// remain visible. `is_hidden` already covers both dotfiles and a
/// directory's `.hidden` listing — GIO's `standard::is-hidden` computes it
/// for us (see `veyra-filesystem`'s `build_file_item`), so no extra
/// filesystem-side work is needed here.
fn build_combined_filter(
    query: Rc<RefCell<String>>,
    quick_filter: Rc<RefCell<crate::sorting::QuickFilter>>,
    show_hidden: Rc<RefCell<bool>>,
) -> gtk4::CustomFilter {
    gtk4::CustomFilter::new(move |object| {
        let Some(boxed) = object.downcast_ref::<gtk4::glib::BoxedAnyObject>() else {
            return true;
        };
        let item = boxed.borrow::<FileItem>();

        if !crate::sorting::passes_hidden_filter(&item, *show_hidden.borrow()) {
            return false;
        }

        let needle = query.borrow();
        if !needle.is_empty() && !item.name().to_lowercase().contains(&needle.to_lowercase()) {
            return false;
        }

        crate::sorting::quick_filter_matches(&item, *quick_filter.borrow(), chrono::Utc::now())
    })
}

/// Navigates `tab` to `path`, pushing the previous location onto its back
/// stack when `push_history` is true (false is used for a tab's initial
/// load and for back/forward navigation, which manage the stacks
/// themselves).
fn navigate_to(tab: &TabPage, chrome: &Chrome, path: VeyraPath, push_history: bool) {
    {
        let mut state_mut = tab.state.borrow_mut();
        if push_history {
            let previous = state_mut.current_dir.clone();
            state_mut.history.record(previous);
        }
        state_mut.current_dir = path.clone();
    }

    if recent::is_recent_location(&path) {
        *tab.sort_config.borrow_mut() = crate::sorting::SortConfig {
            key: crate::sorting::SortKey::Accessed,
            order: crate::sorting::SortOrder::Descending,
            folders_first: false,
        };
        tab.resort();
    }

    update_chrome(tab, chrome);
    load_directory(&tab.state, chrome, path);
}

fn go_back(tab: &TabPage, chrome: &Chrome) {
    let target = {
        let mut state_mut = tab.state.borrow_mut();
        let current = state_mut.current_dir.clone();
        let Some(previous) = state_mut.history.go_back(current) else {
            return;
        };
        state_mut.current_dir = previous.clone();
        previous
    };
    update_chrome(tab, chrome);
    load_directory(&tab.state, chrome, target);
}

fn go_forward(tab: &TabPage, chrome: &Chrome) {
    let target = {
        let mut state_mut = tab.state.borrow_mut();
        let current = state_mut.current_dir.clone();
        let Some(next) = state_mut.history.go_forward(current) else {
            return;
        };
        state_mut.current_dir = next.clone();
        next
    };
    update_chrome(tab, chrome);
    load_directory(&tab.state, chrome, target);
}

/// Refreshes `tab`'s owning panel's chrome (nav button sensitivity, address
/// entry, breadcrumbs, status bar item count) plus `tab`'s own `AdwTabPage`
/// title, to reflect `tab`'s current state. Called on every navigation and
/// whenever the active tab within a panel changes.
fn update_chrome(tab: &TabPage, chrome: &Chrome) {
    let (current_dir, item_count) = {
        let state_ref = tab.state.borrow();
        chrome.back_button.set_sensitive(state_ref.can_go_back());
        chrome
            .forward_button
            .set_sensitive(state_ref.can_go_forward());
        chrome.up_button.set_sensitive(state_ref.can_go_up());
        chrome
            .address_entry
            .set_text(&state_ref.current_dir.to_string());
        (state_ref.current_dir.clone(), state_ref.model.n_items())
    };

    tab.adw_page.set_title(&tab_title(&current_dir));

    let navigate: Rc<dyn Fn(VeyraPath)> = {
        let tab = tab.clone();
        let chrome = chrome.clone();
        Rc::new(move |path: VeyraPath| navigate_to(&tab, &chrome, path, true))
    };
    breadcrumbs::rebuild(
        &chrome.breadcrumbs_box,
        &current_dir,
        navigate,
        tab.dnd_execute.clone(),
    );

    let is_recent = recent::is_recent_location(&current_dir);
    chrome.recent_banner.revealer.set_reveal_child(is_recent);
    if is_recent {
        chrome
            .recent_banner
            .privacy_switch
            .set_active(*chrome.privacy_mode.borrow());
    }

    let is_trash = trash::is_trash_location(&current_dir);
    chrome.trash_banner.revealer.set_reveal_child(is_trash);

    chrome.status_left.set_label(&count_label(item_count));
    update_free_space(&tab.state, chrome);
}

/// The `AdwTabPage` title for `path`: `Home` for the user's home directory,
/// otherwise its final path component (falling back to the raw path for
/// roots and other edge cases with no final component).
fn tab_title(path: &VeyraPath) -> String {
    match path {
        VeyraPath::Local(local) => {
            if *local == gtk4::glib::home_dir() {
                "Home".to_string()
            } else {
                local
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string())
            }
        }
        VeyraPath::Uri(uri) => uri
            .trim_end_matches('/')
            .rsplit_once('/')
            .map(|(_, last)| last.to_string())
            .filter(|last| !last.is_empty())
            .unwrap_or_else(|| path.to_string()),
    }
}

fn load_directory(state: &SharedState, chrome: &Chrome, path: VeyraPath) {
    // Cancel whatever chunked scan is still streaming into this tab's model
    // (Rule #13) — otherwise a slow scan of a huge directory the user has
    // already navigated away from keeps appending stale entries after this
    // call replaces it.
    if let Some(previous) = state.borrow_mut().load_control.take() {
        previous.cancel();
    }

    chrome.status_left.set_label("Loading…");

    let state_for_done = state.clone();
    let chrome_for_done = chrome.clone();

    // `recent:///` isn't a real GVfs mount, so it can't go through the
    // generic `read_dir` below — it's built from the XDG recent-files
    // registry instead (Faz 15). `snapshot_entries` must run here, on the
    // GTK main thread (`RecentManager` requirement); the actual per-entry
    // `stat`s happen off-thread in `list_recent_items`, same as every other
    // location's listing.
    if recent::is_recent_location(&path) {
        let entries = recent::snapshot_entries();
        fs_async::run_blocking(
            move || Ok(recent::list_recent_items(entries)),
            move |result| on_directory_loaded(&state_for_done, &chrome_for_done, result),
        );
        return;
    }

    // `trash:///` is likewise not a real GVfs mount `read_dir` can walk
    // (Faz 18) — `veyra_filesystem::list_trash` reads the home trash's
    // `files/` directory directly instead, same non-blocking contract.
    if trash::is_trash_location(&path) {
        fs_async::run_blocking(veyra_filesystem::list_trash, move |result| {
            on_directory_loaded(&state_for_done, &chrome_for_done, result)
        });
        return;
    }

    // The regular filesystem path streams in `READ_DIR_CHUNK_SIZE`-sized
    // batches so a huge directory (100,000+ entries, Rule #30) paints its
    // first entries almost immediately instead of blocking the model update
    // on the entire scan.
    state.borrow().model.remove_all();

    let control = OperationControl::new();
    state.borrow_mut().load_control = Some(control.clone());

    let model_for_chunk = state.borrow().model.clone();
    let chrome_for_chunk = chrome.clone();
    let control_for_chunk = control.clone();
    let total_bytes = Rc::new(std::cell::Cell::new(0u64));
    let total_bytes_for_chunk = total_bytes.clone();

    let model_for_done = state.borrow().model.clone();
    let control_for_done = control.clone();

    let control_for_worker = control.clone();
    // Faz 34: "Directory Stream Chunk Size" — falls back to
    // `READ_DIR_CHUNK_SIZE` (the same default `VeyraSettings` itself uses)
    // if it somehow reads as zero rather than handing the filesystem layer
    // a chunk size that would never emit anything.
    let stream_chunk_size = match chrome.settings.borrow().stream_chunk_size {
        0 => veyra_filesystem::READ_DIR_CHUNK_SIZE,
        n => n,
    };
    fs_async::run_streaming(
        move |emit| {
            veyra_filesystem::read_dir_chunked(&path, stream_chunk_size, &control_for_worker, emit)
        },
        move |chunk: Vec<FileItem>| {
            if control_for_chunk.is_cancelled() {
                return;
            }
            let mut chunk_bytes = 0u64;
            for item in &chunk {
                chunk_bytes = chunk_bytes.saturating_add(item.metadata.size_bytes);
            }
            total_bytes_for_chunk.set(total_bytes_for_chunk.get().saturating_add(chunk_bytes));
            for item in chunk {
                model_for_chunk.append(&gtk4::glib::BoxedAnyObject::new(item));
            }
            let count = model_for_chunk.n_items();
            chrome_for_chunk
                .status_left
                .set_label(&format!("Loading… ({})", count_label(count)));
        },
        move |result: Result<(), veyra_filesystem::FsError>| {
            if control_for_done.is_cancelled() {
                return;
            }
            state_for_done.borrow_mut().load_control = None;
            match result {
                Ok(()) => {
                    let count = model_for_done.n_items();
                    let size = veyra_filesystem::format_size(total_bytes.get());
                    chrome_for_done
                        .status_left
                        .set_label(&format!("{} ({size})", count_label(count)));
                    update_free_space(&state_for_done, &chrome_for_done);
                }
                Err(err) => {
                    tracing::warn!(error = %err, "failed to read directory");
                    chrome_for_done
                        .status_left
                        .set_label(&format!("Error: {err}"));
                    chrome_for_done.status_right.set_label("");
                }
            }
        },
    );
}

fn on_directory_loaded(
    state: &SharedState,
    chrome: &Chrome,
    result: Result<Vec<FileItem>, veyra_filesystem::FsError>,
) {
    match result {
        Ok(items) => {
            let count = items.len() as u32;
            if recent::is_recent_location(&state.borrow().current_dir) {
                chrome
                    .recent_banner
                    .title_label
                    .set_label(&recent::format_group_summary(&items, chrono::Utc::now()));
            }
            if trash::is_trash_location(&state.borrow().current_dir) {
                chrome
                    .trash_banner
                    .title_label
                    .set_label(&trash::format_summary(&items));
            }
            let model = state.borrow().model.clone();
            model.remove_all();
            for item in items {
                model.append(&gtk4::glib::BoxedAnyObject::new(item));
            }
            chrome.status_left.set_label(&count_label(count));
            update_free_space(state, chrome);
        }
        Err(err) => {
            tracing::warn!(error = %err, "failed to read directory");
            chrome.status_left.set_label(&format!("Error: {err}"));
            chrome.status_right.set_label("");
        }
    }
}

fn count_label(count: u32) -> String {
    if count == 1 {
        "1 item".to_string()
    } else {
        format!("{count} items")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_title_uses_home_label_for_home_directory() {
        let home = VeyraPath::from_local(gtk4::glib::home_dir());
        assert_eq!(tab_title(&home), "Home");
    }

    #[test]
    fn tab_title_uses_final_local_path_component() {
        let path = VeyraPath::from_local(gtk4::glib::home_dir().join("Projects/Veyra"));
        assert_eq!(tab_title(&path), "Veyra");
    }

    #[test]
    fn tab_title_falls_back_to_raw_path_for_filesystem_root() {
        let root = VeyraPath::from_local(std::path::PathBuf::from("/"));
        assert_eq!(tab_title(&root), "/");
    }

    #[test]
    fn tab_title_falls_back_to_raw_uri_when_no_segment_remains() {
        let uri = VeyraPath::from_uri("trash:///");
        assert_eq!(tab_title(&uri), "trash:///");
    }

    #[test]
    fn tab_title_uses_final_uri_path_segment_when_present() {
        let uri = VeyraPath::from_uri("sftp://host/remote/folder");
        assert_eq!(tab_title(&uri), "folder");
    }

    #[test]
    fn count_label_pluralizes_correctly() {
        assert_eq!(count_label(0), "0 items");
        assert_eq!(count_label(1), "1 item");
        assert_eq!(count_label(2), "2 items");
    }
}
