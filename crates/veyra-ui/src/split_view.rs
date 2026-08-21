//! Faz 8: two independent panels (Left/Right) side by side in a resizable
//! `GtkPaned`. Each panel owns a complete Faz 3-7 navigation chrome (back/
//! forward/up/home/refresh, breadcrumbs, status bar) plus its own
//! `AdwTabView` — so the two panels are fully independent: separate
//! location, history, tabs, and view state, mirroring Krusader-style
//! dual-pane file managers. Only the window-level search entry and
//! view-mode switcher (in the header bar) act on whichever panel currently
//! has focus — see `window::focus_panel`.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

use crate::config::SharedSettings;
use crate::i18n::t;
use crate::recent::{self, RecentBannerHandles};
use crate::tab_page::{active_tab, TabPage, TabRegistry};
use crate::trash::{self, TrashBannerHandles};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelId {
    Left,
    Right,
}

impl PanelId {
    /// The panel on the opposite side — the destination for "Copy/Move to
    /// Other Panel" when `self` is the focused (source) panel.
    pub(crate) fn other(self) -> Self {
        match self {
            PanelId::Left => PanelId::Right,
            PanelId::Right => PanelId::Left,
        }
    }
}

/// The navigation chrome a single panel owns outright: nav buttons,
/// breadcrumbs/address entry, and the item-count/free-space status row.
/// Unlike Faz 7's window-wide chrome, every field here belongs to exactly
/// one panel, so panels never need to coordinate about who gets to write to
/// them — a panel's own back/forward/refresh always act on that panel,
/// regardless of which panel currently has focus.
#[derive(Clone)]
pub(crate) struct Chrome {
    pub back_button: gtk4::Button,
    pub forward_button: gtk4::Button,
    pub up_button: gtk4::Button,
    pub home_button: gtk4::Button,
    pub refresh_button: gtk4::Button,
    pub breadcrumbs_box: gtk4::Box,
    pub title_stack: gtk4::Stack,
    pub address_entry: gtk4::Entry,
    pub status_left: gtk4::Label,
    pub status_right: gtk4::Label,
    /// Faz 39: the Git status badge (`main ↑2 ↓1 *`), hidden whenever the
    /// current directory isn't inside a Git repository.
    pub git_badge: gtk4::Label,
    /// Faz 15: the `recent:///`-only info row (Clear History / Privacy
    /// Mode), revealed by `window::update_chrome` whenever this panel's
    /// active tab is showing Recent Files.
    pub recent_banner: RecentBannerHandles,
    /// Faz 15: app-wide privacy toggle, shared by both panels — `true`
    /// means opening a file never adds it to the XDG recent-files registry.
    pub privacy_mode: Rc<RefCell<bool>>,
    /// Faz 18: the `trash:///`-only info row (item/size summary + "Empty
    /// Trash"), revealed by `window::update_chrome` whenever this panel's
    /// active tab is showing Trash.
    pub trash_banner: TrashBannerHandles,
    /// Faz 34: the app-wide preferences store, shared by both panels and
    /// every tab's views (icon size, click policy, per-tab defaults) — same
    /// sharing pattern as `privacy_mode` above.
    pub settings: SharedSettings,
    /// Faz 59: the single window-level `AdwToastOverlay`, shared by both
    /// panels (constructed once in `window::build_window`, before either
    /// panel exists, and threaded in here the same way `settings` already
    /// is) — lets any call site that already holds a `Chrome` (bulk-op
    /// completion, undo/redo, clipboard copy) surface a toast without a new
    /// parameter threaded through every intermediate function.
    pub toast_overlay: adw::ToastOverlay,
    /// Faz 62: every tagged path's color, shared by both panels — a tag is
    /// a property of the file, not of whichever directory it's currently
    /// listed under, so (unlike the per-directory `SharedGitStatuses`) this
    /// lives once at the app level and is reloaded (`crate::tags::reload`)
    /// after every `win.set-tag-selected`/`win.remove-tag-selected`.
    pub tags: crate::tags::SharedTags,
}

