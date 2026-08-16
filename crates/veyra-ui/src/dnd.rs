//! Faz 26: shared Drag & Drop plumbing used by every drag source (view
//! items) and drop target (view background, folder rows, breadcrumbs,
//! sidebar bookmarks, panels) in the app. Uses the standard FreeDesktop/
//! Wayland `gdk::FileList` (`text/uri-list`) content type throughout, so
//! drags interoperate with external applications (file manager, GIMP,
//! browsers) in both directions, not just within Veyra.
//!
//! Action resolution deliberately does not rely on GDK's own action
//! negotiation (Ctrl/Shift auto-mapping is compositor-dependent and hard to
//! test): every drop target here declares a fixed `COPY` action to GDK (the
//! same choice `sidebar.rs`'s existing bookmark drop zone already made) and
//! instead reads the held modifier keys itself via
//! `EventControllerExt::current_event_state`, matching the Faz 26 spec
//! (Ctrl -> Copy, Shift -> Move, Alt or a right-button drag -> Ask) with a
//! deterministic, unit-testable resolution function.

use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};
use libadwaita as adw;
use libadwaita::prelude::*;

use veyra_filesystem::VeyraPath;

/// The four choices offered by the Alt-drag / right-button-drag "Ask"
/// popover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DndChoice {
    Copy,
    Move,
    Link,
    Cancel,
}

/// What a drop should do with the dragged files, decided once GDK's own
/// value/action negotiation is out of the picture (see module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DndAction {
    Copy,
    Move,
    Link,
}

/// Resolves the plain (non-Ask) action from held modifier keys: Ctrl always
/// wins (explicit Copy), then Shift (explicit Move), then `prefer_move` —
/// the caller's same-filesystem heuristic — decides the unmodified default.
pub(crate) fn resolve_action(state: gdk::ModifierType, prefer_move: bool) -> DndAction {
    if state.contains(gdk::ModifierType::CONTROL_MASK) {
        DndAction::Copy
    } else if state.contains(gdk::ModifierType::SHIFT_MASK) || prefer_move {
        DndAction::Move
    } else {
        DndAction::Copy
    }
}

/// Whether `state` (or a right-button-initiated drag, checked separately by
/// the caller) should show the Ask popover instead of acting immediately.
pub(crate) fn wants_ask(state: gdk::ModifierType) -> bool {
    state.contains(gdk::ModifierType::ALT_MASK)
}

