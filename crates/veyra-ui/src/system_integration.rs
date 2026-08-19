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

/// True when this process is running inside a Flatpak sandbox. Checks the
/// same two signals `flatpak-builder`-produced apps and GLib itself agree
/// on: the `/.flatpak-info` file every Flatpak sandbox bind-mounts in (the
/// authoritative source — see `flatpak(5)`), falling back to the
/// `FLATPAK_ID` environment variable Flatpak also sets, in case a future
/// sandbox variant ever omits the file. Cheap enough (one `Path::exists`,
/// one env lookup) to call on every launch decision rather than caching.
///
/// Faz 45: nothing in `veyra-ui` branches its *behavior* on this today —
/// `gio::AppInfo::launch` (`open_with.rs`), `gio::Notification`
/// (`notify_operation_complete` above), and `gio::AppInfo::default_for_type`/
/// `set_as_default_for_type` (`is_default_file_manager`/
/// `set_as_default_file_manager` above) already detect the sandbox
/// themselves at the GLib level and transparently redirect through the
/// matching XDG portal (`OpenURI`, `Notification`, the default-apps
/// `mimeapps.list` proxy) instead of touching the host directly — see
/// `docs/flatpak_permissions.md`. This function exists for diagnostics
/// (logged once at startup) and for future code that genuinely needs to
/// know, rather than to gate any of the above.
pub(crate) fn is_flatpak_sandbox() -> bool {
    is_sandboxed(
        std::path::Path::new("/.flatpak-info").exists(),
        std::env::var_os("FLATPAK_ID").is_some(),
    )
}

/// The actual (env-free, filesystem-free) decision `is_flatpak_sandbox`
/// makes — split out so it's unit testable without mutating real process
/// state (racy across parallel `cargo test` threads).
fn is_sandboxed(flatpak_info_exists: bool, flatpak_id_set: bool) -> bool {
    flatpak_info_exists || flatpak_id_set
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

    #[test]
    fn sandbox_detected_via_flatpak_info_file_alone() {
        assert!(is_sandboxed(true, false));
    }

    #[test]
    fn sandbox_detected_via_flatpak_id_env_alone() {
        assert!(is_sandboxed(false, true));
    }

    #[test]
    fn not_sandboxed_when_neither_signal_present() {
        assert!(!is_sandboxed(false, false));
    }

    /// Faz 45 requirement A: the Flatpak manifest declares exactly the
    /// minimal permission set `docs/flatpak_permissions.md` justifies — a
    /// drift here (an added/removed `finish-args` entry with no matching
    /// doc/manifest update) would silently break the "every permission is
    /// documented" guarantee that document promises the reader.
    const MANIFEST: &str = include_str!("../../../build-aux/flatpak/io.github.erayq1.Veyra.json");

    #[test]
    fn manifest_is_valid_json_with_the_expected_app_id_and_runtime() {
        let value: serde_json::Value =
            serde_json::from_str(MANIFEST).expect("manifest must be valid JSON");
        assert_eq!(value["id"], "io.github.erayq1.Veyra");
        assert_eq!(value["runtime"], "org.gnome.Platform");
        assert_eq!(value["sdk"], "org.gnome.Sdk");
        assert_eq!(value["command"], "veyra");
    }

    #[test]
    fn manifest_finish_args_match_the_documented_minimal_set() {
        let value: serde_json::Value = serde_json::from_str(MANIFEST).unwrap();
        let finish_args: Vec<&str> = value["finish-args"]
            .as_array()
            .expect("finish-args must be an array")
            .iter()
            .map(|v| v.as_str().expect("finish-args entries must be strings"))
            .collect();
        for expected in [
            "--share=ipc",
            "--socket=fallback-x11",
            "--socket=wayland",
            "--filesystem=host",
            "--talk-name=org.freedesktop.FileManager1",
            "--talk-name=org.freedesktop.Notifications",
            "--talk-name=org.gtk.vfs.*",
            "--system-talk-name=org.freedesktop.UDisks2",
        ] {
            assert!(
                finish_args.contains(&expected),
                "manifest is missing finish-args entry {expected:?}"
            );
        }
        // No permission beyond the documented, justified set (Kural #20's
        // "keep sandbox permissions minimal" spirit) — every entry present
        // must be one of the eight `docs/flatpak_permissions.md` covers.
        assert_eq!(finish_args.len(), 8, "unexpected extra finish-args entry");
    }

    #[test]
    fn manifest_references_the_committed_cargo_sources_file() {
        let value: serde_json::Value = serde_json::from_str(MANIFEST).unwrap();
        let sources = value["modules"][0]["sources"]
            .as_array()
            .expect("module must declare sources");
        assert!(sources.iter().any(|s| s == "cargo-sources.json"));
    }

    /// Real cargo-sources.json, generated by `flatpak-cargo-generator.py`
    /// against `Cargo.lock` (see `build-aux/flatpak/README.md`) — not a
    /// placeholder, so it's worth checking it's still well-formed and
    /// non-empty rather than just present.
    const CARGO_SOURCES: &str = include_str!("../../../build-aux/flatpak/cargo-sources.json");

    #[test]
    fn cargo_sources_is_valid_json_and_non_empty() {
        let value: serde_json::Value =
            serde_json::from_str(CARGO_SOURCES).expect("cargo-sources.json must be valid JSON");
        let sources = value
            .as_array()
            .expect("cargo-sources.json must be an array");
        assert!(!sources.is_empty());
    }
}
