//! Veyra GTK4 + Libadwaita user interface layer.
//!
//! Faz 3 provides a usable file manager shell: sidebar (Places/Devices),
//! header bar (navigation, breadcrumbs, search, view switcher), status bar,
//! and three item views (Icon, Compact, Details) backed by an
//! `AdwNavigationSplitView`. All filesystem I/O runs off the GTK main
//! thread (`fs_async`), per Rule #14.

#![forbid(unsafe_code)]

mod archive_ops;
mod bookmarks;
mod breadcrumbs;
mod command_palette;
mod config;
mod context_menu;
mod dev_tools;
mod devices;
mod dialogs;
mod dnd;
mod error_ux;
mod file_associations;
mod fs_async;
mod headerbar;
mod history;
mod i18n;
mod network;
mod open_with;
mod operations;
mod preview;
mod privileged;
mod recent;
mod search_results;
mod session;
mod shortcuts;
mod sidebar;
mod sorting;
mod split_view;
mod state;
mod statusbar;
mod system_integration;
mod tab_page;
mod tags;
mod terminal;
mod thumbnails;
mod trash;
mod undo;
mod views;
mod widgets;
mod window;

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use libadwaita::prelude::*;
use libadwaita::Application;

use veyra_filesystem::VeyraPath;

/// The D-Bus application ID Veyra registers under — must match `Icon=`/
/// `Exec=`'s counterpart in `data/io.github.erayq1.Veyra.desktop` and the
/// desktop file's own basename, since GIO resolves "am I the default file
/// manager" (`system_integration::is_default_file_manager`) and "set myself
/// as default" by looking up `"{APP_ID}.desktop"`.
pub const APP_ID: &str = "io.github.erayq1.Veyra";

/// The linked GTK4 and Libadwaita library versions, as `"major.minor.micro"`
/// strings. Faz 53's crash reporter (`veyra-app`'s `panic_hook`) calls this
/// to record what Veyra crashed against without pulling `gtk4`/`libadwaita`
/// in as its own direct dependency — these are plain version accessors, safe
/// to call at any time, including before `run()`'s `Application` exists.
pub fn runtime_library_versions() -> (String, String) {
    let gtk = format!(
        "{}.{}.{}",
        gtk4::major_version(),
        gtk4::minor_version(),
        gtk4::micro_version()
    );
    let adwaita = format!(
        "{}.{}.{}",
        libadwaita::major_version(),
        libadwaita::minor_version(),
        libadwaita::micro_version()
    );
    (gtk, adwaita)
}

/// Builds and runs the Veyra GTK application. Every open/activate/CLI
/// argument for the process's entire lifetime (including re-invocations of
/// `veyra ...` from a second terminal, which GIO transparently forwards to
/// this already-running primary instance over D-Bus — see Faz 44) funnels
/// through the single `command-line` handler below, so there is exactly one
/// place that decides whether an argument opens a tab in an existing
/// window, forces a new window, or opens Preferences.
///
/// `cache_dir` is where the Faz 9 search index database lives
/// (`<cache_dir>/search_index.db`). `state_dir` is where Faz 53's crash
/// reports live (`<state_dir>/crashes/`) — the very first window built in
/// this process checks it once for a report left by a previous session's
/// panic and, if one exists, shows the crash dialog over that window.
///
/// Blocks the calling thread until the GTK main loop exits. Must be called on
/// the same thread the process started on (GTK main thread requirement).
pub fn run(app_id: &str, cache_dir: &Path, state_dir: &Path) -> glib::ExitCode {
    // Faz 45: diagnostic only — see `system_integration::is_flatpak_sandbox`'s
    // doc comment for why nothing below branches on this.
    tracing::info!(
        sandboxed = system_integration::is_flatpak_sandbox(),
        "sandbox environment detected"
    );

    let app = Application::builder()
        .application_id(app_id)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    let default_icon_name = app_id.to_string();
    let cache_dir = cache_dir.to_path_buf();
    let state_dir = state_dir.to_path_buf();
    // Faz 44: every window built in this process shares one `settings`
    // (loaded once, on the first window) rather than each window re-reading
    // `settings.json` independently — a second window opened via
    // "Open in New Window"/`--new-window` should reflect live Preferences
    // changes made in the first, not a stale on-disk snapshot from whenever
    // it happened to start.
    let settings: Rc<RefCell<Option<config::SharedSettings>>> = Rc::new(RefCell::new(None));
    let windows: Rc<RefCell<Vec<libadwaita::ApplicationWindow>>> =
        Rc::new(RefCell::new(Vec::new()));

    app.connect_command_line(move |app, cmdline| {
        let argv = cmdline.arguments();
        let args: Vec<String> = argv
            .iter()
            .skip(1)
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        let parsed = system_integration::parse_args(&args);
        let is_first_window = windows.borrow().is_empty();

        if parsed.new_window || is_first_window {
            let start_dir = parsed
                .paths
                .first()
                .map(|p| VeyraPath::parse(p))
                .unwrap_or_else(|| VeyraPath::from_local(glib::home_dir()));
            let window = build_window(app, start_dir, &cache_dir, &default_icon_name, &settings);
            {
                let windows = windows.clone();
                window.connect_close_request(move |closed| {
                    windows.borrow_mut().retain(|w| w != closed);
                    glib::Propagation::Proceed
                });
            }
            windows.borrow_mut().push(window);
        }

        let target = windows
            .borrow()
            .last()
            .cloned()
            .expect("a window always exists past the block above");

        // A fresh `--new-window` already opened its one path as the
        // window's own `start_dir`; anything beyond the first path (or
        // every path, for a plain `veyra /dir1 /dir2` with no
        // `--new-window`) still needs to land as its own tab.
        let remaining_paths: &[String] = if parsed.new_window {
            parsed.paths.get(1..).unwrap_or(&[])
        } else {
            &parsed.paths
        };
        for path in remaining_paths {
            gtk4::prelude::ActionGroupExt::activate_action(
                &target,
                "win.open-external-path",
                Some(&path.to_variant()),
            );
        }
        if parsed.preferences {
            gtk4::prelude::ActionGroupExt::activate_action(&target, "win.show-preferences", None);
        }

        target.present();
        if is_first_window {
            check_crash_report(&target, &state_dir);
        }
        0
    });

    app.run()
}

