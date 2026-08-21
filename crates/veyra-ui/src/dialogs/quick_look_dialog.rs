//! Faz 60: Quick Look — a `Space`-triggered, non-modal live preview window
//! for the currently selected file. Reuses `preview.rs`'s image-decode,
//! capped-text-read and MIME-sniffing helpers so that decoding logic stays
//! in one place; adds what the sidebar preview doesn't have: audio/video
//! playback (`gtk4::Video` + `gtk4::MediaFile` — shipped by GTK itself, no
//! new dependency) and an archive-contents listing
//! (`veyra_filesystem::list_preview`).
//!
//! `Escape` or a second `Space` closes the window; `Up`/`Down` walk the same
//! `GtkMultiSelection` the file view is showing, re-rendering the dialog's
//! content in place and syncing the underlying view's selection to match —
//! the dialog itself is never torn down and rebuilt for a nav step. A
//! generation counter (same pattern as `preview.rs`) discards stale async
//! results from a background read that a fast `Up`/`Down` flurry has
//! already superseded (Rule #11-#14).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use veyra_filesystem::{format_size, ArchivePreviewEntry, FileItem, FileKind};

use crate::fs_async;
use crate::i18n::{t, t_fmt};
use crate::preview::{
    decode_image, friendly_gio_error, is_archive_mime, is_image_mime, is_text_mime, modified_label,
    read_capped,
};
use crate::views;

/// Fixed cap for the text card — Quick Look has no settings dependency,
/// unlike the sidebar preview's configurable `max_preview_size_kb`.
const TEXT_PREVIEW_CAP_BYTES: usize = 256 * 1024;
/// How many archive entries the archive card lists before stopping.
const ARCHIVE_PREVIEW_LIMIT: usize = 200;

struct QuickLookHandles {
    dialog: adw::Dialog,
    title: adw::WindowTitle,
    stack: gtk4::Stack,
    image_picture: gtk4::Picture,
    image_meta: gtk4::Label,
    text_view: gtk4::TextView,
    text_meta: gtk4::Label,
    media_video: gtk4::Video,
    media_meta: gtk4::Label,
    archive_list: gtk4::ListBox,
    archive_meta: gtk4::Label,
    info_icon: gtk4::Image,
    info_title: gtk4::Label,
    info_subtitle: gtk4::Label,
    info_meta: gtk4::Label,
}

/// Per-session navigation state, shared by the key controller and every
/// render step.
struct QuickLookState {
    selection: gtk4::MultiSelection,
    position: Cell<u32>,
    generation: Cell<u64>,
    current_item: RefCell<Option<FileItem>>,
}

