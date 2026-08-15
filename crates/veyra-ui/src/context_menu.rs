//! Faz 6: dynamic right-click context menus for the item views (Icon/
//! Compact/Details) and their background (empty-space) area.
//!
//! GTK4's `GtkGridView`/`GtkColumnView` select the item under the pointer on
//! *any* mouse button release (not just primary), so by the time our
//! secondary-button `GtkGestureClick` fires (bubble phase, after the view's
//! own row-selection gesture has already run at the target phase),
//! `selection.selected()` already reflects the clicked item — no manual
//! hit-to-position bookkeeping needed. A right-click that lands on empty
//! space (no row widget under the pointer) is detected via `Widget::pick`
//! returning the view widget itself rather than a row descendant, and opens
//! the background menu instead.
//!
//! Menu entries for actions not yet implemented in later phases (Compress/
//! Extract: Faz 19, Open Terminal Here: Faz 23) are shown insensitive, bound
//! to the shared `win.not-implemented` action, with the owning phase noted
//! in the label — per Rule #2 (no monolithic leaps across the roadmap).
//! "Open in New Tab" is real as of Faz 7. "Copy/Move to Other Panel" (Faz 8)
//! are real too, but only appear at all when the split view is currently
//! showing a second panel to act as the destination — there is nothing to
//! disable-and-label when the concept doesn't apply yet. "Properties" (Faz
//! 12) is real too: the item menu binds it to `win.properties-selected`,
//! the background menu to `win.properties-current` (the currently open
//! directory, since there's no selected item to act on there).

use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{gdk, gio};

use veyra_filesystem::{ArchiveFormat, FileItem};

use crate::open_with;
use crate::views::selected_item;

/// How many `open_with::recommended_apps` entries surface directly in the
/// right-click "Open With" submenu before the "Other Application…" entry —
/// keeps the submenu from growing unbounded on content types with a long
/// MIME-database association list.
const MAX_RECOMMENDED_IN_MENU: usize = 5;

/// Attaches a secondary-click (right-click) popover context menu to `view`
/// (the actual `GtkGridView`/`GtkColumnView`, not an enclosing
/// `GtkScrolledWindow` — hit-testing relies on `view` itself being the "no
/// item under the pointer" sentinel). `has_clipboard` is polled fresh on
/// every background-menu build so the Paste entry's action binding (real
/// `win.paste` vs. the shared disabled action) always matches the clipboard
/// at click time.
pub(crate) fn attach<V: IsA<gtk4::Widget> + Clone>(
    view: &V,
    selection: &gtk4::SingleSelection,
    has_clipboard: Rc<dyn Fn() -> bool>,
    split_active: Rc<dyn Fn() -> bool>,
    is_trash: Rc<dyn Fn() -> bool>,
) {
    let popover = gtk4::PopoverMenu::from_model(None::<&gio::MenuModel>);
    popover.set_parent(view);
    popover.set_has_arrow(false);
    popover.set_halign(gtk4::Align::Start);

    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gdk::BUTTON_SECONDARY);

    let view_widget: gtk4::Widget = view.clone().upcast();
    let selection = selection.clone();
    gesture.connect_released(move |gesture, _n_press, x, y| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);

        let hit_item = view_widget
            .pick(x, y, gtk4::PickFlags::DEFAULT)
            .is_some_and(|picked| picked != view_widget);

        let menu = if is_trash() {
            hit_item
                .then(|| selected_item(&selection))
                .flatten()
                .map(|_| build_trash_item_menu())
                .unwrap_or_else(build_trash_background_menu)
        } else {
            hit_item
                .then(|| selected_item(&selection))
                .flatten()
                .map(|item| build_item_menu(&item, split_active()))
                .unwrap_or_else(|| build_background_menu(has_clipboard()))
        };

        popover.set_menu_model(Some(&menu));
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.popup();
    });

    view.add_controller(gesture);
}