/// Faz 53: checks `state_dir/crashes/` once, on the very first window this
/// process ever builds, for a report a previous session's panic left
/// behind — and if one exists, shows the crash dialog over `window`.
/// Silently does nothing if the directory doesn't exist (no prior crash)
/// or the newest report can't be read back (already deleted, permissions
/// changed underneath us, etc.) — never blocks startup on this.
fn check_crash_report(window: &libadwaita::ApplicationWindow, state_dir: &Path) {
    let crashes_dir = state_dir.join("crashes");
    match veyra_core::crash_report::latest_report(&crashes_dir) {
        Ok(Some(path)) => match std::fs::read_to_string(&path) {
            Ok(text) => dialogs::crash_dialog::show(window, text, path),
            Err(err) => {
                tracing::warn!(error = %err, path = %path.display(), "failed to read crash report")
            }
        },
        Ok(None) => {}
        Err(err) => tracing::warn!(error = %err, "failed to check for a crash report"),
    }
}

/// Builds one new top-level window: on its very first call, loads and
/// caches `settings` for the whole process; every later call (a second
/// window in this same process) reuses that shared handle instead of
/// re-reading `settings.json`.
fn build_window(
    app: &Application,
    start_dir: VeyraPath,
    cache_dir: &Path,
    default_icon_name: &str,
    settings: &Rc<RefCell<Option<config::SharedSettings>>>,
) -> libadwaita::ApplicationWindow {
    tracing::info!("building window");

    let shared_settings = {
        let mut slot = settings.borrow_mut();
        if let Some(existing) = slot.as_ref() {
            existing.clone()
        } else {
            // Faz 34: `VeyraSettings::load()` is the single source of truth
            // for every user preference from here on — the theme applies
            // before the window is even built, and the loaded value (rather
            // than a hardcoded `ColorScheme::Default`) is what a corrupt/
            // missing `settings.json` falls back to via
            // `VeyraSettings::default()`.
            let loaded = config::VeyraSettings::load();
            veyra_core::security::set_sanitize_log_paths(loaded.sanitize_log_paths);
            veyra_core::crash_report::set_crash_reports_enabled(loaded.save_crash_reports);
            i18n::set_locale(loaded.language.resolve());
            libadwaita::StyleManager::default().set_color_scheme(loaded.color_scheme.to_adw());
            config::apply_accent_color(loaded.accent_color);
            config::set_date_format(loaded.date_format);
            config::set_size_unit(loaded.size_unit);
            terminal::set_terminal_pref(loaded.terminal_pref, &loaded.custom_terminal_command);
            veyra_filesystem::set_reflink_enabled(loaded.enable_reflink);
            veyra_filesystem::set_compression_level(loaded.compression_level.level());
            let shared: config::SharedSettings = Rc::new(RefCell::new(loaded));
            *slot = Some(shared.clone());
            shared
        }
    };

    if let Some(display) = gtk4::gdk::Display::default() {
        let icon_theme = gtk4::IconTheme::for_display(&display);
        icon_theme.add_search_path("data/icons");
    }
    gtk4::Window::set_default_icon_name(default_icon_name);

    window::build_window(app, start_dir, cache_dir, shared_settings)
}