/// Shows Quick Look for the item at `position` in `selection`'s model,
/// parented to `window`. No-op if `position` doesn't resolve to an item
/// (e.g. the selection emptied between the keypress and this call).
pub(crate) fn show(
    window: &adw::ApplicationWindow,
    selection: gtk4::MultiSelection,
    position: u32,
) {
    let Some(item) = views::item_at(&selection, position) else {
        return;
    };

    let dialog = adw::Dialog::builder()
        .content_width(760)
        .content_height(580)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let title = adw::WindowTitle::new("", "");
    header.set_title_widget(Some(&title));
    let open_button = gtk4::Button::from_icon_name("document-open-symbolic");
    open_button.set_tooltip_text(Some(t("quick_look.open_with")));
    open_button.update_property(&[gtk4::accessible::Property::Label(t("quick_look.open_with"))]);
    header.pack_end(&open_button);
    toolbar_view.add_top_bar(&header);

    let built = build_stack();
    toolbar_view.set_content(Some(&built.stack));
    dialog.set_child(Some(&toolbar_view));

    let handles = Rc::new(QuickLookHandles {
        dialog: dialog.clone(),
        title,
        stack: built.stack,
        image_picture: built.image_picture,
        image_meta: built.image_meta,
        text_view: built.text_view,
        text_meta: built.text_meta,
        media_video: built.media_video,
        media_meta: built.media_meta,
        archive_list: built.archive_list,
        archive_meta: built.archive_meta,
        info_icon: built.info_icon,
        info_title: built.info_title,
        info_subtitle: built.info_subtitle,
        info_meta: built.info_meta,
    });

    let state = Rc::new(QuickLookState {
        selection,
        position: Cell::new(position),
        generation: Cell::new(0),
        current_item: RefCell::new(None),
    });

    {
        let state = state.clone();
        open_button.connect_clicked(move |_| {
            let Some(path) = state.current_item.borrow().as_ref().map(|i| i.path.clone()) else {
                return;
            };
            std::thread::spawn(move || {
                if let Err(err) = veyra_filesystem::open(&path) {
                    tracing::warn!(path = %veyra_core::security::log_path(&path), error = %err, "failed to open quick look item externally");
                }
            });
        });
    }

    let key_controller = gtk4::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    {
        let handles = handles.clone();
        let state = state.clone();
        key_controller.connect_key_pressed(move |_, keyval, _, _| match keyval {
            gtk4::gdk::Key::Escape | gtk4::gdk::Key::space => {
                handles.dialog.close();
                glib::Propagation::Stop
            }
            gtk4::gdk::Key::Up => {
                navigate(&handles, &state, -1);
                glib::Propagation::Stop
            }
            gtk4::gdk::Key::Down => {
                navigate(&handles, &state, 1);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
    }
    dialog.add_controller(key_controller);

    render(&handles, &state, item);
    dialog.present(Some(window));
}

/// Moves the previewed item by `delta` (`-1`/`+1`) within `state.selection`'s
/// model, clamped to its bounds, re-rendering in place and syncing the
/// underlying view's selection so the file view stays in step with what
/// Quick Look is showing.
fn navigate(handles: &Rc<QuickLookHandles>, state: &Rc<QuickLookState>, delta: i64) {
    let n_items = state.selection.n_items();
    if n_items == 0 {
        return;
    }
    let current = state.position.get() as i64;
    let next = (current + delta).clamp(0, n_items as i64 - 1) as u32;
    if next == state.position.get() {
        return;
    }
    let Some(item) = views::item_at(&state.selection, next) else {
        return;
    };
    state.position.set(next);
    state.selection.select_item(next, true);
    render(handles, state, item);
}

fn render(handles: &Rc<QuickLookHandles>, state: &Rc<QuickLookState>, item: FileItem) {
    let generation = state.generation.get() + 1;
    state.generation.set(generation);
    *state.current_item.borrow_mut() = Some(item.clone());

    handles.title.set_title(item.name());
    handles.title.set_subtitle(&format!(
        "{} · {}",
        format_size(item.metadata.size_bytes),
        item.metadata.mime_type
    ));

    match item.kind().clone() {
        FileKind::Directory => show_directory(handles, &item),
        FileKind::Symlink {
            is_broken: true, ..
        } => show_error(handles, &item, t("preview.error.broken_symlink")),
        FileKind::Symlink { target, .. } => show_symlink(handles, &item, target),
        FileKind::Regular => show_regular(handles, state, generation, item),
        other => show_info_card(
            handles,
            &item,
            crate::views::icon_name_for(&item),
            kind_label(&other),
        ),
    }
}

fn show_regular(
    handles: &Rc<QuickLookHandles>,
    state: &Rc<QuickLookState>,
    generation: u64,
    item: FileItem,
) {
    let mime = item.metadata.mime_type.clone();
    if is_image_mime(&mime) {
        show_image(handles, state, generation, item);
    } else if is_text_mime(&mime) {
        show_text(handles, state, generation, item);
    } else if mime.starts_with("audio/") || mime.starts_with("video/") {
        show_media(handles, &item);
    } else if is_archive_mime(&mime) {
        show_archive(handles, state, generation, item);
    } else {
        show_info_card(
            handles,
            &item,
            crate::views::icon_name_for(&item),
            t("preview.kind.file"),
        );
    }
}

fn show_image(
    handles: &Rc<QuickLookHandles>,
    state: &Rc<QuickLookState>,
    generation: u64,
    item: FileItem,
) {
    handles.stack.set_visible_child_name("loading");
    let path = item.path.clone();
    let handles_done = handles.clone();
    let state_done = state.clone();
    fs_async::run_blocking(
        move || decode_image(&path),
        move |result| {
            if !is_current(&state_done, generation) {
                return;
            }
            match result {
                Ok(decoded) => {
                    let pixbuf = gtk4::gdk_pixbuf::Pixbuf::from_bytes(
                        &decoded.pixels,
                        decoded.colorspace,
                        decoded.has_alpha,
                        decoded.bits_per_sample,
                        decoded.width,
                        decoded.height,
                        decoded.rowstride,
                    );
                    let texture = gtk4::gdk::Texture::for_pixbuf(&pixbuf);
                    handles_done.image_picture.set_paintable(Some(&texture));
                    handles_done.image_meta.set_label(&format!(
                        "{} × {} px · {}",
                        decoded.width,
                        decoded.height,
                        modified_label(&item)
                    ));
                    handles_done.stack.set_visible_child_name("image");
                }
                Err(err) => {
                    tracing::warn!(path = %veyra_core::security::log_path(&item.path), error = %err, "failed to read image for quick look");
                    show_error(&handles_done, &item, &friendly_gio_error(&err));
                }
            }
        },
    );
}

fn show_text(
    handles: &Rc<QuickLookHandles>,
    state: &Rc<QuickLookState>,
    generation: u64,
    item: FileItem,
) {
    handles.stack.set_visible_child_name("loading");
    let path = item.path.clone();
    let handles_done = handles.clone();
    let state_done = state.clone();
    fs_async::run_blocking(
        move || read_capped(&path, TEXT_PREVIEW_CAP_BYTES),
        move |result| {
            if !is_current(&state_done, generation) {
                return;
            }
            match result {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    let truncated = item.metadata.size_bytes as usize > bytes.len();
                    handles_done.text_view.buffer().set_text(&text);
                    let mut meta = format!(
                        "{} lines · {} characters",
                        text.lines().count(),
                        text.chars().count(),
                    );
                    if truncated {
                        meta.push_str(&format!(" · {}", t("quick_look.truncated_warning")));
                    }
                    handles_done.text_meta.set_label(&meta);
                    handles_done.stack.set_visible_child_name("text");
                }
                Err(err) => {
                    tracing::warn!(path = %veyra_core::security::log_path(&item.path), error = %err, "failed to read text for quick look");
                    show_error(&handles_done, &item, &friendly_gio_error(&err));
                }
            }
        },
    );
}

/// GTK's own `GtkMediaFile`/`GtkVideo` (backed by GStreamer at the system
/// level when available) — no `gstreamer-rs` binding needed, so this adds
/// zero new crate dependencies.
fn show_media(handles: &Rc<QuickLookHandles>, item: &FileItem) {
    let media = gtk4::MediaFile::for_file(&item.path.to_gio_file());
    media.set_loop(false);
    handles.media_video.set_media_stream(Some(&media));
    handles.media_meta.set_label(&format!(
        "{} · {}",
        format_size(item.metadata.size_bytes),
        modified_label(item)
    ));
    handles.stack.set_visible_child_name("media");
}

fn show_archive(
    handles: &Rc<QuickLookHandles>,
    state: &Rc<QuickLookState>,
    generation: u64,
    item: FileItem,
) {
    handles.stack.set_visible_child_name("loading");
    let path = item.path.clone();
    let handles_done = handles.clone();
    let state_done = state.clone();
    fs_async::run_blocking(
        move || veyra_filesystem::list_preview(&path, ARCHIVE_PREVIEW_LIMIT),
        move |result| {
            if !is_current(&state_done, generation) {
                return;
            }
            match result {
                Ok(entries) => {
                    populate_archive_list(&handles_done.archive_list, &entries);
                    let mut meta = format!(
                        "{} · {}",
                        format_size(item.metadata.size_bytes),
                        modified_label(&item)
                    );
                    if entries.len() == ARCHIVE_PREVIEW_LIMIT {
                        meta.push_str(&format!(
                            " · {}",
                            t_fmt(
                                "quick_look.archive_more",
                                &[("count", &ARCHIVE_PREVIEW_LIMIT.to_string())]
                            )
                        ));
                    }
                    handles_done.archive_meta.set_label(&meta);
                    handles_done.stack.set_visible_child_name("archive");
                }
                Err(_) => {
                    // Unsupported format (7z) or unreadable — fall back to
                    // the generic info card rather than an error state,
                    // since this isn't a user-facing failure.
                    show_info_card(
                        &handles_done,
                        &item,
                        "package-x-generic-symbolic",
                        t("preview.kind.archive"),
                    );
                }
            }
        },
    );
}

fn populate_archive_list(list: &gtk4::ListBox, entries: &[ArchivePreviewEntry]) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
    for entry in entries {
        let row = adw::ActionRow::builder().title(entry.name.clone()).build();
        if !entry.is_dir {
            row.set_subtitle(&format_size(entry.size));
        }
        let icon = gtk4::Image::from_icon_name(if entry.is_dir {
            "folder-symbolic"
        } else {
            "text-x-generic-symbolic"
        });
        row.add_prefix(&icon);
        list.append(&row);
    }
}

fn show_directory(handles: &Rc<QuickLookHandles>, item: &FileItem) {
    handles
        .info_icon
        .set_icon_name(Some(crate::views::icon_name_for(item)));
    handles.info_title.set_label(item.name());
    handles.info_subtitle.set_label(t("preview.kind.folder"));
    handles
        .info_meta
        .set_label(&format!("{}\n{}", item.path, modified_label(item)));
    handles.stack.set_visible_child_name("info");
}

fn show_symlink(
    handles: &Rc<QuickLookHandles>,
    item: &FileItem,
    target: Option<std::path::PathBuf>,
) {
    handles
        .info_icon
        .set_icon_name(Some(crate::views::icon_name_for(item)));
    handles.info_title.set_label(item.name());
    handles.info_subtitle.set_label(t("preview.kind.symlink"));
    let target_label = target
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| t("preview.symlink.unknown_target").to_string());
    handles
        .info_meta
        .set_label(&format!("→ {target_label}\n{}", modified_label(item)));
    handles.stack.set_visible_child_name("info");
}

