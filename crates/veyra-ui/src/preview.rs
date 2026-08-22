//! Faz 10: the right-hand Preview Sidebar. Shows a type-specific card for
//! whichever file is currently selected in the focused panel's active tab —
//! images, text/code, PDFs/documents, audio/video, archives, directories,
//! symlinks, and anything else — or an empty state when nothing is
//! selected.
//!
//! All file content reads run off the GTK main thread via `fs_async`
//! (images, text, directory child counts); everything else (size, MIME
//! type, modified date) is already sitting on the `FileItem` from the
//! directory listing, so those cards render synchronously. A generation
//! counter discards stale async results, so rapid keyboard-driven selection
//! changes never let a slow, older preview overwrite a newer one (Rule
//! #11-#14).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use veyra_filesystem::{FileItem, FileKind, FsError, VeyraPath};

use crate::config::{format_size, SharedSettings};
use crate::fs_async;
use crate::i18n::t;

/// `PreviewPanelHandles::widget` is the single top-level widget callers
/// embed in the window layout; every other field is an internal handle used
/// by `show` to update whichever card is currently visible.
#[derive(Clone)]
pub(crate) struct PreviewPanelHandles {
    pub widget: gtk4::Widget,
    generation: Rc<Cell<u64>>,
    current_path: Rc<RefCell<Option<VeyraPath>>>,
    stack: gtk4::Stack,
    image_picture: gtk4::Picture,
    image_meta: gtk4::Label,
    text_view: gtk4::TextView,
    text_meta: gtk4::Label,
    info_icon: gtk4::Image,
    info_title: gtk4::Label,
    info_subtitle: gtk4::Label,
    info_meta: gtk4::Label,
    info_open_button: gtk4::Button,
    /// Faz 34: read live at render time for the text-preview size cap
    /// (`max_preview_size_kb`) and whether a directory's card includes its
    /// child count (`show_folder_count`).
    settings: SharedSettings,
}

/// Builds the preview panel, initially showing its empty state. Callers
/// decide when/whether `widget` is visible (Faz 10's `F9` toggle).
pub(crate) fn build(settings: SharedSettings) -> PreviewPanelHandles {
    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    stack.set_vexpand(true);
    stack.set_size_request(260, -1);

    let empty_page = adw::StatusPage::builder()
        .icon_name("view-reveal-symbolic")
        .title(t("preview.empty.title"))
        .description(t("preview.empty.description"))
        .vexpand(true)
        .build();
    stack.add_named(&empty_page, Some("empty"));
    stack.add_named(&loading_page(), Some("loading"));

    let (image_page, image_picture, image_meta) = image_page();
    stack.add_named(&image_page, Some("image"));

    let (text_page, text_view, text_meta) = text_page();
    stack.add_named(&text_page, Some("text"));

    let (info_page, info_icon, info_title, info_subtitle, info_meta, info_open_button) =
        info_page();
    stack.add_named(&info_page, Some("info"));

    stack.set_visible_child_name("empty");

    let current_path: Rc<RefCell<Option<VeyraPath>>> = Rc::new(RefCell::new(None));
    {
        let current_path = current_path.clone();
        info_open_button.connect_clicked(move |_| {
            let Some(path) = current_path.borrow().clone() else {
                return;
            };
            std::thread::spawn(move || {
                if let Err(err) = veyra_filesystem::open(&path) {
                    tracing::warn!(path = %veyra_core::security::log_path(&path), error = %err, "failed to open previewed item");
                }
            });
        });
    }

    PreviewPanelHandles {
        widget: stack.clone().upcast(),
        generation: Rc::new(Cell::new(0)),
        current_path,
        stack,
        image_picture,
        image_meta,
        text_view,
        text_meta,
        info_icon,
        info_title,
        info_subtitle,
        info_meta,
        info_open_button,
        settings,
    }
}

fn loading_page() -> gtk4::Box {
    let spinner = gtk4::Spinner::new();
    spinner.set_spinning(true);
    spinner.set_width_request(32);
    spinner.set_height_request(32);

    let label = gtk4::Label::new(Some(t("preview.loading")));
    label.add_css_class("dim-label");

    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    page.set_valign(gtk4::Align::Center);
    page.set_halign(gtk4::Align::Center);
    page.set_vexpand(true);
    page.append(&spinner);
    page.append(&label);
    page
}

