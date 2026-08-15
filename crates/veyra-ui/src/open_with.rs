//! Faz 22: Open With engine — content-type-based application discovery and
//! launching via `gio::AppInfo`. Never shells out (Rule #19): app discovery
//! reads the desktop-file cache GIO already maintains in memory, and
//! launching goes through `GAppInfo::launch`, which is a fork + D-Bus
//! activation rather than filesystem I/O — fast enough to run directly on
//! the GTK main thread (Rule #11 targets bulk file operations, not this;
//! `window.rs`'s previous `AppChooserDialog` placeholder made the same
//! call).

use gtk4::prelude::*;
use gtk4::{gio, glib};

/// Applications GIO recommends for `content_type`, most-relevant first
/// (the system default, when set, is included and typically leads).
pub(crate) fn recommended_apps(content_type: &str) -> Vec<gio::AppInfo> {
    gio::AppInfo::all_for_type(content_type)
}

/// The system default application for `content_type`, if one is set.
pub(crate) fn default_app(content_type: &str) -> Option<gio::AppInfo> {
    gio::AppInfo::default_for_type(content_type, false)
}

/// Every desktop application registered on the system, hidden/`NoDisplay`
/// entries excluded, sorted by display name for the dialog's "All
/// Applications" section.
pub(crate) fn all_apps() -> Vec<gio::AppInfo> {
    let mut apps: Vec<gio::AppInfo> = gio::AppInfo::all()
        .into_iter()
        .filter(gio::AppInfo::should_show)
        .collect();
    apps.sort_by_key(|a| a.name().to_lowercase());
    apps
}

/// Case-insensitive substring match against an application's name and
/// description, for the Open With dialog's live search filter.
pub(crate) fn matches_query(name: &str, description: &str, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    name.to_lowercase().contains(&query) || description.to_lowercase().contains(&query)
}

/// Launches `app` on `file`. `window` supplies the display the launched
/// application starts on (workspace/startup-notification context) — any
/// realized widget works, since `GtkWidget::display` doesn't require the
/// widget to be mapped.
pub(crate) fn launch(
    app: &gio::AppInfo,
    file: &gio::File,
    window: &impl IsA<gtk4::Widget>,
) -> Result<(), glib::Error> {
    let context = window.display().app_launch_context();
    app.launch(std::slice::from_ref(file), Some(&context))
}

/// Marks `app` as the default handler for `content_type`, updating the
/// user's XDG `mimeapps.list` default-apps association via GIO.
pub(crate) fn set_default(app: &gio::AppInfo, content_type: &str) -> Result<(), glib::Error> {
    app.set_as_default_for_type(content_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_matches_everything() {
        assert!(matches_query("GIMP", "Image editor", ""));
        assert!(matches_query("GIMP", "Image editor", "   "));
    }

    #[test]
    fn matches_name_case_insensitively() {
        assert!(matches_query("GIMP Image Editor", "", "gimp"));
        assert!(matches_query("GIMP Image Editor", "", "IMAGE"));
    }

    #[test]
    fn matches_description_case_insensitively() {
        assert!(matches_query("Files", "Browse and manage files", "browse"));
    }

    #[test]
    fn rejects_non_matching_query() {
        assert!(!matches_query("GIMP", "Image editor", "video"));
    }
}
