//! Rename prompt for the context menu's Rename entry / F2 shortcut. An
//! `AdwAlertDialog` with an `Entry` extra child, matching the pattern
//! `conflict_dialog` already uses for its rename field.

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

/// Shows the rename dialog pre-filled with `current_name` (name stem
/// pre-selected, matching common file-manager UX), calling `on_confirm`
/// with the new name only if the user picks "Rename" with a non-empty,
/// changed value.
pub(crate) fn show(
    parent: &impl IsA<gtk4::Widget>,
    current_name: &str,
    on_confirm: impl FnOnce(String) + 'static,
) {
    let dialog = adw::AlertDialog::builder().heading("Rename").build();

    let entry = gtk4::Entry::new();
    entry.set_text(current_name);
    entry.set_activates_default(true);
    entry.update_property(&[gtk4::accessible::Property::Label("New Name")]);
    // `select_region` takes character offsets, not byte offsets, so the
    // split found via a byte-index `rfind` must be re-measured in `chars()`
    // to land correctly on multi-byte (e.g. Unicode) names.
    let stem_chars = match current_name.rfind('.') {
        Some(0) | None => current_name.chars().count(),
        Some(byte_index) => current_name[..byte_index].chars().count(),
    };
    entry.select_region(0, stem_chars as i32);
    dialog.set_extra_child(Some(&entry));

    dialog.add_responses(&[("cancel", "Cancel"), ("rename", "Rename")]);
    dialog.set_default_response(Some("rename"));
    dialog.set_close_response("cancel");
    dialog.set_response_enabled("rename", !current_name.trim().is_empty());

    {
        let dialog = dialog.clone();
        entry.connect_changed(move |entry| {
            dialog.set_response_enabled("rename", !entry.text().trim().is_empty());
        });
    }

    let entry_for_response = entry.clone();
    dialog.choose(parent, gtk4::gio::Cancellable::NONE, move |response| {
        if response == "rename" {
            on_confirm(entry_for_response.text().trim().to_string());
        }
    });
}
