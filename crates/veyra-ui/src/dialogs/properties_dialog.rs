//! Faz 12: the Properties window. An `AdwPreferencesDialog` (General/
//! Permissions/Advanced pages, each an `AdwPreferencesPage` — the switcher
//! between them, including the narrow-width fallback to a bottom switcher
//! bar, is entirely `AdwPreferencesDialog`'s own built-in behavior) opened
//! from the context menu's "Properties" entry or `Alt+Enter`.
//!
//! The window itself, and its General/Advanced pages, open immediately from
//! whatever the caller's already-loaded `FileItem` already knows — no I/O.
//! Everything that costs an extra stat (disk usage, device id, filesystem
//! type) or a full tree walk (a folder's recursive "Contains" count) is
//! queried afterwards via `fs_async::run_blocking`, per Rule #11/#12, and
//! back-fills the relevant row once it lands. The folder count additionally
//! runs behind an `OperationControl` cancelled when the dialog closes, since
//! it's the one Properties computation that can take a long time on a huge
//! tree (Rule #13).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use chrono::{DateTime, Local, Utc};
use gtk4::prelude::*;
use gtk4::{gio, glib};
use libadwaita as adw;
use libadwaita::prelude::*;

use veyra_filesystem::{FileItem, FileKind, FilePermissions, OperationControl, VeyraPath};

use crate::dialogs::file_associations_dialog;
use crate::fs_async;
use crate::open_with;
use crate::privileged;
use crate::thumbnails::ThumbnailService;
use crate::views::icon_name_for;

/// Shows the Properties window for `item`, parented to `parent`.
/// `on_permissions_changed` is called after every successful permission
/// change so the caller can refresh whatever view is currently showing the
/// item (e.g. its executable-bit icon).
pub(crate) fn show(
    parent: &impl IsA<gtk4::Widget>,
    item: FileItem,
    thumbnails: Rc<ThumbnailService>,
    on_permissions_changed: Rc<dyn Fn()>,
) {
    let dialog = adw::PreferencesDialog::builder()
        .title("Properties")
        .content_width(480)
        .content_height(560)
        .build();

    let general = build_general_page(&item, &thumbnails, parent);
    dialog.add(&general.page);

    if item.metadata.permissions.is_some() {
        let permissions_page =
            build_permissions_page(&dialog, &item, on_permissions_changed.clone());
        dialog.add(&permissions_page);
    }

    let advanced = build_advanced_page(&item);
    dialog.add(&advanced.page);

    // Single fast extra stat: backs the General page's Disk Usage row and
    // the Advanced page's Device/Filesystem rows.
    {
        let path = item.path.clone();
        let disk_usage_row = general.disk_usage_row.clone();
        let device_row = advanced.device_row.clone();
        let filesystem_row = advanced.filesystem_row.clone();
        fs_async::run_blocking(
            move || veyra_filesystem::stat_advanced(&path),
            move |result| match result {
                Ok(info) => {
                    disk_usage_row.set_subtitle(&info.disk_usage_bytes.map_or_else(
                        || "Unknown".to_string(),
                        |b| {
                            format!(
                                "{} ({} bytes)",
                                veyra_filesystem::format_size(b),
                                format_bytes_grouped(b)
                            )
                        },
                    ));
                    device_row.set_subtitle(
                        &info
                            .device_id
                            .map_or_else(|| "Unknown".to_string(), |d| d.to_string()),
                    );
                    filesystem_row.set_subtitle(
                        &info
                            .filesystem_type
                            .unwrap_or_else(|| "Unknown".to_string()),
                    );
                }
                Err(err) => {
                    disk_usage_row.set_subtitle("Unavailable");
                    device_row.set_subtitle("Unavailable");
                    filesystem_row.set_subtitle(&err.to_string());
                }
            },
        );
    }

    // Slow, cancellable recursive descendant count for the "Contains" row.
    if let Some((contains_row, spinner)) = general.contains {
        let control = OperationControl::new();
        {
            let control = control.clone();
            dialog.connect_closed(move |_| control.cancel());
        }
        let path = item.path.clone();
        fs_async::run_blocking(
            move || veyra_filesystem::count_dir_recursive(&path, &control),
            move |result| {
                spinner.set_visible(false);
                match result {
                    Ok(count) => contains_row.set_subtitle(&format!(
                        "{} files, {} folders ({})",
                        count.file_count,
                        count.dir_count,
                        veyra_filesystem::format_size(count.total_size)
                    )),
                    Err(err) => contains_row.set_subtitle(&format!("Unavailable: {err}")),
                }
            },
        );
    }

    dialog.present(Some(parent));
}