/// Builds the per-item menu for `trash:///`: only actions that make sense on
/// a trashed entry survive here — normal "Move to Trash", "Rename",
/// "Compress", "Open with" etc. are meaningless once something is already in
/// the Trash, so they're omitted entirely rather than shown disabled.
fn build_trash_item_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    let restore_section = gio::Menu::new();
    restore_section.append(Some("Restore"), Some("win.restore-selected"));
    restore_section.append(Some("Delete Permanently"), Some("win.delete-selection"));
    menu.append_section(None, &restore_section);

    let properties_section = gio::Menu::new();
    properties_section.append(Some("Properties"), Some("win.properties-selected"));
    menu.append_section(None, &properties_section);

    menu
}

/// Builds the empty-space (background) menu for `trash:///`.
fn build_trash_background_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    let empty_section = gio::Menu::new();
    empty_section.append(Some("Empty Trash"), Some("win.empty-trash"));
    menu.append_section(None, &empty_section);

    let properties_section = gio::Menu::new();
    properties_section.append(Some("Properties"), Some("win.properties-current"));
    menu.append_section(None, &properties_section);

    menu
}

/// Builds the per-item menu for `item`: entries that only make sense for a
/// directory (Open in New Tab/Window) or that only apply to a recognized
/// archive (Extract Here) are omitted or shown disabled accordingly.
/// `is_split_active` gates the Faz 8 "Copy/Move to Other Panel" entries —
/// they're omitted entirely (not shown-disabled) when there's no second
/// panel to act as the destination.
fn build_item_menu(item: &FileItem, is_split_active: bool) -> gio::Menu {
    let is_dir = item.kind().is_directory();
    let is_archive = is_archive_name(item.name());

    let menu = gio::Menu::new();

    let open_section = gio::Menu::new();
    open_section.append(Some("Open"), Some("win.open-selected"));
    open_section.append_submenu(Some("Open With"), &build_open_with_submenu(item));
    if is_dir {
        open_section.append(
            Some("Open in New Tab"),
            Some("win.open-in-new-tab-selected"),
        );
        open_section.append(
            Some("Open in New Window"),
            Some("win.open-in-new-window-selected"),
        );
    }
    menu.append_section(None, &open_section);

    if is_dir {
        let bookmark_section = gio::Menu::new();
        bookmark_section.append(
            Some("Add to Bookmarks"),
            Some("win.add-to-bookmarks-selected"),
        );
        menu.append_section(None, &bookmark_section);

        let analyze_section = gio::Menu::new();
        analyze_section.append(
            Some("Analyze Disk Usage…"),
            Some("win.analyze-disk-selected"),
        );
        menu.append_section(None, &analyze_section);
    }

    let clipboard_section = gio::Menu::new();
    clipboard_section.append(Some("Copy"), Some("win.copy-selection"));
    clipboard_section.append(Some("Cut"), Some("win.cut-selection"));
    menu.append_section(None, &clipboard_section);

    if is_split_active {
        let panel_section = gio::Menu::new();
        panel_section.append(
            Some("Copy to Other Panel"),
            Some("win.copy-to-other-panel-selected"),
        );
        panel_section.append(
            Some("Move to Other Panel"),
            Some("win.move-to-other-panel-selected"),
        );
        menu.append_section(None, &panel_section);
    }

    let mutate_section = gio::Menu::new();
    mutate_section.append(Some("Rename"), Some("win.rename-selected"));
    mutate_section.append(Some("Move to Trash"), Some("win.trash-selection"));
    mutate_section.append(Some("Delete Permanently"), Some("win.delete-selection"));
    menu.append_section(None, &mutate_section);

    let archive_section = gio::Menu::new();
    archive_section.append(Some("Compress…"), Some("win.compress-selected"));
    if is_archive {
        archive_section.append(Some("Extract Here"), Some("win.extract-here-selected"));
        archive_section.append(Some("Extract to…"), Some("win.extract-to-selected"));
    }
    menu.append_section(None, &archive_section);

    let path_section = gio::Menu::new();
    path_section.append(
        Some("Open Terminal Here (Faz 23)"),
        Some("win.not-implemented"),
    );
    path_section.append(Some("Copy Path"), Some("win.copy-path-selected"));
    path_section.append(Some("Copy Location"), Some("win.copy-location-selected"));
    menu.append_section(None, &path_section);

    let properties_section = gio::Menu::new();
    properties_section.append(Some("Properties"), Some("win.properties-selected"));
    menu.append_section(None, &properties_section);

    menu
}