fn image_page() -> (gtk4::Box, gtk4::Picture, gtk4::Label) {
    let picture = gtk4::Picture::new();
    picture.set_can_shrink(true);
    picture.set_vexpand(true);
    picture.set_margin_top(12);
    picture.set_margin_start(12);
    picture.set_margin_end(12);

    let meta = meta_label();

    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    page.append(&picture);
    page.append(&meta);
    (page, picture, meta)
}

fn text_page() -> (gtk4::Box, gtk4::TextView, gtk4::Label) {
    let text_view = gtk4::TextView::new();
    text_view.set_editable(false);
    text_view.set_cursor_visible(false);
    text_view.set_monospace(true);
    text_view.set_wrap_mode(gtk4::WrapMode::WordChar);
    text_view.set_top_margin(8);
    text_view.set_bottom_margin(8);
    text_view.set_left_margin(8);
    text_view.set_right_margin(8);

    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .child(&text_view)
        .build();

    let meta = meta_label();

    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    page.append(&scrolled);
    page.append(&meta);
    (page, text_view, meta)
}

#[allow(clippy::type_complexity)]
fn info_page() -> (
    gtk4::Box,
    gtk4::Image,
    gtk4::Label,
    gtk4::Label,
    gtk4::Label,
    gtk4::Button,
) {
    let icon = gtk4::Image::new();
    icon.set_pixel_size(64);
    icon.set_margin_top(24);

    let title = gtk4::Label::new(None);
    title.add_css_class("title-2");
    title.set_wrap(true);
    title.set_justify(gtk4::Justification::Center);

    let subtitle = gtk4::Label::new(None);
    subtitle.add_css_class("dim-label");

    let meta = meta_label();
    meta.set_justify(gtk4::Justification::Center);
    meta.set_halign(gtk4::Align::Center);

    let open_button = gtk4::Button::with_label(t("preview.open_in_default_app"));
    open_button.set_halign(gtk4::Align::Center);
    open_button.set_margin_top(12);
    open_button.add_css_class("pill");

    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    page.set_valign(gtk4::Align::Center);
    page.set_vexpand(true);
    page.set_margin_start(16);
    page.set_margin_end(16);
    page.append(&icon);
    page.append(&title);
    page.append(&subtitle);
    page.append(&meta);
    page.append(&open_button);
    (page, icon, title, subtitle, meta, open_button)
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

/// Updates the panel for `item` (`None` clears it back to the empty state).
/// Called whenever the focused panel's active tab/view selection changes.
pub(crate) fn show(handles: &PreviewPanelHandles, item: Option<FileItem>) {
    let generation = handles.generation.get() + 1;
    handles.generation.set(generation);
    *handles.current_path.borrow_mut() = item.as_ref().map(|i| i.path.clone());

    let Some(item) = item else {
        handles.stack.set_visible_child_name("empty");
        return;
    };

    match item.kind().clone() {
        FileKind::Directory => show_directory(handles, generation, item),
        FileKind::Symlink {
            is_broken: true, ..
        } => show_error(handles, &item, t("preview.error.broken_symlink")),
        FileKind::Symlink { target, .. } => show_symlink(handles, &item, target),
        FileKind::Regular => show_regular(handles, generation, item),
        other => show_info_card(
            handles,
            &item,
            crate::views::icon_name_for(&item),
            kind_label(&other),
            false,
        ),
    }
}

fn show_regular(handles: &PreviewPanelHandles, generation: u64, item: FileItem) {
    let mime = item.metadata.mime_type.clone();
    if is_image_mime(&mime) {
        show_image(handles, generation, item);
    } else if is_text_mime(&mime) {
        show_text(handles, generation, item);
    } else if mime == "application/pdf" {
        show_info_card(
            handles,
            &item,
            "x-office-document-symbolic",
            t("preview.kind.pdf"),
            true,
        );
    } else if mime.starts_with("audio/") {
        show_info_card(
            handles,
            &item,
            "audio-x-generic-symbolic",
            t("preview.kind.audio"),
            true,
        );
    } else if mime.starts_with("video/") {
        show_info_card(
            handles,
            &item,
            "video-x-generic-symbolic",
            t("preview.kind.video"),
            true,
        );
    } else if is_archive_mime(&mime) {
        show_info_card(
            handles,
            &item,
            "package-x-generic-symbolic",
            t("preview.kind.archive"),
            true,
        );
    } else {
        show_info_card(
            handles,
            &item,
            crate::views::icon_name_for(&item),
            t("preview.kind.file"),
            true,
        );
    }
}

/// A decoded image's raw pixel buffer plus the layout GdkPixbuf needs to
/// reinterpret it, carried across the `fs_async` channel from the
/// background-thread decode back to the main thread. Every field is `Send`
/// (`glib::Bytes` explicitly so, the rest are plain `Copy` values), unlike
/// `gdk_pixbuf::Pixbuf`/`gdk4::Texture` themselves, which are GTK-main-thread-
/// bound GObjects and can never cross that channel directly.
pub(crate) struct DecodedImage {
    pub pixels: glib::Bytes,
    pub colorspace: gtk4::gdk_pixbuf::Colorspace,
    pub has_alpha: bool,
    pub bits_per_sample: i32,
    pub width: i32,
    pub height: i32,
    pub rowstride: i32,
}

/// Reads and decodes `path`'s image content off the GTK main thread.
/// `gdk_pixbuf::Pixbuf` decoding (unlike `gdk4::Texture` construction) has no
/// main-thread requirement, so it can run here alongside the file read.
pub(crate) fn decode_image(path: &VeyraPath) -> Result<DecodedImage, glib::Error> {
    let (raw, _) = path
        .to_gio_file()
        .load_contents(gtk4::gio::Cancellable::NONE)?;
    let pixbuf = gtk4::gdk_pixbuf::Pixbuf::from_read(std::io::Cursor::new(raw.to_vec()))?;
    Ok(DecodedImage {
        pixels: pixbuf.read_pixel_bytes(),
        colorspace: pixbuf.colorspace(),
        has_alpha: pixbuf.has_alpha(),
        bits_per_sample: pixbuf.bits_per_sample(),
        width: pixbuf.width(),
        height: pixbuf.height(),
        rowstride: pixbuf.rowstride(),
    })
}

fn show_image(handles: &PreviewPanelHandles, generation: u64, item: FileItem) {
    handles.stack.set_visible_child_name("loading");
    let path = item.path.clone();
    let handles_done = handles.clone();
    fs_async::run_blocking(
        move || decode_image(&path),
        move |result| {
            if !is_current(&handles_done, generation) {
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
                    let meta = format!(
                        "{} × {} px\n{}\n{}\n{}",
                        decoded.width,
                        decoded.height,
                        format_size(item.metadata.size_bytes),
                        item.metadata.mime_type,
                        modified_label(&item),
                    );
                    handles_done.image_meta.set_label(&meta);
                    handles_done.stack.set_visible_child_name("image");
                }
                Err(err) => {
                    tracing::warn!(path = %veyra_core::security::log_path(&item.path), error = %err, "failed to read image for preview");
                    show_error(&handles_done, &item, &friendly_gio_error(&err));
                }
            }
        },
    );
}

