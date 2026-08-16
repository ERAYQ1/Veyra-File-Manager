//! Faz 7: per-tab isolated state and the tab registry that backs the window's
//! `AdwTabView`.
//!
//! Each open tab owns its own `AppState` (location, history stack, item
//! model), its own Icon/Compact/Details view stack, its own per-view
//! selections, and its own search query/filter — matching Dolphin-level tab
//! isolation. The window-level chrome (header bar buttons, breadcrumbs,
//! status bar, clipboard) stays shared and simply reflects whichever tab is
//! currently selected in the `AdwTabView`.
//!
//! `TabPage` values are looked up from `TabRegistry` by the `AdwTabPage` GTK
//! hands back from `AdwTabView::selected_page()`; `glib`'s object wrapper
//! types implement `Hash`/`Eq` by pointer identity, so a plain `HashMap`
//! keyed on `adw::TabPage` is enough — no unsafe qdata needed (this crate
//! forbids `unsafe`).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

use veyra_filesystem::FileItem;

use crate::sorting::{QuickFilter, SortConfig, SortKey};
use crate::state::SharedState;
use crate::views::ViewMode;

/// The three views' independent `GtkMultiSelection` chains, so keyboard/menu
/// operations (Copy/Cut/Trash/Delete/Compress/…) can find "the selected
/// item(s)" in whichever view is currently visible for a tab.
#[derive(Clone)]
pub(crate) struct ViewSelections {
    pub icon: gtk4::MultiSelection,
    pub compact: gtk4::MultiSelection,
    pub details: gtk4::MultiSelection,
}

impl ViewSelections {
    /// The `GtkMultiSelection` backing whichever of the three views is
    /// currently visible in `view_stack`.
    pub fn active(&self, view_stack: &gtk4::Stack) -> &gtk4::MultiSelection {
        match view_stack.visible_child_name().as_deref() {
            Some(name) if name == ViewMode::Compact.stack_name() => &self.compact,
            Some(name) if name == ViewMode::Details.stack_name() => &self.details,
            _ => &self.icon,
        }
    }

    /// The first selected item (ascending position order), for actions that
    /// only ever target a single item (Open, Rename, Properties, …).
    pub fn selected(&self, view_stack: &gtk4::Stack) -> Option<FileItem> {
        crate::views::selected_item(self.active(view_stack))
    }

    /// Every selected item, for bulk actions (Copy/Cut/Trash/Delete/
    /// Compress/panel transfer).
    pub fn selected_items(&self, view_stack: &gtk4::Stack) -> Vec<FileItem> {
        crate::views::selected_items(self.active(view_stack))
    }
}

/// Everything a single tab owns in isolation. Cheap to clone: every field is
/// either a `Rc` or a GTK widget handle (internally refcounted).
#[derive(Clone)]
pub(crate) struct TabPage {
    pub state: SharedState,
    pub view_stack: gtk4::Stack,
    pub selections: ViewSelections,
    pub filter: gtk4::CustomFilter,
    pub search_query: Rc<RefCell<String>>,
    /// Faz 13: this tab's sort criterion/direction/folders-first, shared by
    /// all three views' sorters (`crate::sorting::build_sorter`) and by the
    /// Details view's per-column header click handling.
    pub sort_config: Rc<RefCell<SortConfig>>,
    /// Faz 13: the active quick file-type/attribute filter, ANDed with
    /// `search_query` in `filter`.
    pub quick_filter: Rc<RefCell<QuickFilter>>,
    /// Faz 14: whether hidden files (dotfiles and entries listed in a
    /// directory's `.hidden` file — both already folded into
    /// `FileMetadata::is_hidden` by GIO) are shown in this tab. Toggled by
    /// Ctrl+H (`win.toggle-hidden-files`), isolated per tab like every
    /// other view/filter/sort preference (Kural #51).
    pub show_hidden: Rc<RefCell<bool>>,
    /// The `GtkCustomSorter` all three views' `GtkSortListModel`s share;
    /// `resort()` is the only thing that needs to touch it directly.
    pub sorter: gtk4::CustomSorter,
    /// The Details view's `GtkColumnView`, needed to mirror `sort_config`
    /// into its header sort-indicator arrows (see `resort`).
    pub details_column_view: gtk4::ColumnView,
    /// Maps each `SortKey` that has a Details column to that column, so
    /// `resort` can find which header to mark as the active sort column.
    pub details_sort_columns: Rc<Vec<(SortKey, gtk4::ColumnViewColumn)>>,
    /// Reentrancy guard shared with the Details view's header-click
    /// listener: set while `resort` programmatically calls
    /// `sort_by_column`, so that call's own `changed` signal doesn't loop
    /// back into updating `sort_config` again.
    pub sort_sync_guard: Rc<Cell<bool>>,
    pub adw_page: adw::TabPage,
    /// Faz 26: the shared Copy/Move/Link executor this tab's views were
    /// built with, reused by `window::update_chrome` to wire the
    /// breadcrumbs' per-segment drop targets on every navigation.
    pub dnd_execute: crate::dnd::DropExecutor,
}

impl TabPage {
    /// Re-sorts every view after `sort_config` changes (menu selection or a
    /// Details column header click) and mirrors the new criterion/direction
    /// onto the Details view's header indicator.
    pub(crate) fn resort(&self) {
        self.sorter.changed(gtk4::SorterChange::Different);

        let config = *self.sort_config.borrow();
        let column = self
            .details_sort_columns
            .iter()
            .find(|(key, _)| *key == config.key)
            .map(|(_, col)| col.clone());

        self.sort_sync_guard.set(true);
        self.details_column_view
            .sort_by_column(column.as_ref(), config.order.to_gtk());
        self.sort_sync_guard.set(false);
    }

    /// Re-evaluates the combined search/quick-filter after either changes.
    pub(crate) fn refresh_filter(&self) {
        self.filter.changed(gtk4::FilterChange::Different);
    }
}

/// Maps each `AdwTabPage` GTK owns to the `TabPage` state Veyra owns for it.
pub(crate) type TabRegistry = Rc<RefCell<HashMap<adw::TabPage, TabPage>>>;

/// The `TabPage` behind the `AdwTabView`'s currently selected page, if any.
/// A freshly built window always has one tab open, so `None` only happens
/// transiently during tab teardown.
pub(crate) fn active_tab(tab_view: &adw::TabView, registry: &TabRegistry) -> Option<TabPage> {
    let page = tab_view.selected_page()?;
    registry.borrow().get(&page).cloned()
}