fn show_info_card(
    handles: &Rc<QuickLookHandles>,
    item: &FileItem,
    icon_name: &str,
    subtitle: &str,
) {
    handles.info_icon.set_icon_name(Some(icon_name));
    handles.info_title.set_label(item.name());
    handles.info_subtitle.set_label(subtitle);
    let permissions = item
        .metadata
        .permissions
        .map(|p| p.symbolic_string())
        .unwrap_or_else(|| t("quick_look.permissions_unknown").to_string());
    handles.info_meta.set_label(&format!(
        "{}\n{}\n{}\n{}",
        item.path,
        format_size(item.metadata.size_bytes),
        permissions,
        modified_label(item),
    ));
    handles.stack.set_visible_child_name("info");
}

fn show_error(handles: &Rc<QuickLookHandles>, item: &FileItem, reason: &str) {
    handles
        .info_icon
        .set_icon_name(Some("dialog-warning-symbolic"));
    handles.info_title.set_label(t("preview.error.unable"));
    handles.info_subtitle.set_label(reason);
    handles.info_meta.set_label(item.name());
    handles.stack.set_visible_child_name("info");
}

fn is_current(state: &Rc<QuickLookState>, generation: u64) -> bool {
    state.generation.get() == generation
}

fn kind_label(kind: &FileKind) -> &'static str {
    match kind {
        FileKind::Fifo => t("preview.kind.pipe"),
        FileKind::Socket => t("preview.kind.socket"),
        FileKind::BlockDevice => t("preview.kind.block_device"),
        FileKind::CharDevice => t("preview.kind.char_device"),
        _ => t("preview.kind.special"),
    }
}