fn show_text(handles: &PreviewPanelHandles, generation: u64, item: FileItem) {
    handles.stack.set_visible_child_name("loading");
    let path = item.path.clone();
    let handles_done = handles.clone();
    // Faz 34: the Preview page's "Preview Size Limit" setting, read fresh on
    // every preview render rather than captured once at panel construction.
    let cap_bytes = handles.settings.borrow().max_preview_size_kb * 1024;
    fs_async::run_blocking(
        move || read_capped(&path, cap_bytes),
        move |result| {
            if !is_current(&handles_done, generation) {
                return;
            }
            match result {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    let truncated = item.metadata.size_bytes as usize > bytes.len();
                    handles_done.text_view.buffer().set_text(&text);
                    let mut meta = format!(
                        "{} lines · {} characters\n{} · {}",
                        text.lines().count(),
                        text.chars().count(),
                        format_size(item.metadata.size_bytes),
                        item.metadata.mime_type,
                    );
                    if truncated {
                        meta.push_str(&format!(
                            "\n(showing first {} only)",
                            format_size(cap_bytes as u64)
                        ));
                    }
                    handles_done.text_meta.set_label(&meta);
                    handles_done.stack.set_visible_child_name("text");
                }
                Err(err) => {
                    tracing::warn!(path = %veyra_core::security::log_path(&item.path), error = %err, "failed to read text for preview");
                    show_error(&handles_done, &item, &friendly_gio_error(&err));
                }
            }
        },
    );
}