struct GeneralPageHandles {
    page: adw::PreferencesPage,
    disk_usage_row: adw::ActionRow,
    contains: Option<(adw::ActionRow, gtk4::Spinner)>,
}

fn build_general_page(
    item: &FileItem,
    thumbnails: &Rc<ThumbnailService>,
    parent: &impl IsA<gtk4::Widget>,
) -> GeneralPageHandles {
    let page = adw::PreferencesPage::builder()
        .title("General")
        .icon_name("dialog-information-symbolic")
        .build();

    let header_group = adw::PreferencesGroup::new();
    let header_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    header_box.set_halign(gtk4::Align::Center);
    let icon = gtk4::Image::new();
    icon.set_pixel_size(64);
    icon.set_icon_name(Some(icon_name_for(item)));
    thumbnails.bind(&icon, item);
    header_box.append(&icon);
    let name_label = gtk4::Label::new(Some(item.name()));
    name_label.add_css_class("title-2");
    name_label.set_wrap(true);
    name_label.set_justify(gtk4::Justification::Center);
    header_box.append(&name_label);
    header_group.add(&header_box);
    page.add(&header_group);

    let info_group = adw::PreferencesGroup::new();
    info_group.add(
        &adw::ActionRow::builder()
            .title("Type")
            .subtitle(describe_type(item))
            .title_lines(1)
            .subtitle_lines(2)
            .build(),
    );

    // For regular files only, add a "Default Application" row
    if matches!(item.kind(), FileKind::Regular) {
        let default_app = open_with::default_app(&item.metadata.mime_type);
        let default_app_row = adw::ActionRow::builder()
            .title("Default Application")
            .title_lines(1)
            .subtitle_lines(1)
            .build();
        default_app_row.set_use_markup(false);

        // Suffix: app icon/name and "Change…" button
        let suffix_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        suffix_box.set_halign(gtk4::Align::End);
        suffix_box.set_valign(gtk4::Align::Center);

        let app_label = gtk4::Label::new(None);
        app_label.add_css_class("dim-label");
        app_label.add_css_class("caption");
        let app_text = default_app
            .as_ref()
            .map(|a| a.name().to_string())
            .unwrap_or_else(|| "None".to_string());
        app_label.set_text(&app_text);
        suffix_box.append(&app_label);

        let change_button = gtk4::Button::with_label("Change…");
        change_button.add_css_class("flat");
        change_button.set_valign(gtk4::Align::Center);

        let content_type_for_button = item.metadata.mime_type.clone();
        let parent = parent.clone();
        let app_label = app_label.clone();
        change_button.connect_clicked(move |_| {
            let on_changed = {
                let app_label = app_label.clone();
                Rc::new(move |app: Option<gio::AppInfo>| {
                    let text = app
                        .as_ref()
                        .map(|a| a.name().to_string())
                        .unwrap_or_else(|| "None".to_string());
                    app_label.set_text(&text);
                })
            };
            file_associations_dialog::pick_default_app(
                &parent,
                &content_type_for_button,
                on_changed,
            );
        });
        suffix_box.append(&change_button);

        default_app_row.add_suffix(&suffix_box);
        info_group.add(&default_app_row);
    }

    let location_row = adw::ActionRow::builder()
        .title("Location")
        .subtitle(parent_display(&item.path))
        .title_lines(1)
        .subtitle_lines(2)
        .build();
    let copy_path_button = gtk4::Button::from_icon_name("edit-copy-symbolic");
    copy_path_button.set_valign(gtk4::Align::Center);
    copy_path_button.add_css_class("flat");
    copy_path_button.set_tooltip_text(Some("Copy Path"));
    {
        let path = item.path.clone();
        copy_path_button.connect_clicked(move |button| {
            button.clipboard().set_text(&path.to_string());
        });
    }
    location_row.add_suffix(&copy_path_button);
    info_group.add(&location_row);

    info_group.add(
        &adw::ActionRow::builder()
            .title("Size")
            .subtitle(format!(
                "{} ({} bytes)",
                item.metadata.size_human(),
                format_bytes_grouped(item.metadata.size_bytes)
            ))
            .title_lines(1)
            .subtitle_lines(2)
            .build(),
    );

    let disk_usage_row = adw::ActionRow::builder()
        .title("Disk Usage")
        .subtitle("Calculating…")
        .title_lines(1)
        .subtitle_lines(2)
        .build();
    info_group.add(&disk_usage_row);

    let contains = if item.kind().is_directory() {
        let row = adw::ActionRow::builder()
            .title("Contains")
            .subtitle("Calculating…")
            .title_lines(1)
            .subtitle_lines(2)
            .build();
        let spinner = gtk4::Spinner::new();
        spinner.set_spinning(true);
        spinner.set_valign(gtk4::Align::Center);
        row.add_suffix(&spinner);
        info_group.add(&row);
        Some((row, spinner))
    } else {
        None
    };
    page.add(&info_group);

    let time_group = adw::PreferencesGroup::new();
    time_group.add(&timestamp_row("Created", item.metadata.created));
    time_group.add(&timestamp_row("Modified", item.metadata.modified));
    time_group.add(&timestamp_row("Accessed", item.metadata.accessed));
    page.add(&time_group);

    GeneralPageHandles {
        page,
        disk_usage_row,
        contains,
    }
}