/// One independent panel: its own navigation chrome, its own `AdwTabView`
/// (and therefore its own tabs, each carrying the Faz 7 per-tab isolated
/// state), and the outer `frame` used for the active-panel highlight
/// (`install_panel_css`) and click-to-focus detection.
#[derive(Clone)]
pub(crate) struct Panel {
    pub id: PanelId,
    pub tab_view: adw::TabView,
    pub registry: TabRegistry,
    pub chrome: Chrome,
    pub frame: gtk4::Box,
}

/// Toggles the `.veyra-split-active` class on `window` — the ancestor the
/// `.veyra-split-active .veyra-panel.veyra-active-panel` CSS rule requires —
/// so the active-panel border can only ever appear while the right panel is
/// actually visible, never in single-panel mode.
pub(crate) fn set_split_active_class(window: &impl gtk4::prelude::WidgetExt, active: bool) {
    if active {
        window.add_css_class("veyra-split-active");
    } else {
        window.remove_css_class("veyra-split-active");
    }
}

impl Panel {
    pub fn set_highlighted(&self, active: bool) {
        if active {
            self.frame.add_css_class("veyra-active-panel");
        } else {
            self.frame.remove_css_class("veyra-active-panel");
        }
    }
}

#[derive(Clone)]
pub(crate) struct Panels {
    pub left: Panel,
    pub right: Panel,
}

impl Panels {
    pub fn get(&self, id: PanelId) -> &Panel {
        match id {
            PanelId::Left => &self.left,
            PanelId::Right => &self.right,
        }
    }
}

/// The `TabPage` behind whichever panel is currently focused, if it has any
/// tabs open yet (the right panel starts with zero tabs until Faz 8's split
/// view is activated for the first time).
pub(crate) fn focused_tab(panels: &Panels, focused: &Rc<RefCell<PanelId>>) -> Option<TabPage> {
    let panel = panels.get(*focused.borrow());
    active_tab(&panel.tab_view, &panel.registry)
}

