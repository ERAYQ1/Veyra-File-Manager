//! Faz 44: Linux desktop system integration — default-file-manager status,
//! CLI argument parsing for the single-instance `GApplication` command-line
//! entry point (`lib.rs`), and desktop notifications for long-running
//! background operations.

use gio::prelude::*;
use libadwaita as adw;

use crate::APP_ID;

const DIRECTORY_MIME_TYPE: &str = "inode/directory";
const URI_SCHEME_HANDLER_MIME_TYPE: &str = "x-scheme-handler/file";

/// True when Veyra is currently registered as the system's default handler
/// for `inode/directory` — the standard "default file manager" signal every
/// desktop environment (GNOME Settings, KDE System Settings, `xdg-mime
/// query default`) reads.
pub(crate) fn is_default_file_manager() -> bool {
    let desktop_id = format!("{APP_ID}.desktop");
    gio::AppInfo::default_for_type(DIRECTORY_MIME_TYPE, false)
        .and_then(|info| info.id())
        .is_some_and(|id| id == desktop_id)
}

/// Registers Veyra as the default handler for `inode/directory` (opening
/// folders) and, best-effort, `x-scheme-handler/file`. Goes through GIO's
/// `mimeapps.list` machinery, which only ever touches the invoking user's
/// own `~/.config/mimeapps.list` — no root or Polkit needed (Kural #20).
pub(crate) fn set_as_default_file_manager() -> Result<(), glib::Error> {
    let desktop_id = format!("{APP_ID}.desktop");
    let info = gio::DesktopAppInfo::new(&desktop_id).ok_or_else(|| {
        glib::Error::new(
            gio::IOErrorEnum::NotFound,
            "Veyra's .desktop entry isn't installed/registered with GIO",
        )
    })?;
    info.set_as_default_for_type(DIRECTORY_MIME_TYPE)?;
    if let Err(err) = info.set_as_default_for_type(URI_SCHEME_HANDLER_MIME_TYPE) {
        tracing::warn!(error = %err, "failed to register Veyra as x-scheme-handler/file default");
    }
    Ok(())
}

/// Sends a desktop notification for a finished background operation, but
/// only when `window` doesn't currently have input focus — a focused window
/// already shows completion via its progress toast/status bar, so a
/// notification there would just be redundant noise.
pub(crate) fn notify_operation_complete(window: &adw::ApplicationWindow, title: &str, body: &str) {
    use gtk4::prelude::GtkWindowExt;
    if window.is_active() {
        return;
    }
    let Some(app) = window.application() else {
        return;
    };
    let notification = gio::Notification::new(title);
    notification.set_body(Some(body));
    notification.set_priority(gio::NotificationPriority::Normal);
    app.send_notification(Some("operation-complete"), &notification);
}

/// The result of parsing a `GApplication` `command-line` invocation's
/// argument vector (argv[0], the program name, already excluded by the
/// caller). Kept as a separate, GTK-free struct so parsing itself is unit
/// testable without a running `GApplication`/display.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ParsedArgs {
    /// `--new-window`: force building a genuinely new window (used by
    /// `window.rs`'s "Open in New Window" context-menu action) rather than
    /// attaching to whichever window is already running.
    pub new_window: bool,
    /// `--preferences`: open the Preferences dialog on the target window.
    pub preferences: bool,
    /// Every remaining bare argument, in order — directories to open as new
    /// tabs, or a single file to reveal-and-select in its parent directory.
    pub paths: Vec<String>,
}

/// Parses a raw argv slice (already excluding argv[0]) into `ParsedArgs`.
/// Unknown flags are ignored rather than rejected (Kural #15 — a stray/
/// unrecognized argument must never abort startup); anything not starting
/// with `--` is treated as a path.
pub(crate) fn parse_args<S: AsRef<str>>(args: &[S]) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    for arg in args {
        match arg.as_ref() {
            "--new-window" => parsed.new_window = true,
            "--preferences" => parsed.preferences = true,
            other if !other.starts_with("--") => parsed.paths.push(other.to_string()),
            _ => {}
        }
    }
    parsed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_paths() {
        let parsed = parse_args(&["/home/user/dir1", "/home/user/dir2"]);
        assert!(!parsed.new_window);
        assert!(!parsed.preferences);
        assert_eq!(parsed.paths, vec!["/home/user/dir1", "/home/user/dir2"]);
    }

    #[test]
    fn parses_preferences_flag_alone() {
        let parsed = parse_args(&["--preferences"]);
        assert!(parsed.preferences);
        assert!(parsed.paths.is_empty());
    }

    #[test]
    fn parses_new_window_flag_with_a_path() {
        let parsed = parse_args(&["--new-window", "/tmp/some/dir"]);
        assert!(parsed.new_window);
        assert_eq!(parsed.paths, vec!["/tmp/some/dir"]);
    }

    #[test]
    fn empty_argv_yields_empty_defaults() {
        let parsed = parse_args::<&str>(&[]);
        assert_eq!(parsed, ParsedArgs::default());
    }

    #[test]
    fn unknown_flags_are_ignored_not_treated_as_paths() {
        let parsed = parse_args(&["--unknown-flag", "/real/path"]);
        assert_eq!(parsed.paths, vec!["/real/path"]);
    }

    /// Faz 44 requirement A: the shipped `.desktop` file declares every
    /// piece of integration this module and `lib.rs` rely on actually being
    /// wired up — a mismatch here (e.g. a renamed CLI flag) would silently
    /// break "Open in New Window"/"Preferences" from a desktop/dock
    /// right-click without any compile-time signal.
    const DESKTOP_FILE: &str = include_str!("../../../data/io.github.erayq1.Veyra.desktop");

    #[test]
    fn desktop_file_declares_directory_and_file_scheme_mime_types() {
        assert!(DESKTOP_FILE.contains("MimeType=inode/directory;x-scheme-handler/file;"));
    }

    #[test]
    fn desktop_file_declares_new_window_and_preferences_actions() {
        assert!(DESKTOP_FILE.contains("Actions=NewWindow;Preferences;"));
        assert!(DESKTOP_FILE.contains("[Desktop Action NewWindow]"));
        assert!(DESKTOP_FILE.contains("Exec=veyra --new-window %f"));
        assert!(DESKTOP_FILE.contains("[Desktop Action Preferences]"));
        assert!(DESKTOP_FILE.contains("Exec=veyra --preferences"));
    }

    #[test]
    fn desktop_file_opts_into_notifications() {
        assert!(DESKTOP_FILE.contains("X-GNOME-UsesNotifications=true"));
    }
}