struct AdvancedPageHandles {
    page: adw::PreferencesPage,
    device_row: adw::ActionRow,
    filesystem_row: adw::ActionRow,
}

fn build_advanced_page(item: &FileItem) -> AdvancedPageHandles {
    let page = adw::PreferencesPage::builder()
        .title("Advanced")
        .icon_name("applications-engineering-symbolic")
        .build();

    let group = adw::PreferencesGroup::new();
    group.add(
        &adw::ActionRow::builder()
            .title("MIME Type")
            .subtitle(item.metadata.mime_type.clone())
            .title_lines(1)
            .subtitle_lines(2)
            .build(),
    );
    group.add(
        &adw::ActionRow::builder()
            .title("Inode")
            .subtitle(
                item.metadata
                    .inode
                    .map_or_else(|| "Unknown".to_string(), |i| i.to_string()),
            )
            .title_lines(1)
            .subtitle_lines(2)
            .build(),
    );
    let device_row = adw::ActionRow::builder()
        .title("Device")
        .subtitle("Calculating…")
        .title_lines(1)
        .subtitle_lines(2)
        .build();
    group.add(&device_row);
    let filesystem_row = adw::ActionRow::builder()
        .title("Filesystem")
        .subtitle("Calculating…")
        .title_lines(1)
        .subtitle_lines(2)
        .build();
    group.add(&filesystem_row);
    page.add(&group);

    if let FileKind::Symlink { target, is_broken } = item.kind() {
        let link_group = adw::PreferencesGroup::builder()
            .title("Symbolic Link")
            .build();
        let target_text = target
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |t| t.display().to_string());
        link_group.add(
            &adw::ActionRow::builder()
                .title("Target")
                .subtitle(target_text)
                .title_lines(1)
                .subtitle_lines(2)
                .build(),
        );
        let status = if *is_broken {
            "Broken — target does not exist"
        } else {
            "Valid"
        };
        link_group.add(
            &adw::ActionRow::builder()
                .title("Status")
                .subtitle(status)
                .title_lines(1)
                .subtitle_lines(2)
                .build(),
        );
        page.add(&link_group);
    }

    AdvancedPageHandles {
        page,
        device_row,
        filesystem_row,
    }
}

