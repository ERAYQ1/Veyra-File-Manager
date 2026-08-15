//! Faz 19: "Compress…" prompt — archive file name plus a format picker
//! (ZIP/TAR/TAR.GZ/TAR.XZ/TAR.ZST/7z), matching the `AdwAlertDialog` +
//! extra-child pattern `rename_dialog` already uses. Changing the format
//! dropdown rewrites the name entry's extension live, so the two controls
//! never end up disagreeing about what gets written.

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use veyra_filesystem::ArchiveFormat;

/// Shows the compress dialog pre-filled with `default_stem` (no extension)
/// and `default_format`, calling `on_confirm` with the chosen archive file
/// name (extension included) and format only if the user picks "Compress"
/// with a non-empty name.
pub(crate) fn show(
    parent: &impl IsA<gtk4::Widget>,
    default_stem: &str,
    default_format: ArchiveFormat,
    on_confirm: impl FnOnce(String, ArchiveFormat) + 'static,
) {
    let dialog = adw::AlertDialog::builder().heading("Compress").build();

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);

    let name_entry = gtk4::Entry::new();
    name_entry.set_activates_default(true);
    content.append(&name_entry);

    let labels: Vec<&str> = ArchiveFormat::ALL.iter().map(|f| f.label()).collect();
    let format_dropdown = gtk4::DropDown::from_strings(&labels);
    let default_index = ArchiveFormat::ALL
        .iter()
        .position(|f| *f == default_format)
        .unwrap_or(0) as u32;
    format_dropdown.set_selected(default_index);
    content.append(&format_dropdown);

    dialog.set_extra_child(Some(&content));

    let set_name_for_format = {
        let name_entry = name_entry.clone();
        move |format: ArchiveFormat| {
            let stem = strip_known_extension(&name_entry.text());
            name_entry.set_text(&format!("{stem}{}", format.extension()));
        }
    };
    set_name_for_format(default_format);
    let stem_end = name_entry.text().len() as i32 - default_format.extension().len() as i32;
    name_entry.select_region(0, stem_end.max(0));

    dialog.add_responses(&[("cancel", "Cancel"), ("compress", "Compress")]);
    dialog.set_default_response(Some("compress"));
    dialog.set_close_response("cancel");
    dialog.set_response_enabled("compress", !default_stem.trim().is_empty());

    {
        let dialog = dialog.clone();
        name_entry.connect_changed(move |entry| {
            dialog.set_response_enabled("compress", !entry.text().trim().is_empty());
        });
    }
    format_dropdown.connect_selected_notify(move |dropdown| {
        let index = dropdown.selected() as usize;
        if let Some(format) = ArchiveFormat::ALL.get(index) {
            set_name_for_format(*format);
        }
    });

    let name_entry_for_response = name_entry.clone();
    let format_dropdown_for_response = format_dropdown.clone();
    dialog.choose(parent, gtk4::gio::Cancellable::NONE, move |response| {
        if response != "compress" {
            return;
        }
        let name = name_entry_for_response.text().trim().to_string();
        if name.is_empty() {
            return;
        }
        let format = ArchiveFormat::ALL
            .get(format_dropdown_for_response.selected() as usize)
            .copied()
            .unwrap_or(ArchiveFormat::Zip);
        on_confirm(name, format);
    });
}

/// Strips any of `ArchiveFormat`'s known extensions from `name`, so
/// switching the format dropdown doesn't pile up `archive.zip.tar.gz`.
fn strip_known_extension(name: &str) -> String {
    for format in ArchiveFormat::ALL {
        if let Some(stem) = name.strip_suffix(format.extension()) {
            return stem.to_string();
        }
    }
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_known_double_and_single_extensions() {
        assert_eq!(strip_known_extension("photos.tar.gz"), "photos");
        assert_eq!(strip_known_extension("photos.zip"), "photos");
        assert_eq!(strip_known_extension("photos.7z"), "photos");
    }

    #[test]
    fn leaves_unrelated_names_untouched() {
        assert_eq!(strip_known_extension("photos"), "photos");
        assert_eq!(
            strip_known_extension("photos.tar.gz.bak"),
            "photos.tar.gz.bak"
        );
    }
}