struct BuiltStack {
    stack: gtk4::Stack,
    image_picture: gtk4::Picture,
    image_meta: gtk4::Label,
    text_view: gtk4::TextView,
    text_meta: gtk4::Label,
    media_video: gtk4::Video,
    media_meta: gtk4::Label,
    archive_list: gtk4::ListBox,
    archive_meta: gtk4::Label,
    info_icon: gtk4::Image,
    info_title: gtk4::Label,
    info_subtitle: gtk4::Label,
    info_meta: gtk4::Label,
}

fn build_stack() -> BuiltStack {
    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    stack.set_vexpand(true);

    stack.add_named(&loading_page(), Some("loading"));

    let image_picture = gtk4::Picture::new();
    image_picture.set_can_shrink(true);
    image_picture.set_vexpand(true);
    image_picture.set_margin_top(12);
    image_picture.set_margin_start(12);
    image_picture.set_margin_end(12);
    let image_meta = meta_label();
    let image_page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    image_page.append(&image_picture);
    image_page.append(&image_meta);
    stack.add_named(&image_page, Some("image"));

    let text_view = gtk4::TextView::new();
    text_view.set_editable(false);
    text_view.set_cursor_visible(false);
    text_view.set_monospace(true);
    text_view.set_wrap_mode(gtk4::WrapMode::WordChar);
    text_view.set_can_focus(false);
    text_view.set_top_margin(8);
    text_view.set_bottom_margin(8);
    text_view.set_left_margin(8);
    text_view.set_right_margin(8);
    let text_scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .child(&text_view)
        .build();
    let text_meta = meta_label();
    let text_page = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    text_page.append(&text_scrolled);
    text_page.append(&text_meta);
    stack.add_named(&text_page, Some("text"));

    let media_video = gtk4::Video::new();
    media_video.set_vexpand(true);
    media_video.set_autoplay(false);
    let media_meta = meta_label();
    let media_page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    media_page.append(&media_video);
    media_page.append(&media_meta);
    stack.add_named(&media_page, Some("media"));

    let archive_list = gtk4::ListBox::new();
    archive_list.set_selection_mode(gtk4::SelectionMode::None);
    archive_list.set_can_focus(false);
    archive_list.add_css_class("boxed-list");
    let archive_scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .child(&archive_list)
        .build();
    let archive_meta = meta_label();
    let archive_page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    archive_page.set_margin_top(12);
    archive_page.set_margin_start(12);
    archive_page.set_margin_end(12);
    archive_page.append(&archive_scrolled);
    archive_page.append(&archive_meta);
    stack.add_named(&archive_page, Some("archive"));

    let (info_page, info_icon, info_title, info_subtitle, info_meta) = info_page();
    stack.add_named(&info_page, Some("info"));

    BuiltStack {
        stack,
        image_picture,
        image_meta,
        text_view,
        text_meta,
        media_video,
        media_meta,
        archive_list,
        archive_meta,
        info_icon,
        info_title,
        info_subtitle,
        info_meta,
    }
}

