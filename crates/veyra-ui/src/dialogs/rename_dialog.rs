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
    warn_bidi: bool,
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

    // Faz 59: Unicode bidi-override spoofing guard (Rule #21/#23-adjacent
    // security hardening) — a name containing e.g. U+202E (RIGHT-TO-LEFT
    // OVERRIDE) can visually reverse its own extension (`"evil\u{202E}gpj.exe"`
    // renders as if it ends in `.exe.jpg`... reversed the other way). This is
    // advisory in `veyra_core::security::has_bidi_override` itself (some RTL
    // filenames legitimately use nearby codepoints), so the UI surfaces a
    // visible warning and blocks confirmation rather than silently stripping
    // anything — the user can still see exactly what they typed.
    let warning_label = gtk4::Label::new(None);
    warning_label.set_wrap(true);
    warning_label.set_xalign(0.0);
    warning_label.add_css_class("error");
    warning_label.add_css_class("caption");
    warning_label.set_visible(false);
    warning_label.set_label(
        "This name contains invisible text-direction characters, which can be \
         used to disguise a file's real extension. Remove them to continue.",
    );

    let extra = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    extra.append(&entry);
    extra.append(&warning_label);
    dialog.set_extra_child(Some(&extra));

    dialog.add_responses(&[("cancel", "Cancel"), ("rename", "Rename")]);
    dialog.set_default_response(Some("rename"));
    dialog.set_close_response("cancel");
    dialog.set_response_enabled("rename", !current_name.trim().is_empty());

    {
        let dialog = dialog.clone();
        let warning_label = warning_label.clone();
        entry.connect_changed(move |entry| {
            let text = entry.text();
            // Faz 65: "Warn on BiDi Spoofing" preference — off skips the
            // check entirely rather than just hiding the label, so a user
            // who's deliberately turned this off (e.g. legitimate RTL
            // filenames tripping the advisory heuristic) isn't also blocked
            // from confirming.
            let has_bidi = warn_bidi && veyra_core::security::has_bidi_override(&text);
            warning_label.set_visible(has_bidi);
            dialog.set_response_enabled("rename", !text.trim().is_empty() && !has_bidi);
        });
    }

    let entry_for_response = entry.clone();
    dialog.choose(parent, gtk4::gio::Cancellable::NONE, move |response| {
        if response == "rename" {
            on_confirm(entry_for_response.text().trim().to_string());
        }
    });
}