/// Builds one panel's full navigation chrome + tab strip: back/forward/up/
/// home/refresh buttons, a breadcrumbs row that swaps into an editable
/// address entry on click (Esc/focus-out reverts), an `AdwTabBar` +
/// `AdwTabView`, and a status row (item count / free space). Mirrors the
/// single-panel header bar Faz 3-7 built, just scoped to one panel instead
/// of the whole window.
pub(crate) fn build_panel(
    id: PanelId,
    privacy_mode: Rc<RefCell<bool>>,
    settings: SharedSettings,
    toast_overlay: adw::ToastOverlay,
    tags: crate::tags::SharedTags,
) -> Panel {
    let back_button = nav_button("go-previous-symbolic", t("nav.back"), t("nav.back.tooltip"));
    let forward_button = nav_button(
        "go-next-symbolic",
        t("nav.forward"),
        t("nav.forward.tooltip"),
    );
    let up_button = nav_button("go-up-symbolic", t("nav.up"), t("nav.up.tooltip"));
    let home_button = nav_button("go-home-symbolic", t("nav.home"), t("nav.home"));
    let refresh_button = nav_button(
        "view-refresh-symbolic",
        t("nav.refresh"),
        t("nav.refresh.tooltip"),
    );

    let nav_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    nav_box.add_css_class("linked");
    nav_box.set_margin_start(4);
    nav_box.set_margin_top(4);
    nav_box.set_margin_bottom(4);
    nav_box.append(&back_button);
    nav_box.append(&forward_button);
    nav_box.append(&up_button);
    nav_box.append(&home_button);
    nav_box.append(&refresh_button);

    let breadcrumbs_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    breadcrumbs_box.set_halign(gtk4::Align::Start);
    breadcrumbs_box.set_hexpand(true);
    breadcrumbs_box.set_margin_start(8);

    let address_entry = gtk4::Entry::new();
    address_entry.set_hexpand(true);
    address_entry.set_margin_start(4);
    address_entry.set_margin_end(4);
    address_entry.set_tooltip_text(Some(t("nav.address.tooltip")));
    address_entry.update_property(&[gtk4::accessible::Property::Label(t(
        "nav.address.accessible_label",
    ))]);

    let title_stack = gtk4::Stack::new();
    title_stack.set_hexpand(true);
    title_stack.add_named(&breadcrumbs_box, Some("breadcrumbs"));
    title_stack.add_named(&address_entry, Some("address"));
    title_stack.set_visible_child_name("breadcrumbs");

    let breadcrumbs_click = gtk4::GestureClick::new();
    {
        let title_stack = title_stack.clone();
        let address_entry = address_entry.clone();
        breadcrumbs_click.connect_released(move |gesture, _, _, _| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            title_stack.set_visible_child_name("address");
            address_entry.grab_focus();
            address_entry.select_region(0, -1);
        });
    }
    breadcrumbs_box.add_controller(breadcrumbs_click);

    let address_key = gtk4::EventControllerKey::new();
    {
        let title_stack = title_stack.clone();
        let breadcrumbs_box = breadcrumbs_box.clone();
        address_key.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gtk4::gdk::Key::Escape {
                title_stack.set_visible_child_name("breadcrumbs");
                breadcrumbs_box.grab_focus();
                gtk4::glib::Propagation::Stop
            } else {
                gtk4::glib::Propagation::Proceed
            }
        });
    }
    address_entry.add_controller(address_key);

    let address_focus = gtk4::EventControllerFocus::new();
    {
        let title_stack = title_stack.clone();
        address_focus.connect_leave(move |_| {
            title_stack.set_visible_child_name("breadcrumbs");
        });
    }
    address_entry.add_controller(address_focus);

    // Faz 39: Git status badge — hidden by default, revealed by
    // `window::update_git_badge` whenever the panel's active tab is
    // currently showing a location inside a Git repository.
    let git_badge = gtk4::Label::new(None);
    git_badge.add_css_class("dim-label");
    git_badge.add_css_class("caption");
    git_badge.set_margin_start(4);
    git_badge.set_margin_end(8);
    git_badge.set_visible(false);
    git_badge.set_selectable(true);

    let toolbar_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    toolbar_row.append(&nav_box);
    toolbar_row.append(&title_stack);
    toolbar_row.append(&git_badge);

    let recent_banner = recent::build_banner(privacy_mode.clone());
    let trash_banner = trash::build_banner();

    let tab_view = adw::TabView::new();
    let tab_bar = adw::TabBar::new();
    tab_bar.set_view(Some(&tab_view));
    tab_view.set_vexpand(true);

    let status = crate::statusbar::build();

    let frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    frame.add_css_class("veyra-panel");
    frame.append(&toolbar_row);
    frame.append(&recent_banner.revealer);
    frame.append(&trash_banner.revealer);
    frame.append(&tab_bar);
    frame.append(&tab_view);
    frame.append(&status.widget);

    let chrome = Chrome {
        back_button,
        forward_button,
        up_button,
        home_button,
        refresh_button,
        breadcrumbs_box,
        title_stack,
        address_entry,
        status_left: status.left_label,
        status_right: status.right_label,
        git_badge,
        recent_banner,
        privacy_mode,
        trash_banner,
        settings,
        toast_overlay,
        tags,
    };

    Panel {
        id,
        tab_view,
        registry: TabRegistry::default(),
        chrome,
        frame,
    }
}

fn nav_button(icon_name: &str, accessible_label: &str, tooltip: &str) -> gtk4::Button {
    let button = gtk4::Button::from_icon_name(icon_name);
    button.set_tooltip_text(Some(tooltip));
    button.update_property(&[gtk4::accessible::Property::Label(accessible_label)]);
    button
}