/// Builds the Permissions page: ownership (read-only), an editable octal mode,
/// Faz 28 special bits (SUID/SGID/Sticky), a Read/Write/Execute switch matrix
/// per owner/group/other, a convenience "allow executing as program" switch for
/// regular files, and an "Apply Permissions to Enclosed Files…" button for
/// directories (with recursive chmod retry-as-admin support). Every switch and
/// the mode entry apply their change to disk immediately via a centralized
/// apply function, reverting on failure with error dialog offering
/// "Retry as Administrator" for permission-denied errors.
fn build_permissions_page(
    dialog: &adw::PreferencesDialog,
    item: &FileItem,
    on_changed: Rc<dyn Fn()>,
) -> adw::PreferencesPage {
    let permissions = item
        .metadata
        .permissions
        .expect("page only built when FileMetadata::permissions is Some");
    let path = item.path.clone();
    let state = Rc::new(Cell::new(permissions));

    let page = adw::PreferencesPage::builder()
        .title("Permissions")
        .icon_name("system-lock-screen-symbolic")
        .build();

    let ownership_group = adw::PreferencesGroup::builder().title("Ownership").build();
    ownership_group.add(
        &adw::ActionRow::builder()
            .title("Owner")
            .subtitle(
                item.metadata
                    .owner
                    .clone()
                    .unwrap_or_else(|| "Unknown".to_string()),
            )
            .title_lines(1)
            .subtitle_lines(2)
            .build(),
    );
    ownership_group.add(
        &adw::ActionRow::builder()
            .title("Group")
            .subtitle(
                item.metadata
                    .group
                    .clone()
                    .unwrap_or_else(|| "Unknown".to_string()),
            )
            .title_lines(1)
            .subtitle_lines(2)
            .build(),
    );
    page.add(&ownership_group);

    // Faz 28: Editable octal mode entry with validation
    let mode_group = adw::PreferencesGroup::new();
    let mode_row = adw::ActionRow::builder()
        .title("Mode")
        .subtitle(mode_subtitle(permissions))
        .title_lines(1)
        .subtitle_lines(2)
        .build();
    let mode_entry = gtk4::Entry::new();
    mode_entry.set_text(&permissions.octal_string());
    mode_entry.set_width_chars(6);
    mode_entry.set_halign(gtk4::Align::End);
    mode_entry.set_valign(gtk4::Align::Center);
    mode_entry.add_css_class("monospace");
    let mode_error_label = gtk4::Label::new(Some(""));
    mode_error_label.add_css_class("dim-label");
    mode_error_label.add_css_class("caption");
    mode_error_label.set_halign(gtk4::Align::End);
    let entry_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    entry_box.append(&mode_entry);
    entry_box.append(&mode_error_label);
    mode_row.add_suffix(&entry_box);
    mode_group.add(&mode_row);
    page.add(&mode_group);

    // Faz 28: Special permissions group (SUID/SGID/Sticky)
    let mut special_switches: Option<(adw::SwitchRow, adw::SwitchRow, adw::SwitchRow)> = None;
    if !matches!(item.kind(), FileKind::Symlink { .. }) {
        let special_group = adw::PreferencesGroup::builder()
            .title("Special Permissions")
            .build();

        let suid_switch = adw::SwitchRow::builder()
            .title("Set User ID (SUID)")
            .active(permissions.is_setuid())
            .build();
        special_group.add(&suid_switch);
        wire_switch(
            &suid_switch,
            dialog,
            &path,
            &state,
            &mode_row,
            &on_changed,
            FilePermissions::with_setuid,
        );

        let sgid_switch = adw::SwitchRow::builder()
            .title("Set Group ID (SGID)")
            .active(permissions.is_setgid())
            .build();
        special_group.add(&sgid_switch);
        wire_switch(
            &sgid_switch,
            dialog,
            &path,
            &state,
            &mode_row,
            &on_changed,
            FilePermissions::with_setgid,
        );

        let sticky_switch = adw::SwitchRow::builder()
            .title("Sticky Bit")
            .active(permissions.is_sticky())
            .build();
        special_group.add(&sticky_switch);
        wire_switch(
            &sticky_switch,
            dialog,
            &path,
            &state,
            &mode_row,
            &on_changed,
            FilePermissions::with_sticky,
        );

        page.add(&special_group);
        special_switches = Some((suid_switch, sgid_switch, sticky_switch));
    }

    let (owner_group, owner_switches) = build_class_group(
        "Owner",
        permissions.is_owner_readable(),
        permissions.is_owner_writable(),
        permissions.is_owner_executable(),
    );
    page.add(&owner_group);
    wire_switch(
        &owner_switches.0,
        dialog,
        &path,
        &state,
        &mode_row,
        &on_changed,
        FilePermissions::with_owner_read,
    );
    wire_switch(
        &owner_switches.1,
        dialog,
        &path,
        &state,
        &mode_row,
        &on_changed,
        FilePermissions::with_owner_write,
    );
    wire_switch(
        &owner_switches.2,
        dialog,
        &path,
        &state,
        &mode_row,
        &on_changed,
        FilePermissions::with_owner_execute,
    );

    let (group_group, group_switches) = build_class_group(
        "Group",
        permissions.is_group_readable(),
        permissions.is_group_writable(),
        permissions.is_group_executable(),
    );
    page.add(&group_group);
    wire_switch(
        &group_switches.0,
        dialog,
        &path,
        &state,
        &mode_row,
        &on_changed,
        FilePermissions::with_group_read,
    );
    wire_switch(
        &group_switches.1,
        dialog,
        &path,
        &state,
        &mode_row,
        &on_changed,
        FilePermissions::with_group_write,
    );
    wire_switch(
        &group_switches.2,
        dialog,
        &path,
        &state,
        &mode_row,
        &on_changed,
        FilePermissions::with_group_execute,
    );

    let (other_group, other_switches) = build_class_group(
        "Others",
        permissions.is_other_readable(),
        permissions.is_other_writable(),
        permissions.is_other_executable(),
    );
    page.add(&other_group);
    wire_switch(
        &other_switches.0,
        dialog,
        &path,
        &state,
        &mode_row,
        &on_changed,
        FilePermissions::with_other_read,
    );
    wire_switch(
        &other_switches.1,
        dialog,
        &path,
        &state,
        &mode_row,
        &on_changed,
        FilePermissions::with_other_write,
    );
    wire_switch(
        &other_switches.2,
        dialog,
        &path,
        &state,
        &mode_row,
        &on_changed,
        FilePermissions::with_other_execute,
    );

    // Faz 28: Wire the mode entry to apply via parsing octal, syncing every
    // Read/Write/Execute (and, when present, special-bit) switch to the
    // newly applied mode on success. Each switch's own `wire_switch` handler
    // re-derives its bit from `state` (already updated below before the
    // switches are touched), so the resulting `set_active` calls are
    // idempotent no-ops on disk rather than a cascade of conflicting writes.
    {
        let dialog_for_entry = dialog.clone();
        let path = path.clone();
        let state = state.clone();
        let mode_row = mode_row.clone();
        let on_changed = on_changed.clone();
        let mode_error_label = mode_error_label.clone();
        let owner_switches = owner_switches.clone();
        let group_switches = group_switches.clone();
        let other_switches = other_switches.clone();
        let special_switches = special_switches.clone();
        mode_entry.connect_activate(move |entry| {
            if let Some(parsed) = FilePermissions::parse_octal(entry.text().as_str()) {
                mode_error_label.set_text("");
                let previous = state.get();
                state.set(parsed);
                mode_row.set_subtitle(&mode_subtitle(parsed));

                let path = path.clone();
                let state = state.clone();
                let mode_row = mode_row.clone();
                let on_changed = on_changed.clone();
                let dialog_for_result = dialog_for_entry.clone();
                let entry = entry.clone();
                let owner_switches = owner_switches.clone();
                let group_switches = group_switches.clone();
                let other_switches = other_switches.clone();
                let special_switches = special_switches.clone();
                fs_async::run_blocking(
                    move || veyra_filesystem::set_permissions(&path, parsed),
                    move |result| match result {
                        Ok(()) => {
                            owner_switches.0.set_active(parsed.is_owner_readable());
                            owner_switches.1.set_active(parsed.is_owner_writable());
                            owner_switches.2.set_active(parsed.is_owner_executable());
                            group_switches.0.set_active(parsed.is_group_readable());
                            group_switches.1.set_active(parsed.is_group_writable());
                            group_switches.2.set_active(parsed.is_group_executable());
                            other_switches.0.set_active(parsed.is_other_readable());
                            other_switches.1.set_active(parsed.is_other_writable());
                            other_switches.2.set_active(parsed.is_other_executable());
                            if let Some((suid, sgid, sticky)) = &special_switches {
                                suid.set_active(parsed.is_setuid());
                                sgid.set_active(parsed.is_setgid());
                                sticky.set_active(parsed.is_sticky());
                            }
                            on_changed();
                        }
                        Err(err) => {
                            state.set(previous);
                            mode_row.set_subtitle(&mode_subtitle(previous));
                            entry.set_text(&previous.octal_string());
                            show_chmod_error_simple(
                                &dialog_for_result,
                                "Couldn't Change Permissions",
                                &err,
                            );
                        }
                    },
                );
            } else {
                mode_error_label.set_text("Invalid mode");
            }
        });
    }

    if matches!(item.kind(), FileKind::Regular) {
        let execute_group = adw::PreferencesGroup::new();
        let execute_switch = adw::SwitchRow::builder()
            .title("Allow executing file as program")
            .active(permissions.is_executable())
            .build();
        execute_group.add(&execute_switch);
        page.add(&execute_group);
        wire_switch(
            &execute_switch,
            dialog,
            &path,
            &state,
            &mode_row,
            &on_changed,
            |p, enabled| {
                p.with_owner_execute(enabled)
                    .with_group_execute(enabled)
                    .with_other_execute(enabled)
            },
        );
    }

    // Faz 28: "Apply Permissions to Enclosed Files" button for directories
    if item.kind().is_directory() {
        let recursive_group = adw::PreferencesGroup::new();
        let apply_button = gtk4::Button::with_label("Apply Permissions to Enclosed Files…");
        apply_button.add_css_class("suggested-action");
        let apply_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        apply_box.set_margin_top(6);
        apply_box.set_margin_bottom(6);
        apply_box.set_margin_start(12);
        apply_box.set_margin_end(12);
        apply_box.append(&apply_button);
        recursive_group.add(&apply_box);

        {
            let dialog = dialog.clone();
            let path = path.clone();
            let state = state.clone();
            apply_button.connect_clicked(move |_| {
                let current_perms = state.get();
                let confirm_dialog = adw::AlertDialog::builder()
                    .heading("Apply Permissions Recursively")
                    .body(format!(
                        "This will change permissions for every file and folder inside {}. This cannot be undone.",
                        path.file_name().unwrap_or_else(|| "this folder".to_string())
                    ))
                    .build();
                confirm_dialog.add_responses(&[("cancel", "Cancel"), ("apply", "Apply")]);
                confirm_dialog.set_response_appearance("apply", adw::ResponseAppearance::Destructive);
                confirm_dialog.set_default_response(Some("cancel"));
                confirm_dialog.set_close_response("cancel");

                let dialog_for_callback = dialog.clone();
                let path = path.clone();
                confirm_dialog.connect_response(None, move |_, response| {
                    if response == "apply" {
                        show_recursive_chmod_dialog(&dialog_for_callback, &path, current_perms);
                    }
                });
                confirm_dialog.present(Some(&dialog));
            });
        }

        page.add(&recursive_group);
    }

    page
}