/// Reads up to `cap` bytes of `path`'s content off the GTK main thread.
pub(crate) fn read_capped(path: &VeyraPath, cap: usize) -> Result<Vec<u8>, glib::Error> {
    let stream = path.to_gio_file().read(gtk4::gio::Cancellable::NONE)?;
    let mut chunk = vec![0u8; 64 * 1024];
    let mut data = Vec::new();
    while data.len() < cap {
        let want = chunk.len().min(cap - data.len());
        let read = stream.read(&mut chunk[..want], gtk4::gio::Cancellable::NONE)?;
        if read == 0 {
            break;
        }
        data.extend_from_slice(&chunk[..read]);
    }
    let _ = stream.close(gtk4::gio::Cancellable::NONE);
    Ok(data)
}

fn show_directory(handles: &PreviewPanelHandles, generation: u64, item: FileItem) {
    // Faz 34: "Show Folder Item Count" — when off, skip the `read_dir` scan
    // entirely (worth avoiding for a directory with many entries) and show
    // the info card immediately without a child count line.
    if !handles.settings.borrow().show_folder_count {
        handles
            .info_icon
            .set_icon_name(Some(crate::views::icon_name_for(&item)));
        handles.info_title.set_label(item.name());
        handles.info_subtitle.set_label(t("preview.kind.folder"));
        handles
            .info_meta
            .set_label(&format!("{}\n{}", item.path, modified_label(&item)));
        handles.info_open_button.set_visible(false);
        handles.stack.set_visible_child_name("info");
        return;
    }

    handles.stack.set_visible_child_name("loading");
    let path = item.path.clone();
    let handles_done = handles.clone();
    fs_async::run_blocking(
        move || veyra_filesystem::read_dir(&path).map(|entries| entries.len()),
        move |result| {
            if !is_current(&handles_done, generation) {
                return;
            }
            match result {
                Ok(count) => {
                    handles_done
                        .info_icon
                        .set_icon_name(Some(crate::views::icon_name_for(&item)));
                    handles_done.info_title.set_label(item.name());
                    handles_done
                        .info_subtitle
                        .set_label(t("preview.kind.folder"));
                    let meta = format!(
                        "{}\n{}\n{}",
                        item.path,
                        entry_count_label(count as u32),
                        modified_label(&item),
                    );
                    handles_done.info_meta.set_label(&meta);
                    handles_done.info_open_button.set_visible(false);
                    handles_done.stack.set_visible_child_name("info");
                }
                Err(err) => {
                    tracing::warn!(path = %veyra_core::security::log_path(&item.path), error = %err, "failed to read directory for preview");
                    show_error(&handles_done, &item, &friendly_fs_error(&err));
                }
            }
        },
    );
}

fn show_symlink(
    handles: &PreviewPanelHandles,
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
    let meta = format!("→ {target_label}\n{}", modified_label(item));
    handles.info_meta.set_label(&meta);
    handles.info_open_button.set_visible(true);
    handles.stack.set_visible_child_name("info");
}

fn show_info_card(
    handles: &PreviewPanelHandles,
    item: &FileItem,
    icon_name: &str,
    subtitle: &str,
    show_open: bool,
) {
    handles.info_icon.set_icon_name(Some(icon_name));
    handles.info_title.set_label(item.name());
    handles.info_subtitle.set_label(subtitle);
    let meta = format!(
        "{}\n{}\n{}",
        format_size(item.metadata.size_bytes),
        item.metadata.mime_type,
        modified_label(item),
    );
    handles.info_meta.set_label(&meta);
    handles.info_open_button.set_visible(show_open);
    handles.stack.set_visible_child_name("info");
}