/// Builds the empty-space (background) menu for the current directory.
/// `has_clipboard` decides whether Paste is bound to the real `win.paste`
/// action or the shared disabled `win.not-implemented` action.
fn build_background_menu(has_clipboard: bool) -> gio::Menu {
    let menu = gio::Menu::new();

    let create_section = gio::Menu::new();
    create_section.append(Some("New Folder"), Some("win.create-folder"));
    create_section.append(Some("New Document"), Some("win.create-document"));
    menu.append_section(None, &create_section);

    let paste_section = gio::Menu::new();
    let paste_action = if has_clipboard {
        "win.paste"
    } else {
        "win.not-implemented"
    };
    paste_section.append(Some("Paste"), Some(paste_action));
    menu.append_section(None, &paste_section);

    let misc_section = gio::Menu::new();
    misc_section.append(
        Some("Open Terminal Here (Faz 23)"),
        Some("win.not-implemented"),
    );
    misc_section.append(
        Some("Analyze Disk Usage…"),
        Some("win.analyze-disk-current"),
    );
    menu.append_section(None, &misc_section);

    let properties_section = gio::Menu::new();
    properties_section.append(Some("Properties"), Some("win.properties-current"));
    menu.append_section(None, &properties_section);

    menu
}

/// Builds the "Open With" submenu for `item`: up to
/// `MAX_RECOMMENDED_IN_MENU` applications GIO recommends for the item's
/// content type, each bound to `win.open-with-app` with its desktop id as
/// the action's string target (see `window.rs`'s `action_open_with_app`),
/// followed by "Other Application…" which opens the full Faz 22 dialog via
/// `win.open-with-selected`.
fn build_open_with_submenu(item: &FileItem) -> gio::Menu {
    let submenu = gio::Menu::new();

    let recommended = open_with::recommended_apps(&item.metadata.mime_type);
    if !recommended.is_empty() {
        let apps_section = gio::Menu::new();
        for app in recommended.iter().take(MAX_RECOMMENDED_IN_MENU) {
            let Some(id) = app.id() else { continue };
            let menu_item = gio::MenuItem::new(Some(&app.name()), None);
            menu_item.set_action_and_target_value(
                Some("win.open-with-app"),
                Some(&id.as_str().to_variant()),
            );
            apps_section.append_item(&menu_item);
        }
        submenu.append_section(None, &apps_section);
    }

    let other_section = gio::Menu::new();
    other_section.append(Some("Other Application…"), Some("win.open-with-selected"));
    submenu.append_section(None, &other_section);

    submenu
}

/// Recognized archive extensions for the "Extract Here"/"Extract to…"
/// entries' visibility. Delegates to `ArchiveFormat::from_name` so the menu
/// never offers extraction for a name the Faz 19 engine can't actually
/// dispatch (and never omits one it can).
fn is_archive_name(name: &str) -> bool {
    ArchiveFormat::from_name(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_common_archive_extensions() {
        assert!(is_archive_name("backup.zip"));
        assert!(is_archive_name("Backup.TAR.GZ"));
        assert!(is_archive_name("data.7z"));
        assert!(is_archive_name("data.tar.xz"));
        assert!(is_archive_name("data.tar.zst"));
    }

    #[test]
    fn rejects_non_archive_names() {
        assert!(!is_archive_name("notes.txt"));
        assert!(!is_archive_name("archive-of-photos"));
        assert!(!is_archive_name("gzip-notes.md"));
    }

    #[test]
    fn rejects_unsupported_archive_like_extensions() {
        // Recognized by common file managers but not by the Faz 19 engine
        // (`ArchiveFormat` has no variant for these) — the menu must not
        // offer "Extract Here" for a format that can't actually extract.
        assert!(!is_archive_name("data.tar.bz2"));
        assert!(!is_archive_name("data.tbz2"));
        assert!(!is_archive_name("data.rar"));
        assert!(!is_archive_name("data.xz"));
    }
}