fn build_class_group(
    title: &str,
    read: bool,
    write: bool,
    execute: bool,
) -> (
    adw::PreferencesGroup,
    (adw::SwitchRow, adw::SwitchRow, adw::SwitchRow),
) {
    let group = adw::PreferencesGroup::builder().title(title).build();
    let read_row = adw::SwitchRow::builder().title("Read").active(read).build();
    let write_row = adw::SwitchRow::builder()
        .title("Write")
        .active(write)
        .build();
    let execute_row = adw::SwitchRow::builder()
        .title("Execute")
        .active(execute)
        .build();
    group.add(&read_row);
    group.add(&write_row);
    group.add(&execute_row);
    (group, (read_row, write_row, execute_row))
}

/// Wires `switch` to apply `apply_bit(current_state, new_active)` to disk on
/// every toggle. On write failure, the switch is flipped back and `state`/
/// `mode_row` restored to the pre-toggle value — the flip-back is done with
/// this handler blocked, so it doesn't re-enter and attempt a second
/// (redundant, and potentially also-failing) write.
fn wire_switch(
    switch: &adw::SwitchRow,
    dialog: &adw::PreferencesDialog,
    path: &VeyraPath,
    state: &Rc<Cell<FilePermissions>>,
    mode_row: &adw::ActionRow,
    on_changed: &Rc<dyn Fn()>,
    apply_bit: impl Fn(FilePermissions, bool) -> FilePermissions + Clone + 'static,
) {
    let handler_id: Rc<RefCell<Option<glib::SignalHandlerId>>> = Rc::new(RefCell::new(None));
    let id = {
        let dialog = dialog.clone();
        let path = path.clone();
        let state = state.clone();
        let mode_row = mode_row.clone();
        let on_changed = on_changed.clone();
        let handler_id = handler_id.clone();
        switch.connect_active_notify(move |switch| {
            let enabled = switch.is_active();
            let previous = state.get();
            let updated = apply_bit.clone()(previous, enabled);
            state.set(updated);
            mode_row.set_subtitle(&mode_subtitle(updated));

            let switch = switch.clone();
            let dialog = dialog.clone();
            let path = path.clone();
            let state = state.clone();
            let mode_row = mode_row.clone();
            let on_changed = on_changed.clone();
            let handler_id = handler_id.clone();
            fs_async::run_blocking(
                move || veyra_filesystem::set_permissions(&path, updated),
                move |result| match result {
                    Ok(()) => on_changed(),
                    Err(err) => {
                        tracing::warn!(error = %err, "chmod failed");
                        state.set(previous);
                        mode_row.set_subtitle(&mode_subtitle(previous));
                        if let Some(id) = handler_id.borrow().as_ref() {
                            switch.block_signal(id);
                        }
                        switch.set_active(!enabled);
                        if let Some(id) = handler_id.borrow().as_ref() {
                            switch.unblock_signal(id);
                        }
                        show_chmod_error(&dialog, &err);
                    }
                },
            );
        })
    };
    *handler_id.borrow_mut() = Some(id);
}