fn show_error(handles: &PreviewPanelHandles, item: &FileItem, reason: &str) {
    handles
        .info_icon
        .set_icon_name(Some("dialog-warning-symbolic"));
    handles.info_title.set_label(t("preview.error.unable"));
    handles.info_subtitle.set_label(reason);
    handles.info_meta.set_label(item.name());
    handles.info_open_button.set_visible(true);
    handles.stack.set_visible_child_name("info");
}

fn is_current(handles: &PreviewPanelHandles, generation: u64) -> bool {
    handles.generation.get() == generation
}

pub(crate) fn modified_label(item: &FileItem) -> String {
    item.metadata
        .modified
        .map(|dt| format!("Modified {}", crate::config::format_timestamp(dt)))
        .unwrap_or_else(|| "Modified: unknown".to_string())
}

fn entry_count_label(count: u32) -> String {
    if count == 1 {
        "1 item".to_string()
    } else {
        format!("{count} items")
    }
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

pub(crate) fn is_image_mime(mime: &str) -> bool {
    mime.starts_with("image/")
}

pub(crate) fn is_text_mime(mime: &str) -> bool {
    mime.starts_with("text/")
        || matches!(
            mime,
            "application/json"
                | "application/xml"
                | "application/x-yaml"
                | "application/yaml"
                | "application/toml"
                | "application/x-toml"
                | "application/javascript"
                | "application/x-javascript"
                | "application/x-sh"
                | "application/x-shellscript"
                | "application/x-csv"
        )
}

pub(crate) fn is_archive_mime(mime: &str) -> bool {
    matches!(
        mime,
        "application/zip"
            | "application/x-tar"
            | "application/gzip"
            | "application/x-gzip"
            | "application/x-7z-compressed"
            | "application/x-xz"
            | "application/x-bzip2"
            | "application/x-compressed-tar"
            | "application/x-rar"
            | "application/vnd.rar"
    )
}

/// User-facing message for a `glib::Error` from an image/text preview read
/// (Faz 10 requirement C.3 — never surface a raw GLib error string).
pub(crate) fn friendly_gio_error(err: &glib::Error) -> String {
    if err.matches(gtk4::gio::IOErrorEnum::PermissionDenied) {
        "Permission denied".to_string()
    } else if err.matches(gtk4::gio::IOErrorEnum::NotFound) {
        "File not found".to_string()
    } else {
        "Unable to read file".to_string()
    }
}

pub(crate) fn friendly_fs_error(err: &FsError) -> String {
    match err {
        FsError::NotFound(_) => "File not found".to_string(),
        FsError::PermissionDenied(_) => "Permission denied".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_mime_detection() {
        assert!(is_image_mime("image/png"));
        assert!(is_image_mime("image/svg+xml"));
        assert!(!is_image_mime("application/pdf"));
    }

    #[test]
    fn text_mime_detection() {
        assert!(is_text_mime("text/plain"));
        assert!(is_text_mime("text/x-rust"));
        assert!(is_text_mime("application/json"));
        assert!(is_text_mime("application/toml"));
        assert!(!is_text_mime("application/pdf"));
        assert!(!is_text_mime("image/png"));
    }

    #[test]
    fn archive_mime_detection() {
        assert!(is_archive_mime("application/zip"));
        assert!(is_archive_mime("application/x-7z-compressed"));
        assert!(!is_archive_mime("application/pdf"));
    }

    #[test]
    fn entry_count_label_pluralizes_correctly() {
        assert_eq!(entry_count_label(0), "0 items");
        assert_eq!(entry_count_label(1), "1 item");
        assert_eq!(entry_count_label(5), "5 items");
    }

    #[test]
    fn kind_label_covers_special_files() {
        assert_eq!(kind_label(&FileKind::Fifo), t("preview.kind.pipe"));
        assert_eq!(kind_label(&FileKind::Socket), t("preview.kind.socket"));
        assert_eq!(kind_label(&FileKind::Unknown), t("preview.kind.special"));
    }
}
