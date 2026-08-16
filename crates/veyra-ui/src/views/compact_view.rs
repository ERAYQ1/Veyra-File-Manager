use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;

use crate::dnd::DndWiring;
use crate::thumbnails::ThumbnailService;
use crate::views::{build_grid_view, build_selection, item_at};

const ICON_SIZE: i32 = 20;

/// Small icons with the name beside them, flowing in dense columns for
/// quick visual scanning of large directories, ordered by the tab's shared
/// `SortConfig` (`sorter`, see `crate::sorting`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_compact_view(
    model: &gio::ListStore,
    filter: &gtk4::CustomFilter,
    sorter: &gtk4::CustomSorter,
    on_open: impl Fn(veyra_filesystem::FileItem) + 'static,
    has_clipboard: Rc<dyn Fn() -> bool>,
    split_active: Rc<dyn Fn() -> bool>,
    is_trash: Rc<dyn Fn() -> bool>,
    thumbnails: Rc<ThumbnailService>,
    dnd_wiring: DndWiring,
) -> (gtk4::Widget, gtk4::MultiSelection) {
    let selection = build_selection(model, filter, Some(sorter.clone()));
    let selection_for_activate = selection.clone();

    let grid_view = build_grid_view(
        &selection,
        ICON_SIZE,
        true,
        thumbnails,
        dnd_wiring,
        move |position| {
            if let Some(item) = item_at(&selection_for_activate, position) {
                on_open(item);
            }
        },
    );
    // Narrow horizontal item boxes (icon + name) pack into many columns,
    // giving the dense "flowing columns" layout Compact View is meant for.
    grid_view.set_min_columns(3);
    crate::context_menu::attach(
        &grid_view,
        &selection,
        has_clipboard,
        split_active,
        is_trash,
    );

    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&grid_view)
        .build();

    (scrolled.upcast(), selection)
}