/// Faz 28: Shows error dialog for chmod failures. Simple version without
/// retry-as-admin to avoid complex closure issues.
fn show_chmod_error_simple(
    parent: &impl IsA<gtk4::Widget>,
    heading: &str,
    err: &veyra_filesystem::FsError,
) {
    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(err.to_string())
        .build();
    dialog.add_responses(&[("ok", "OK")]);
    dialog.set_default_response(Some("ok"));
    dialog.set_close_response("ok");
    dialog.present(Some(parent));
}

fn show_chmod_error(parent: &impl IsA<gtk4::Widget>, err: &veyra_filesystem::FsError) {
    let dialog = adw::AlertDialog::builder()
        .heading("Couldn't Change Permissions")
        .body(err.to_string())
        .build();
    dialog.add_responses(&[("ok", "OK")]);
    dialog.set_default_response(Some("ok"));
    dialog.set_close_response("ok");
    dialog.present(Some(parent));
}

/// Faz 28: Shows a recursive chmod operation dialog with progress/result feedback.
fn show_recursive_chmod_dialog(
    parent: &impl IsA<gtk4::Widget>,
    path: &VeyraPath,
    mode: FilePermissions,
) {
    let parent = parent.clone();
    let path_for_work = path.clone();
    let path_for_result = path.clone();
    fs_async::run_blocking(
        move || {
            let control = veyra_filesystem::OperationControl::new();
            veyra_filesystem::chmod_recursive(&path_for_work, mode, &control)
        },
        move |result| {
            match result {
                Ok(outcome) => {
                    if outcome.errors.is_empty() {
                        let success_dialog = adw::AlertDialog::builder()
                            .heading("Permissions Applied")
                            .body(format!(
                                "Successfully changed permissions for {} items.",
                                outcome.succeeded
                            ))
                            .build();
                        success_dialog.add_responses(&[("ok", "OK")]);
                        success_dialog.present(Some(&parent));
                    } else if outcome
                        .errors
                        .iter()
                        .all(|(_, e)| matches!(e, veyra_filesystem::FsError::PermissionDenied(_)))
                    {
                        // All errors are permission denied, offer retry with admin
                        show_recursive_chmod_retry_admin(&parent, &path_for_result, mode);
                    } else {
                        let error_dialog = adw::AlertDialog::builder()
                            .heading("Permissions Partially Applied")
                            .body(format!(
                                "Successfully changed {} items, but {} failed.",
                                outcome.succeeded,
                                outcome.errors.len()
                            ))
                            .build();
                        error_dialog.add_responses(&[("ok", "OK")]);
                        error_dialog.present(Some(&parent));
                    }
                }
                Err(err) => {
                    show_chmod_error(&parent, &err);
                }
            }
        },
    );
}

