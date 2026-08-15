use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use veyra_filesystem::{FileItem, OperationKind, OperationRequest, VeyraPath};
use veyra_search::SearchIndex;

use crate::operations::OperationEvent;
use crate::preview::{self, PreviewPanelHandles};
use crate::split_view::{self, Chrome, Panel, PanelId, Panels};
use crate::state::{AppState, SharedState};
use crate::tab_page::{active_tab, TabPage, TabRegistry, ViewSelections};
use crate::views::ViewMode;
use crate::widgets::progress_toast::ProgressToastHandles;
use crate::{breadcrumbs, dialogs, fs_async, headerbar, operations, sidebar, widgets};

/// A single Copy/Cut clipboard slot, shared across both panels and all their
/// tabs (Faz 5 operates on the current selection, not a multi-item marquee —
/// see `AGENTS.md` scope note in the Faz 5 changelog entry). `cut` decides
/// whether `win.paste` runs a Move or a Copy.
#[derive(Clone)]
struct ClipboardEntry {
    path: VeyraPath,
    cut: bool,
}

pub(crate) fn build_window(
    app: &adw::Application,
    start_dir: VeyraPath,
    cache_dir: &Path,
) -> adw::ApplicationWindow {
    let left = split_view::build_panel(PanelId::Left);
    let right = split_view::build_panel(PanelId::Right);
    right.frame.set_visible(false);
    let panels = Panels { left, right };

    let focused: Rc<RefCell<PanelId>> = Rc::new(RefCell::new(PanelId::Left));

    let clipboard: Rc<RefCell<Option<ClipboardEntry>>> = Rc::new(RefCell::new(None));
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

    let preview = preview::build();
    let refresh_preview: Rc<dyn Fn()> = {
        let panels = panels.clone();
        let focused = focused.clone();
        let preview = preview.clone();
        Rc::new(move || sync_preview(&panels, &focused, &preview))
    };

    let search_index = Arc::new(open_search_index(cache_dir));
    veyra_search::spawn_background_index(search_index.clone(), glib::home_dir());

    let thumbnails = crate::thumbnails::ThumbnailService::new(cache_dir.join("thumbnails"));

    let header = headerbar::build(
        &panels,
        focused.clone(),
        search_index,
        navigate.clone(),
        refresh_preview.clone(),
    );
    let progress = widgets::progress_toast::build();

    wire_panel(&panels.left, &focused, &header, &refresh_preview);
    wire_panel(&panels.right, &focused, &header, &refresh_preview);
    attach_focus_gesture(&panels.left, &panels, &focused, &header, &refresh_preview);
    attach_focus_gesture(&panels.right, &panels, &focused, &header, &refresh_preview);
    attach_panel_focus_key(&panels.left, &panels, &focused, &header, &refresh_preview);
    attach_panel_focus_key(&panels.right, &panels, &focused, &header, &refresh_preview);

    open_tab(
        &panels.left.tab_view,
        &panels.left.registry,
        &panels.left.chrome,
        has_clipboard.clone(),
        split_active.clone(),
        refresh_preview.clone(),
        thumbnails.clone(),
        start_dir,
    );
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

    preview.widget.set_visible(false);
    let content_paned = gtk4::Paned::new(gtk4::Orientation::Horizontal);
    content_paned.set_start_child(Some(&paned));
    content_paned.set_end_child(Some(&preview.widget));
    content_paned.set_resize_start_child(true);
    content_paned.set_resize_end_child(false);
    content_paned.set_shrink_start_child(false);
    content_paned.set_shrink_end_child(true);
    content_paned.set_position(880);

    let content_page = adw::NavigationPage::new(&content_paned, "Files");
    let sidebar_widget = sidebar::build(navigate);
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
    );
    setup_operation_actions(app, &window, &panels, &focused, &progress, &clipboard);
    setup_context_menu_actions(
        app,
        &window,
        &panels,
        &focused,
        has_clipboard,
        split_active,
        refresh_preview.clone(),
        thumbnails.clone(),
    );
    setup_properties_actions(app, &window, &panels, &focused, thumbnails);
    setup_preview_actions(app, &window, &preview, &header, refresh_preview);

    window
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
) {
    let action_new_tab = gio::SimpleAction::new("new-tab", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        let refresh_preview = refresh_preview.clone();
        let thumbnails = thumbnails.clone();
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
) {
    let action_toggle_split = gio::SimpleAction::new("toggle-split-view", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        let header = header.clone();
        let refresh_preview = refresh_preview.clone();
        let thumbnails = thumbnails.clone();
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
        action_copy_other.connect_activate(move |_, _| {
            transfer_to_other_panel(&window, &panels, &focused, &progress, OperationKind::Copy);
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
        action_move_other.connect_activate(move |_, _| {
            transfer_to_other_panel(&window, &panels, &focused, &progress, OperationKind::Move);
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
    let Some(item) = source_tab.selections.selected(&source_tab.view_stack) else {
        return;
    };
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
        vec![item.path],
        Some(destination),
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
            if let Some(item) = tab.selections.selected(&tab.view_stack) {
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
        let panels = panels.clone();
        let focused = focused.clone();
        let clipboard = clipboard.clone();
        action_cut.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            if let Some(item) = tab.selections.selected(&tab.view_stack) {
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
        let panels = panels.clone();
        let focused = focused.clone();
        let progress = progress.clone();
        let clipboard = clipboard.clone();
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
        let panels = panels.clone();
        let focused = focused.clone();
        let progress = progress.clone();
        action_trash.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            if let Some(item) = tab.selections.selected(&tab.view_stack) {
                run_bulk_operation(
                    &window,
                    vec![(tab.state.clone(), panel.chrome.clone())],
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
        let panels = panels.clone();
        let focused = focused.clone();
        let progress = progress.clone();
        action_delete.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            let Some(tab) = active_tab(&panel.tab_view, &panel.registry) else {
                return;
            };
            let Some(item) = tab.selections.selected(&tab.view_stack) else {
                return;
            };
            let window_for_confirm = window.clone();
            let state = tab.state.clone();
            let chrome = panel.chrome.clone();
            let progress = progress.clone();
            let path = item.path;
            let path_for_delete = path.clone();
            dialogs::delete_confirm::show(&window, std::slice::from_ref(&path), move || {
                run_bulk_operation(
                    &window_for_confirm,
                    vec![(state, chrome)],
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
                show_open_with_dialog(&window, &item.path);
            }
        });
    }
    window.add_action(&action_open_with);

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
        action_create_folder.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
                create_child_entry(&tab.state, &panel.chrome, "New Folder", true);
            }
        });
    }
    window.add_action(&action_create_folder);

    let action_create_document = gio::SimpleAction::new("create-document", None);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        action_create_document.connect_activate(move |_, _| {
            let panel = panels.get(*focused.borrow()).clone();
            if let Some(tab) = active_tab(&panel.tab_view, &panel.registry) {
                create_child_entry(&tab.state, &panel.chrome, "New Document", false);
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
    start_dir: VeyraPath,
) -> TabPage {
    let state = AppState::new(start_dir.clone());
    let search_query = Rc::new(RefCell::new(String::new()));
    let quick_filter = Rc::new(RefCell::new(crate::sorting::QuickFilter::default()));
    let show_hidden = Rc::new(RefCell::new(false));
    let filter = build_combined_filter(
        search_query.clone(),
        quick_filter.clone(),
        show_hidden.clone(),
    );
    let sort_config = Rc::new(RefCell::new(crate::sorting::SortConfig::default()));
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
            thumbnails.clone(),
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
            thumbnails.clone(),
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
            thumbnails,
        );
        let details_selection = details.selection;
        view_stack.add_named(&details.widget, Some(ViewMode::Details.stack_name()));
        details_column_view_slot = details.column_view;
        details_sort_columns_slot = details.sort_columns;

        view_stack.set_visible_child_name(ViewMode::Icon.stack_name());

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
        std::thread::spawn(move || {
            if let Err(err) = veyra_filesystem::open(&item.path) {
                tracing::warn!(path = %item.path, error = %err, "failed to open item");
            }
        });
    }
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
// `AppChooserDialog` was deprecated in GTK 4.10 in favor of
// `GtkFileLauncher`-adjacent APIs that don't yet cover this dialog's exact
// "pick an app for this file" use case in gtk4-rs; revisit in a later phase.
#[allow(deprecated)]
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
                    for (state, chrome) in &refresh_targets {
                        refresh(state, chrome);
                    }
                    if !outcome.errors.is_empty() {
                        for (path, err) in &outcome.errors {
                            tracing::warn!(path = %path, error = %err, "bulk operation error");
                        }
                        if let Some((_, chrome)) = refresh_targets.first() {
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
    breadcrumbs::rebuild(&chrome.breadcrumbs_box, &current_dir, navigate);

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
            let count = items.len() as u32;
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
