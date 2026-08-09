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

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use libadwaita as adw;

use veyra_filesystem::FileItem;

use crate::state::SharedState;
use crate::views::ViewMode;

/// The three views' independent `GtkSingleSelection` chains, so keyboard
/// operations (Copy/Cut/Trash/Delete) can find "the selected item" in
/// whichever view is currently visible for a tab.
#[derive(Clone)]
pub(crate) struct ViewSelections {
    pub icon: gtk4::SingleSelection,
    pub compact: gtk4::SingleSelection,
    pub details: gtk4::SingleSelection,
}

impl ViewSelections {
    pub fn selected(&self, view_stack: &gtk4::Stack) -> Option<FileItem> {
        let selection = match view_stack.visible_child_name().as_deref() {
            Some(name) if name == ViewMode::Compact.stack_name() => &self.compact,
            Some(name) if name == ViewMode::Details.stack_name() => &self.details,
            _ => &self.icon,
        };
        crate::views::selected_item(selection)
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
    pub adw_page: adw::TabPage,
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