/// Faz 28: Shows retry dialog for recursive chmod with permission-denied errors.
fn show_recursive_chmod_retry_admin(
    parent: &impl IsA<gtk4::Widget>,
    path: &VeyraPath,
    mode: FilePermissions,
) {
    if !privileged::is_available() {
        return;
    }

    let path = path.clone();
    let parent_rc = Rc::new(parent.clone());
    let dialog = adw::AlertDialog::builder()
        .heading("Insufficient Permissions")
        .body("Some files couldn't be changed due to insufficient permissions.")
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("retry", "Retry as Administrator")]);
    dialog.set_response_appearance("retry", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("retry"));

    let parent_for_retry = parent_rc.clone();
    dialog.connect_response(None, move |_, response| {
        if response == "retry" {
            let path = path.clone();
            let parent_for_inner = parent_for_retry.clone();
            fs_async::run_blocking(
                move || privileged::chmod(path.as_local_path().unwrap(), &mode, true),
                move |result| {
                    if let Err(err) = result {
                        show_privileged_error(
                            parent_for_inner.as_ref(),
                            "Couldn't Apply Permissions as Root",
                            &err,
                        );
                    }
                },
            );
        }
    });

    dialog.present(Some(parent));
}

/// Faz 28: Shows error dialog for privileged operation failures.
fn show_privileged_error(
    parent: &impl IsA<gtk4::Widget>,
    heading: &str,
    err: &privileged::PrivilegedError,
) {
    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(err.to_string())
        .build();
    dialog.add_responses(&[("ok", "OK")]);
    dialog.set_default_response(Some("ok"));
    dialog.set_close_response("ok");
    dialog.present(Some(parent));
}

