//! Modal name-collision dialog shown when a Copy/Move destination already
//! exists. Built on `AdwAlertDialog::choose`, whose callback fires once the
//! user answers — the background operation thread is meanwhile blocked on
//! `answer_rx.recv_blocking()` (see `crate::operations`), so this dialog
//! being modal doesn't stall anything but that one worker thread.

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use veyra_filesystem::{Conflict, ConflictDecision, VeyraPath};

/// Shows the conflict dialog for `conflict`, calling `on_decision` exactly
/// once with the user's answer (`Cancel` if the dialog is dismissed without
/// picking a response).
pub(crate) fn show(
    parent: &impl IsA<gtk4::Widget>,
    conflict: &Conflict,
    on_decision: impl FnOnce(ConflictDecision) + 'static,
) {
    let source_name = conflict.source.file_name().unwrap_or_default();
    let dest_name = conflict
        .destination
        .file_name()
        .unwrap_or_else(|| conflict.destination.to_string());

    let dialog = adw::AlertDialog::builder()
        .heading("File Already Exists")
        .body(format!(
            "\"{dest_name}\" already exists in this location.\n\n{}",
            compare_text(conflict)
        ))
        .build();

    let suggested_name = veyra_filesystem::suggest_name(&source_name, |candidate| {
        sibling_exists(&conflict.destination, candidate)
    });

    let rename_entry = gtk4::Entry::new();
    rename_entry.set_text(&suggested_name);
    rename_entry.set_tooltip_text(Some("New name to save as instead"));

    let apply_to_all =
        gtk4::CheckButton::with_label("Apply this choice to all remaining conflicts");
    apply_to_all.set_margin_top(6);

    let extra = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    extra.append(&rename_entry);
    extra.append(&apply_to_all);
    dialog.set_extra_child(Some(&extra));

    dialog.add_responses(&[
        ("skip", "Skip"),
        ("rename", "Rename"),
        ("replace", "Replace"),
    ]);
    dialog.set_response_appearance("replace", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("rename"));
    dialog.set_close_response("skip");

    dialog.choose(parent, gtk4::gio::Cancellable::NONE, move |response| {
        let blanket = apply_to_all.is_active();
        let decision = match response.as_str() {
            "skip" if blanket => ConflictDecision::SkipAll,
            "skip" => ConflictDecision::Skip,
            "replace" if blanket => ConflictDecision::ReplaceAll,
            "replace" => ConflictDecision::Replace,
            "rename" => {
                let text = rename_entry.text().to_string();
                let name = if text.trim().is_empty() {
                    suggested_name
                } else {
                    text
                };
                ConflictDecision::Rename(name)
            }
            _ => ConflictDecision::Cancel,
        };
        on_decision(decision);
    });
}

/// Best-effort size/modified-time comparison between the two files, so the
/// user can tell them apart without a separate "Compare" step. Local stats
/// only take microseconds, so this tiny synchronous query on the GTK main
/// thread (inside an already-modal dialog) is an acceptable trade for not
/// building a second async round trip just to fill in two lines of text.
fn compare_text(conflict: &Conflict) -> String {
    let existing = describe(&conflict.destination);
    let incoming = describe(&conflict.source);
    format!("Existing: {existing}\nIncoming: {incoming}")
}

fn describe(path: &VeyraPath) -> String {
    let Ok(info) = path.to_gio_file().query_info(
        "standard::size,time::modified",
        gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
        gio::Cancellable::NONE,
    ) else {
        return "unknown".to_string();
    };

    let size = veyra_filesystem::format_size(info.size().max(0) as u64);
    match info.modification_date_time() {
        Some(modified) => format!(
            "{size}, modified {}",
            modified.format("%Y-%m-%d %H:%M").unwrap_or_default()
        ),
        None => size,
    }
}

fn sibling_exists(destination: &VeyraPath, candidate: &str) -> bool {
    match destination {
        VeyraPath::Local(path) => path
            .parent()
            .map(|parent| parent.join(candidate))
            .is_some_and(|candidate_path| candidate_path.exists()),
        VeyraPath::Uri(uri) => {
            let parent = uri.rsplit_once('/').map(|(p, _)| p).unwrap_or(uri);
            gio::File::for_uri(&format!("{parent}/{candidate}"))
                .query_exists(gio::Cancellable::NONE)
        }
    }
}
