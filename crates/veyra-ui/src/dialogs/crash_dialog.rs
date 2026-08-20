//! Faz 53: shown once at startup (`lib.rs::check_crash_report`) when a
//! previous session's panic left a report on disk. Lets the user read
//! exactly what would be shared before sharing anything — Veyra itself
//! never uploads a byte of it (Kural #24); every action here is something
//! the user explicitly triggers.

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::i18n::t;

/// Where `[Copy Report & Open GitHub Issues]` sends the user — a plain
/// "new issue" URL, no report contents baked in (the report is copied to
/// the clipboard separately, so nothing is transmitted by opening this).
const NEW_ISSUE_URL: &str = "https://github.com/ERAYQ1/Veyra-File-Manager/issues/new";

/// Shows the crash-detected dialog over `window`. `report_text` is the
/// already-sanitized report body read back from disk; `report_path` is
/// only ever used by the "Dismiss & Delete" response — every other
/// response (including closing the dialog) leaves the file in place, so a
/// user who dismisses by accident (e.g. Escape) doesn't lose the report
/// before deciding what to do with it.
pub(crate) fn show(
    window: &adw::ApplicationWindow,
    report_text: String,
    report_path: std::path::PathBuf,
) {
    let dialog = adw::AlertDialog::builder()
        .heading(t("crash.title"))
        .body(t("crash.description"))
        .build();

    let content = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .build();

    let expander = adw::ExpanderRow::builder()
        .title(t("crash.show_report"))
        .build();
    expander.add_css_class("card");

    let report_label = gtk4::Label::builder()
        .label(&report_text)
        .wrap(true)
        .xalign(0.0)
        .selectable(true)
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    report_label.add_css_class("monospace");
    report_label.add_css_class("dim-label");
    expander.add_row(&report_label);

    let details_list = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .build();
    details_list.add_css_class("boxed-list");
    details_list.append(&expander);
    content.append(&details_list);
    dialog.set_extra_child(Some(&content));

    dialog.add_response("dismiss", t("crash.action.dismiss"));
    dialog.add_response("save", t("crash.action.save"));
    dialog.add_response("copy", t("crash.action.copy_and_report"));
    dialog.set_response_appearance("copy", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("copy"));
    // Escape/window-close reads as "Dismiss & Delete" too, matching how
    // this dialog's third button already behaves — there's no neutral
    // "just close" response to fall back to since every `add_response`
    // becomes a visible button.
    dialog.set_close_response("dismiss");

    let window_for_response = window.clone();
    dialog.connect_response(None, move |_, response| match response {
        "copy" => {
            window_for_response.clipboard().set_text(&report_text);
            launch_new_issue(&window_for_response);
        }
        "save" => save_to_file(&window_for_response, report_text.clone()),
        _ => {
            if let Err(err) = std::fs::remove_file(&report_path) {
                tracing::warn!(error = %err, "failed to delete crash report");
            }
        }
    });

    dialog.present(Some(window));
}

fn launch_new_issue(window: &adw::ApplicationWindow) {
    gtk4::UriLauncher::new(NEW_ISSUE_URL).launch(
        Some(window),
        gtk4::gio::Cancellable::NONE,
        |result| {
            if let Err(err) = result {
                tracing::warn!(error = %err, "failed to open the new-issue page");
            }
        },
    );
}

fn save_to_file(window: &adw::ApplicationWindow, report_text: String) {
    let file_dialog = gtk4::FileDialog::builder()
        .title(t("crash.action.save"))
        .initial_name("veyra-crash-report.txt")
        .build();
    file_dialog.save(Some(window), gtk4::gio::Cancellable::NONE, move |result| {
        let Ok(file) = result else {
            return;
        };
        let Some(path) = file.path() else {
            return;
        };
        if let Err(err) = std::fs::write(&path, &report_text) {
            tracing::warn!(error = %err, "failed to save crash report to file");
        }
    });
}