/// Loads the CSS that draws the active-panel highlight border and installs
/// it for `display`. `.veyra-panel` never draws a border on its own — the
/// highlight only exists to tell the two panels of a *split* view apart, so
/// it is scoped under `.veyra-split-active` (toggled on the window by
/// `set_split_active_class` whenever the right panel's visibility changes)
/// and never appears in single-panel mode, even if `.veyra-active-panel`
/// were ever left stuck on a frame.
///
/// Also installs `.veyra-hidden-item` (Faz 14): dims and italicizes hidden
/// files/directories in every view when the tab's Ctrl+H toggle is showing
/// them, so they read as visually distinct from regular entries.
///
/// Faz 36 (Kural #29 — keyboard-first): a `:focus-visible` rule gives every
/// keyboard-focused *interactive* control (button, row, grid cell, entry,
/// breadcrumb) a solid 2px `@accent_color` outline with a 2px offset so it
/// never gets lost inside a widget's own border/selection styling — mouse
/// clicks don't trigger `:focus-visible`, so this never adds a ring around a
/// merely-clicked control, only a `Tab`/arrow-key-focused one. Scoped to
/// specific interactive selectors rather than `*` so structural containers
/// (boxes, panes, the window itself) never pick up a stray outline when
/// keyboard focus lands on them instead of a specific control — e.g. a
/// gridview/listview that gains focus without a row selected (as happens
/// right after `Ctrl+A` on an empty folder) used to outline its whole
/// scrolled area.
pub(crate) fn install_panel_css(display: &gtk4::gdk::Display) {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(
        ".veyra-panel { border: none; }\n\
         .veyra-split-active .veyra-panel.veyra-active-panel { \
           border: 2px solid @accent_color; border-radius: 6px; }\n\
         .veyra-hidden-item { opacity: 0.55; font-style: italic; }\n\
         .veyra-git-badge { font-size: smaller; font-weight: bold; margin-left: 4px; }\n\
         .veyra-git-badge.veyra-git-modified { color: #e5a50a; }\n\
         .veyra-git-badge.veyra-git-staged { color: #26a269; }\n\
         .veyra-git-badge.veyra-git-untracked { color: #3584e4; }\n\
         .veyra-git-badge.veyra-git-ignored { color: alpha(currentColor, 0.45); }\n\
         .veyra-git-badge.veyra-git-conflicted { color: #e01b24; }\n\
         .veyra-git-badge.veyra-git-deleted { color: #c64600; }\n\
         button:focus-visible, entry:focus-visible, listview > row:focus-visible, \
         gridview > child:focus-visible, .veyra-crumb:focus-visible { \
           outline: 2px solid @accent_color; outline-offset: 2px; }\n\
         /* Faz 59: modern pill breadcrumbs — soft hover wash, accent-tinted\n\
            capsule fill on the active (final) segment. */\n\
         .veyra-crumb { border-radius: 999px; padding-left: 10px; padding-right: 10px; \
           transition: background-color 150ms ease; }\n\
         .veyra-crumb:hover { background-color: alpha(@accent_bg_color, 0.14); }\n\
         .veyra-crumb-current { border-radius: 999px; padding: 2px 10px; \
           background-color: alpha(@accent_bg_color, 0.16); color: @accent_color; }\n\
         /* Faz 59: view-mode switcher (Icon/Compact/Details) — rounded\n\
            capsule group with an accent-tinted checked state. */\n\
         .veyra-view-switcher togglebutton { border-radius: 8px; transition: background-color 150ms ease; }\n\
         .veyra-view-switcher togglebutton:checked { \
           background-color: alpha(@accent_bg_color, 0.22); color: @accent_color; }\n\
         /* Faz 59: sidebar active-location indicator — a soft capsule fill\n\
            plus a 3px accent bar flush against the row's leading edge. */\n\
         .veyra-sidebar-active { \
           background-color: alpha(@accent_bg_color, 0.14); \
           border-radius: 8px; \
           box-shadow: inset 3px 0 0 0 @accent_color; }\n\
         /* Faz 59: elevated depth for empty-state and storage-panel cards,\n\
            matching modern Libadwaita's soft-shadow card language. */\n\
         .veyra-elevated-card { \
           border-radius: 12px; \
           border: 1px solid alpha(currentColor, 0.08); \
           box-shadow: 0 1px 3px alpha(black, 0.12); }\n\
         /* Faz 62: color tag pill (view rows) and dot (sidebar rows) — a\n\
            small glowing circle tinted per `TagColor`, sized differently\n\
            since a sidebar row has more room than a grid/column cell. */\n\
         .veyra-tag-pill { min-width: 10px; min-height: 10px; border-radius: 999px; \
           margin-left: 4px; box-shadow: 0 0 3px alpha(currentColor, 0.5); }\n\
         .veyra-tag-dot { min-width: 12px; min-height: 12px; border-radius: 999px; \
           margin-right: 2px; box-shadow: 0 0 3px alpha(currentColor, 0.5); }\n\
         .veyra-tag-red { background-color: #e01b24; color: #e01b24; }\n\
         .veyra-tag-orange { background-color: #ff7800; color: #ff7800; }\n\
         .veyra-tag-yellow { background-color: #f6d32d; color: #f6d32d; }\n\
         .veyra-tag-green { background-color: #26a269; color: #26a269; }\n\
         .veyra-tag-blue { background-color: #3584e4; color: #3584e4; }\n\
         .veyra-tag-purple { background-color: #9141ac; color: #9141ac; }",
    );
    gtk4::style_context_add_provider_for_display(
        display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    watch_high_contrast(display);
}

/// Faz 36: installs (and keeps live-synced with) extra CSS that widens
/// borders and darkens dim/secondary text whenever Libadwaita's system
/// high-contrast mode (`AdwStyleManager::is-high-contrast`) is on — on top
/// of, not instead of, GTK's own high-contrast theme, since that already
/// swaps the whole color palette; this only reinforces the two things a
/// file manager draws itself: panel borders and dimmed hidden-file text.
/// Re-evaluated on every `notify::high-contrast` so toggling the system
/// setting takes effect immediately, matching `apply_accent_color`'s
/// install/remove-on-reapply pattern in `config.rs`.
fn watch_high_contrast(display: &gtk4::gdk::Display) {
    thread_local! {
        static HIGH_CONTRAST_PROVIDER: RefCell<Option<gtk4::CssProvider>> = const { RefCell::new(None) };
    }

    fn sync(display: &gtk4::gdk::Display, enabled: bool) {
        HIGH_CONTRAST_PROVIDER.with(|cell| {
            if let Some(old) = cell.borrow_mut().take() {
                gtk4::style_context_remove_provider_for_display(display, &old);
            }
        });
        if !enabled {
            return;
        }
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(
            ".veyra-split-active .veyra-panel.veyra-active-panel { border-width: 3px; }\n\
             .veyra-hidden-item { opacity: 0.75; }\n\
             button:focus-visible, entry:focus-visible, listview > row:focus-visible, \
             gridview > child:focus-visible, .veyra-crumb:focus-visible { outline-width: 3px; }",
        );
        gtk4::style_context_add_provider_for_display(
            display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        HIGH_CONTRAST_PROVIDER.with(|cell| *cell.borrow_mut() = Some(provider));
    }

    let style_manager = adw::StyleManager::default();
    sync(display, style_manager.is_high_contrast());
    let display = display.clone();
    style_manager.connect_high_contrast_notify(move |manager| {
        sync(&display, manager.is_high_contrast());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn other_panel_swaps_left_and_right() {
        assert_eq!(PanelId::Left.other(), PanelId::Right);
        assert_eq!(PanelId::Right.other(), PanelId::Left);
    }

    #[test]
    fn other_panel_is_involutive() {
        assert_eq!(PanelId::Left.other().other(), PanelId::Left);
        assert_eq!(PanelId::Right.other().other(), PanelId::Right);
    }
}