/// Same-volume heuristic behind the unmodified-drag default (Move on the
/// same filesystem, Copy across filesystems/mounts) — mirrors the Files/
/// Nautilus convention. Falls back to `false` (Copy) if either side's
/// filesystem id can't be queried (e.g. a location that vanished between
/// drag start and drop), which is the safer default (Copy never deletes the
/// source).
pub(crate) fn same_filesystem(a: &VeyraPath, b: &VeyraPath) -> bool {
    if a == b {
        return true;
    }
    let id_of = |p: &VeyraPath| -> Option<glib::GString> {
        p.to_gio_file()
            .query_filesystem_info(gio::FILE_ATTRIBUTE_ID_FILESYSTEM, gio::Cancellable::NONE)
            .ok()?
            .attribute_string(gio::FILE_ATTRIBUTE_ID_FILESYSTEM)
    };
    match (id_of(a), id_of(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

/// Executes a resolved DND drop: routes Copy/Move through the caller's
/// bulk-operation runner (Faz 5 `OperationQueue`, with its progress/conflict
/// UI), and Create-Link through a small synchronous symlink loop off the
/// main thread (Rule #11/#12) since symlinks are a single, un-conflicted
/// GIO call each and don't need the full queue/conflict machinery.
pub(crate) type DropExecutor = Rc<dyn Fn(Vec<VeyraPath>, VeyraPath, DndAction)>;

/// The two pieces of DND context every view needs, injected once per tab
/// from `window.rs`: the tab's own current directory (the destination for a
/// background drop) and the shared executor that turns a resolved
/// Copy/Move/Link into an actual `OperationQueue`/symlink run.
#[derive(Clone)]
pub(crate) struct DndWiring {
    pub current_dir: Rc<dyn Fn() -> VeyraPath>,
    pub execute: DropExecutor,
}

/// Attaches a `GtkDragSource` to `widget`, dragging one or more files at
/// once. `get_source` is polled both when the drag starts (`prepare`, to
/// build the `gdk::FileList` content) and again in `drag_begin` (to look up
/// the drag icon) — called twice rather than cached because callers backed
/// by a recycled `GtkListItem` factory row must re-resolve "the item(s) this
/// row's drag currently represents" live, not at controller-setup time.
pub(crate) fn attach_drag_source(
    widget: &impl IsA<gtk4::Widget>,
    button: u32,
    get_source: impl Fn() -> Option<(Vec<VeyraPath>, &'static str)> + 'static,
) {
    let drag_source = gtk4::DragSource::new();
    drag_source.set_button(button);
    if button == gdk::BUTTON_SECONDARY {
        // Faz 26.C: a right-button drag always resolves as Ask, regardless
        // of modifiers held during the drag itself.
        drag_source.set_actions(gdk::DragAction::ASK);
    } else {
        drag_source
            .set_actions(gdk::DragAction::COPY | gdk::DragAction::MOVE | gdk::DragAction::LINK);
    }

    let get_source = Rc::new(get_source);

    {
        let get_source = get_source.clone();
        drag_source.connect_prepare(move |_, _, _| {
            let (paths, _) = get_source()?;
            if paths.is_empty() {
                return None;
            }
            let files: Vec<gio::File> = paths.iter().map(VeyraPath::to_gio_file).collect();
            let file_list = gdk::FileList::from_array(&files);
            Some(gdk::ContentProvider::for_value(&file_list.to_value()))
        });
    }

    {
        let get_source = get_source.clone();
        drag_source.connect_drag_begin(move |source, _drag| {
            let Some((_, icon_name)) = get_source() else {
                return;
            };
            let Some(display) = source.widget().map(|w| w.display()) else {
                return;
            };
            let theme = gtk4::IconTheme::for_display(&display);
            let paintable = theme.lookup_icon(
                icon_name,
                &[],
                32,
                1,
                gtk4::TextDirection::None,
                gtk4::IconLookupFlags::empty(),
            );
            source.set_icon(Some(&paintable), 0, 0);
        });
    }

    widget.add_controller(drag_source);
}

/// Attaches a `GtkDropTarget` to `widget` accepting a `gdk::FileList`.
/// `accept` gates whether this target wants the drop at all (e.g. a folder
/// row only accepts when the row under the pointer is a directory);
/// `destination` resolves the drop's target directory, evaluated live at
/// drop time for the same recycled-row reason `attach_drag_source` polls
/// `get_source` live. When both succeed, `execute` runs the resolved
/// Copy/Move/Link (or opens the Ask popover first).
pub(crate) fn attach_drop_target(
    widget: &impl IsA<gtk4::Widget>,
    accept: impl Fn() -> bool + 'static,
    destination: impl Fn() -> Option<VeyraPath> + 'static,
    execute: DropExecutor,
) -> gtk4::DropTarget {
    let drop_target = gtk4::DropTarget::new(
        gdk::FileList::static_type(),
        gdk::DragAction::COPY
            | gdk::DragAction::MOVE
            | gdk::DragAction::LINK
            | gdk::DragAction::ASK,
    );

    {
        let accept = Rc::new(accept);
        drop_target.connect_accept(move |_, _| accept());
    }

    let widget_for_popover: gtk4::Widget = widget.clone().upcast();
    drop_target.connect_drop(move |target, value, x, y| {
        let Ok(file_list) = value.get::<gdk::FileList>() else {
            return false;
        };
        let sources: Vec<VeyraPath> = file_list
            .files()
            .into_iter()
            .map(|f| VeyraPath::from_gio_file(&f))
            .collect();
        if sources.is_empty() {
            return false;
        }
        let Some(dest) = destination() else {
            return false;
        };
        resolve_and_execute(
            target,
            &widget_for_popover,
            x,
            y,
            sources,
            dest,
            execute.clone(),
        );
        true
    });

    widget.add_controller(drop_target.clone());
    drop_target
}

/// The decision core behind every drop handler: forced-Ask (a right-button
/// drag, or a target that only offered `ASK`) or Alt-held opens the Copy/
/// Move/Link/Cancel popover; otherwise Ctrl/Shift/same-filesystem resolves
/// the action immediately and runs it. Factored out of `attach_drop_target`
/// so a caller with its own reasons to resolve the `gdk::FileList` itself —
/// `sidebar.rs`'s bookmark rows peek at the dropped files' kind first, to
/// tell "bookmark this folder" apart from "file dropped onto a bookmark" —
/// can still reuse the same modifier/Ask/execute behavior afterwards.
pub(crate) fn resolve_and_execute(
    target: &gtk4::DropTarget,
    popover_parent: &gtk4::Widget,
    x: f64,
    y: f64,
    sources: Vec<VeyraPath>,
    destination: VeyraPath,
    execute: DropExecutor,
) {
    let forced_ask = target
        .current_drop()
        .map(|d| d.actions() == gdk::DragAction::ASK)
        .unwrap_or(false);
    let state = target.current_event_state();

    if forced_ask || wants_ask(state) {
        show_ask_popover(popover_parent, x, y, move |choice| {
            let action = match choice {
                DndChoice::Copy => DndAction::Copy,
                DndChoice::Move => DndAction::Move,
                DndChoice::Link => DndAction::Link,
                DndChoice::Cancel => return,
            };
            execute(sources.clone(), destination.clone(), action);
        });
        return;
    }

    let prefer_move = sources
        .first()
        .map(|s| same_filesystem(s, &destination))
        .unwrap_or(false);
    let action = resolve_action(state, prefer_move);
    execute(sources, destination, action);
}

/// The small Copy Here / Move Here / Create Link Here / Cancel popover
/// shown for an Alt-modified or right-button drag (Faz 26.C).
fn show_ask_popover(
    parent: &gtk4::Widget,
    x: f64,
    y: f64,
    on_choice: impl Fn(DndChoice) + 'static,
) {
    let popover = gtk4::Popover::new();
    popover.set_parent(parent);
    popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    popover.set_has_arrow(true);

    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    let on_choice = Rc::new(on_choice);
    let entries: [(&str, &str, DndChoice); 4] = [
        ("edit-copy-symbolic", "Copy Here", DndChoice::Copy),
        ("folder-move-symbolic", "Move Here", DndChoice::Move),
        ("insert-link-symbolic", "Create Link Here", DndChoice::Link),
        ("process-stop-symbolic", "Cancel", DndChoice::Cancel),
    ];
    for (icon_name, label, choice) in entries {
        let button = gtk4::Button::builder().css_classes(["flat"]).build();
        let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        content.append(&gtk4::Image::from_icon_name(icon_name));
        let text = gtk4::Label::new(Some(label));
        text.set_xalign(0.0);
        content.append(&text);
        button.set_child(Some(&content));

        let popover_for_click = popover.clone();
        let on_choice = on_choice.clone();
        button.connect_clicked(move |_| {
            on_choice(choice);
            popover_for_click.popdown();
        });
        list.append(&button);
    }

    popover.set_child(Some(&list));
    popover.connect_closed(move |popover| {
        popover.unparent();
    });
    popover.popup();
}

/// Reports a Create-Link (or any DND execution) failure the same way
/// `sidebar.rs`'s device error dialog does (Rule #15/#18): a plain
/// `AdwAlertDialog`, never a silent no-op or a panic.
pub(crate) fn show_dnd_error(parent: &impl IsA<gtk4::Widget>, heading: &str, body: &str) {
    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .build();
    dialog.add_responses(&[("ok", "OK")]);
    dialog.set_default_response(Some("ok"));
    dialog.set_close_response("ok");
    dialog.present(Some(parent));
}

/// Creates a symlink inside `destination` for each of `sources`, off the
/// GTK main thread (Rule #11/#12). Bypasses the `OperationQueue` (no
/// progress/conflict round trip) — symlink creation is a single fast GIO
/// call per source with no partial-completion story to report progress on;
/// a name collision or permissions failure is reported as one summary
/// error rather than a per-file conflict prompt.
pub(crate) fn create_links(
    window: &adw::ApplicationWindow,
    sources: Vec<VeyraPath>,
    destination: VeyraPath,
    on_done: impl Fn() + 'static,
) {
    let window = window.clone();
    crate::fs_async::run_blocking(
        move || {
            let dest_dir = destination.to_gio_file();
            let mut errors: Vec<String> = Vec::new();
            for source in &sources {
                let Some(name) = source.file_name() else {
                    continue;
                };
                let link_file = dest_dir.child(&name);
                let target = source.to_string();
                if let Err(err) = link_file.make_symbolic_link(&target, gio::Cancellable::NONE) {
                    errors.push(format!("{name}: {err}"));
                }
            }
            errors
        },
        move |errors| {
            if !errors.is_empty() {
                show_dnd_error(&window, "Unable to Create Link", &errors.join("\n"));
            }
            on_done();
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(ctrl: bool, shift: bool) -> gdk::ModifierType {
        let mut state = gdk::ModifierType::empty();
        if ctrl {
            state |= gdk::ModifierType::CONTROL_MASK;
        }
        if shift {
            state |= gdk::ModifierType::SHIFT_MASK;
        }
        state
    }

    #[test]
    fn ctrl_always_copies() {
        assert_eq!(resolve_action(state(true, false), true), DndAction::Copy);
        assert_eq!(resolve_action(state(true, true), true), DndAction::Copy);
    }

    #[test]
    fn shift_always_moves() {
        assert_eq!(resolve_action(state(false, true), false), DndAction::Move);
    }

    #[test]
    fn unmodified_follows_prefer_move() {
        assert_eq!(resolve_action(state(false, false), true), DndAction::Move);
        assert_eq!(resolve_action(state(false, false), false), DndAction::Copy);
    }

    #[test]
    fn alt_wants_ask() {
        assert!(wants_ask(gdk::ModifierType::ALT_MASK));
        assert!(!wants_ask(gdk::ModifierType::CONTROL_MASK));
    }

    #[test]
    fn same_path_is_same_filesystem() {
        let path = VeyraPath::from_local("/tmp");
        assert!(same_filesystem(&path, &path));
    }
}