fn loading_page() -> gtk4::Box {
    let spinner = gtk4::Spinner::new();
    spinner.set_spinning(true);
    spinner.set_width_request(32);
    spinner.set_height_request(32);
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    page.set_valign(gtk4::Align::Center);
    page.set_halign(gtk4::Align::Center);
    page.set_vexpand(true);
    page.append(&spinner);
    page
}

#[allow(clippy::type_complexity)]
fn info_page() -> (
    gtk4::Box,
    gtk4::Image,
    gtk4::Label,
    gtk4::Label,
    gtk4::Label,
) {
    let icon = gtk4::Image::new();
    icon.set_pixel_size(96);
    icon.set_margin_top(32);

    let title = gtk4::Label::new(None);
    title.add_css_class("title-2");
    title.set_wrap(true);
    title.set_justify(gtk4::Justification::Center);

    let subtitle = gtk4::Label::new(None);
    subtitle.add_css_class("dim-label");

    let meta = meta_label();
    meta.set_justify(gtk4::Justification::Center);
    meta.set_halign(gtk4::Align::Center);

    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    page.set_valign(gtk4::Align::Center);
    page.set_vexpand(true);
    page.set_margin_start(16);
    page.set_margin_end(16);
    page.append(&icon);
    page.append(&title);
    page.append(&subtitle);
    page.append(&meta);
    (page, icon, title, subtitle, meta)
}

fn meta_label() -> gtk4::Label {
    let label = gtk4::Label::new(None);
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.add_css_class("dim-label");
    label.add_css_class("caption");
    label.set_margin_start(12);
    label.set_margin_end(12);
    label.set_margin_bottom(12);
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_preview_cap_is_256_kib() {
        assert_eq!(TEXT_PREVIEW_CAP_BYTES, 256 * 1024);
    }
}