fn mode_subtitle(permissions: FilePermissions) -> String {
    format!(
        "{} · {}",
        permissions.octal_string(),
        permissions.symbolic_string()
    )
}

fn timestamp_row(title: &str, value: Option<DateTime<Utc>>) -> adw::ActionRow {
    let subtitle = value.map_or_else(
        || "Unknown".to_string(),
        |dt| {
            dt.with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        },
    );
    adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .title_lines(1)
        .subtitle_lines(2)
        .build()
}

/// Descriptive, human-facing type string: structural kinds get a fixed
/// label, regular files defer to the shared-mime-info description of their
/// MIME type (e.g. "PNG image", falling back to the raw MIME string if the
/// system database doesn't recognize it).
fn describe_type(item: &FileItem) -> String {
    match item.kind() {
        FileKind::Directory => "Folder".to_string(),
        FileKind::Symlink {
            is_broken: true, ..
        } => "Broken Symbolic Link".to_string(),
        FileKind::Symlink { .. } => "Symbolic Link".to_string(),
        FileKind::Fifo => "Named Pipe (FIFO)".to_string(),
        FileKind::Socket => "Socket".to_string(),
        FileKind::BlockDevice => "Block Device".to_string(),
        FileKind::CharDevice => "Character Device".to_string(),
        FileKind::Unknown => item.metadata.mime_type.clone(),
        FileKind::Regular => {
            let description = gio::content_type_get_description(&item.metadata.mime_type);
            if description.is_empty() {
                item.metadata.mime_type.clone()
            } else {
                description.to_string()
            }
        }
    }
}

/// The containing directory of `path`, as a display string (falls back to
/// `path` itself if it has no parent, e.g. the filesystem root) —
/// duplicated from `window::parent_display` since that helper is private to
/// its module and this is the only other call site.
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

/// Formats a byte count with thousands separators, e.g. `2516582` ->
/// `"2,516,582"` — the exact-byte-count half of the "human size (exact
/// bytes)" General page display.
fn format_bytes_grouped(bytes: u64) -> String {
    let digits = bytes.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_grouped_adds_thousands_separators() {
        assert_eq!(format_bytes_grouped(2_516_582), "2,516,582");
        assert_eq!(format_bytes_grouped(999), "999");
        assert_eq!(format_bytes_grouped(1000), "1,000");
        assert_eq!(format_bytes_grouped(0), "0");
    }

    #[test]
    fn parent_display_falls_back_to_self_at_root() {
        assert_eq!(parent_display(&VeyraPath::from_local("/")), "/");
    }

    #[test]
    fn parent_display_local_path() {
        assert_eq!(
            parent_display(&VeyraPath::from_local("/home/user/notes.txt")),
            "/home/user"
        );
    }
}
