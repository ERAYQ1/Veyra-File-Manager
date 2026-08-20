//! Faz 53: privacy-friendly crash reporting — model, redaction, and on-disk
//! rotation for the local-only report Veyra writes when it panics. Nothing
//! in this module ever touches the network (Kural #24): a report exists
//! only in `$XDG_STATE_HOME/veyra/crashes/` until the user explicitly
//! copies, saves, or deletes it through the crash dialog (`veyra-ui`'s
//! `dialogs::crash_dialog`). The user's home directory and any
//! secret-looking environment variable are stripped before a single byte
//! is written (Kural #23).

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};

use crate::security;

/// Process-wide flag backing Preferences' "Save Crash Reports Locally"
/// toggle (mirrors `security::SANITIZE_LOG_PATHS`'s pattern: a plain
/// `AtomicBool` since the panic hook that reads it runs outside any
/// `Rc<RefCell<Settings>>`, potentially on a panicking non-main thread).
/// Defaults to `true` — capturing a local-only, redacted report is the
/// privacy-safe default; nothing ever leaves the machine either way.
static SAVE_CRASH_REPORTS: AtomicBool = AtomicBool::new(true);

/// Whether the panic hook should write a crash report to disk right now.
pub fn crash_reports_enabled() -> bool {
    SAVE_CRASH_REPORTS.load(Ordering::Relaxed)
}

/// Enables or disables local crash-report capture, effective immediately
/// for the next panic on any thread.
pub fn set_crash_reports_enabled(enabled: bool) {
    SAVE_CRASH_REPORTS.store(enabled, Ordering::Relaxed);
}

/// Maximum crash reports kept on disk; [`CrashReport::write`] deletes the
/// oldest beyond this count.
pub const MAX_RETAINED_REPORTS: usize = 5;

/// Environment variable name fragments (case-insensitive) that mark a
/// variable as secret-like and worth stripping entirely from a report —
/// session tokens, API keys, and similar credentials must never end up on
/// disk even though the rest of the environment (`DESKTOP_SESSION`,
/// `LANG`, …) is genuinely useful for debugging a crash.
// Note: "SESSION" alone is deliberately excluded — it would flag ubiquitous
// benign desktop vars like `DESKTOP_SESSION` and `XDG_SESSION_TYPE` (useful
// diagnostic context in a crash report, not secrets). `SESSIONID` catches
// actual session-identifier secrets (`SESSIONID`, `PHP_SESSIONID`, ...)
// without that collision; `SESSION_TOKEN`/`SESSION_SECRET`-style keys are
// already caught by the `TOKEN`/`SECRET` fragments below.
const SENSITIVE_ENV_FRAGMENTS: [&str; 10] = [
    "TOKEN",
    "KEY",
    "SECRET",
    "AUTH",
    "PASS",
    "CREDENTIAL",
    "PRIVATE",
    "SIGNATURE",
    "COOKIE",
    "SESSIONID",
];

/// Whether `key` looks like it holds a secret, per
/// [`SENSITIVE_ENV_FRAGMENTS`].
pub fn is_sensitive_env_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    SENSITIVE_ENV_FRAGMENTS
        .iter()
        .any(|fragment| upper.contains(fragment))
}

/// Drops every sensitive-looking entry from `vars`, keeping the rest in
/// their original order.
pub fn filter_env_vars(vars: Vec<(String, String)>) -> Vec<(String, String)> {
    vars.into_iter()
        .filter(|(key, _)| !is_sensitive_env_key(key))
        .collect()
}

/// Replaces every occurrence of `home` in `text` with `~/[REDACTED]`,
/// leaving whatever came after it in that path untouched — e.g.
/// `/home/alice/Documents/report.pdf` becomes
/// `~/[REDACTED]/Documents/report.pdf`. This hides the username and home
/// location without needing to also strip the (potentially
/// crash-relevant) subdirectory structure a backtrace or panic message
/// mentions.
pub fn redact_home_path(text: &str, home: &Path) -> String {
    let home_str = home.to_string_lossy();
    if home_str.is_empty() {
        return text.to_string();
    }
    text.replace(home_str.as_ref(), "~/[REDACTED]")
}

/// A single captured panic. Every field is already sanitized by
/// [`CrashReport::capture`] before this type is ever constructed, so
/// there's no separate "sanitize before writing" step callers can forget.
#[derive(Debug, Clone, PartialEq)]
pub struct CrashReport {
    pub app_version: String,
    pub os_info: String,
    pub kernel_info: String,
    pub gtk_version: String,
    pub adwaita_version: String,
    pub message: String,
    pub location: String,
    pub backtrace: String,
    pub env_vars: Vec<(String, String)>,
    pub timestamp: DateTime<Utc>,
}

impl CrashReport {
    /// Builds a report from raw panic data. `home` redacts itself out of
    /// `message`/`location`/`backtrace`/every `raw_env_vars` value;
    /// `raw_env_vars` (typically `std::env::vars().collect()`) also has
    /// every secret-looking key dropped via [`filter_env_vars`].
    #[allow(clippy::too_many_arguments)]
    pub fn capture(
        home: &Path,
        app_version: impl Into<String>,
        os_info: impl Into<String>,
        kernel_info: impl Into<String>,
        gtk_version: impl Into<String>,
        adwaita_version: impl Into<String>,
        message: &str,
        location: &str,
        backtrace: &str,
        raw_env_vars: Vec<(String, String)>,
    ) -> Self {
        let env_vars = filter_env_vars(raw_env_vars)
            .into_iter()
            .map(|(key, value)| (key, redact_home_path(&value, home)))
            .collect();
        CrashReport {
            app_version: app_version.into(),
            os_info: os_info.into(),
            kernel_info: kernel_info.into(),
            gtk_version: gtk_version.into(),
            adwaita_version: adwaita_version.into(),
            message: redact_home_path(message, home),
            location: redact_home_path(location, home),
            backtrace: redact_home_path(backtrace, home),
            env_vars,
            timestamp: Utc::now(),
        }
    }

    /// Renders the report as the plain-text form shown in the crash
    /// dialog, saved to `.txt`, and copied to the clipboard for a GitHub
    /// issue.
    pub fn to_text(&self) -> String {
        let env_section = if self.env_vars.is_empty() {
            String::new()
        } else {
            let lines: Vec<String> = self
                .env_vars
                .iter()
                .map(|(key, value)| format!("  {key}={value}"))
                .collect();
            format!("\nEnvironment:\n{}\n", lines.join("\n"))
        };
        format!(
            "Veyra Crash Report\n\
             ===================\n\
             Timestamp: {timestamp}\n\
             Veyra Version: {app_version}\n\
             OS: {os_info}\n\
             Kernel: {kernel_info}\n\
             GTK: {gtk_version}\n\
             Libadwaita: {adwaita_version}\n\
             \n\
             Panic: {message}\n\
             Location: {location}\n\
             \n\
             Backtrace:\n{backtrace}\n\
             {env_section}\n\
             -- No information in this report has been sent anywhere. --\n",
            timestamp = self.timestamp.to_rfc3339(),
            app_version = self.app_version,
            os_info = self.os_info,
            kernel_info = self.kernel_info,
            gtk_version = self.gtk_version,
            adwaita_version = self.adwaita_version,
            message = self.message,
            location = self.location,
            backtrace = self.backtrace,
        )
    }

    /// Writes this report to `crashes_dir` (created if missing) as an
    /// owner-only-readable timestamped `.txt` file, then prunes anything
    /// beyond [`MAX_RETAINED_REPORTS`]. Returns the path written.
    pub fn write(&self, crashes_dir: &Path) -> io::Result<PathBuf> {
        std::fs::create_dir_all(crashes_dir)?;
        let filename = format!("crash-{}.txt", self.timestamp.format("%Y%m%dT%H%M%S%.3fZ"));
        let final_path = crashes_dir.join(&filename);
        let tmp_path = crashes_dir.join(format!("{filename}.tmp"));
        security::write_atomic_private(&tmp_path, &final_path, self.to_text().as_bytes())?;
        rotate(crashes_dir, MAX_RETAINED_REPORTS)?;
        Ok(final_path)
    }
}

/// Every `crash-*.txt` report currently on disk in `crashes_dir`, oldest
/// first — the `crash-<timestamp>.txt` naming sorts lexicographically the
/// same as chronologically, so a plain string sort is enough. Returns an
/// empty list (not an error) if the directory doesn't exist yet.
pub fn list_reports(crashes_dir: &Path) -> io::Result<Vec<PathBuf>> {
    if !crashes_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut reports: Vec<PathBuf> = std::fs::read_dir(crashes_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "txt")
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("crash-"))
        })
        .collect();
    reports.sort();
    Ok(reports)
}

/// The most recently written crash report, if any — what the startup
/// detection dialog shows.
pub fn latest_report(crashes_dir: &Path) -> io::Result<Option<PathBuf>> {
    Ok(list_reports(crashes_dir)?.into_iter().next_back())
}

/// Deletes the oldest reports in `crashes_dir` beyond `keep`.
pub fn rotate(crashes_dir: &Path, keep: usize) -> io::Result<()> {
    let reports = list_reports(crashes_dir)?;
    if reports.len() <= keep {
        return Ok(());
    }
    let excess = reports.len() - keep;
    for path in &reports[..excess] {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// Deletes every crash report on disk in `crashes_dir` — backs
/// Preferences' "Clear Saved Crash Reports" button. Not an error if the
/// directory doesn't exist or is already empty.
pub fn clear_all(crashes_dir: &Path) -> io::Result<()> {
    for path in list_reports(crashes_dir)? {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "veyra-crash-report-test-{label}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn redact_home_path_masks_home_but_keeps_the_rest_of_the_path() {
        let home = Path::new("/home/alice");
        let text = "panicked at /home/alice/Documents/report.pdf:12:3";
        assert_eq!(
            redact_home_path(text, home),
            "panicked at ~/[REDACTED]/Documents/report.pdf:12:3"
        );
    }

    #[test]
    fn redact_home_path_masks_every_occurrence() {
        let home = Path::new("/home/alice");
        let text = "/home/alice/a.txt and also /home/alice/b.txt";
        assert_eq!(
            redact_home_path(text, home),
            "~/[REDACTED]/a.txt and also ~/[REDACTED]/b.txt"
        );
    }

    #[test]
    fn redact_home_path_is_a_no_op_when_home_is_absent_from_text() {
        let home = Path::new("/home/alice");
        assert_eq!(
            redact_home_path("nothing to redact here", home),
            "nothing to redact here"
        );
    }

    #[test]
    fn redact_home_path_handles_an_empty_home_without_mangling_text() {
        let home = Path::new("");
        assert_eq!(redact_home_path("some text", home), "some text");
    }

    #[test]
    fn is_sensitive_env_key_matches_every_fragment_case_insensitively() {
        for key in [
            "GITHUB_TOKEN",
            "api_key",
            "MY_SECRET",
            "AUTH_HEADER",
            "DB_PASSWORD",
            "AWS_CREDENTIAL_FILE",
            "SSH_PRIVATE_KEY",
            "AWS_SIGNATURE_VERSION",
            "HTTP_COOKIE",
            "PHP_SESSIONID",
        ] {
            assert!(is_sensitive_env_key(key), "expected {key} to be sensitive");
        }
    }

    #[test]
    fn is_sensitive_env_key_leaves_ordinary_vars_alone() {
        for key in ["LANG", "DESKTOP_SESSION", "PATH", "XDG_SESSION_TYPE"] {
            assert!(!is_sensitive_env_key(key), "expected {key} to be ordinary");
        }
    }

    #[test]
    fn filter_env_vars_drops_only_sensitive_entries() {
        let vars = vec![
            ("LANG".to_string(), "en_US.UTF-8".to_string()),
            ("GITHUB_TOKEN".to_string(), "ghp_secret".to_string()),
            ("XDG_SESSION_TYPE".to_string(), "wayland".to_string()),
        ];
        let filtered = filter_env_vars(vars);
        assert_eq!(
            filtered,
            vec![
                ("LANG".to_string(), "en_US.UTF-8".to_string()),
                ("XDG_SESSION_TYPE".to_string(), "wayland".to_string()),
            ]
        );
    }

    fn sample_report() -> CrashReport {
        CrashReport::capture(
            Path::new("/home/alice"),
            "0.1.0",
            "Linux",
            "6.10.0",
            "4.16.0",
            "1.6.0",
            "index out of bounds",
            "/home/alice/src/veyra/window.rs:42:5",
            "0: veyra::main\n1: /home/alice/src/veyra/main.rs:10",
            vec![
                ("LANG".to_string(), "en_US.UTF-8".to_string()),
                ("API_KEY".to_string(), "sk-secret".to_string()),
            ],
        )
    }

    #[test]
    fn capture_redacts_home_out_of_message_location_and_backtrace() {
        let report = sample_report();
        assert!(!report.location.contains("/home/alice"));
        assert!(report.location.contains("~/[REDACTED]"));
        assert!(!report.backtrace.contains("/home/alice"));
    }

    #[test]
    fn capture_strips_sensitive_env_vars_but_keeps_ordinary_ones() {
        let report = sample_report();
        assert!(report.env_vars.iter().any(|(k, _)| k == "LANG"));
        assert!(!report.env_vars.iter().any(|(k, _)| k == "API_KEY"));
    }

    #[test]
    fn to_text_includes_every_field_and_the_never_sent_notice() {
        let text = sample_report().to_text();
        assert!(text.contains("Veyra Version: 0.1.0"));
        assert!(text.contains("GTK: 4.16.0"));
        assert!(text.contains("Libadwaita: 1.6.0"));
        assert!(text.contains("index out of bounds"));
        assert!(text.contains("LANG=en_US.UTF-8"));
        assert!(!text.contains("API_KEY"));
        assert!(!text.contains("/home/alice"));
        assert!(text.contains("No information in this report has been sent anywhere"));
    }

    #[test]
    fn write_creates_the_directory_and_a_readable_report_file() {
        let dir = unique_dir("write");
        let path = sample_report().write(&dir).unwrap();
        assert!(path.exists());
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("Veyra Crash Report"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_rotates_out_the_oldest_reports_beyond_the_retention_limit() {
        let dir = unique_dir("rotate");
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..(MAX_RETAINED_REPORTS + 3) {
            let path = dir.join(format!("crash-2024010{i}T000000.000Z.txt"));
            std::fs::write(&path, "old report").unwrap();
        }
        rotate(&dir, MAX_RETAINED_REPORTS).unwrap();
        let remaining = list_reports(&dir).unwrap();
        assert_eq!(remaining.len(), MAX_RETAINED_REPORTS);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn latest_report_is_none_when_the_directory_does_not_exist() {
        let dir = unique_dir("missing");
        assert_eq!(latest_report(&dir).unwrap(), None);
    }

    #[test]
    fn latest_report_picks_the_lexicographically_last_filename() {
        let dir = unique_dir("latest");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("crash-20240101T000000.000Z.txt"), "a").unwrap();
        std::fs::write(dir.join("crash-20240202T000000.000Z.txt"), "b").unwrap();
        let latest = latest_report(&dir).unwrap().unwrap();
        assert_eq!(
            latest.file_name().unwrap(),
            "crash-20240202T000000.000Z.txt"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn clear_all_removes_every_report_and_leaves_the_directory_usable() {
        let dir = unique_dir("clear");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("crash-20240101T000000.000Z.txt"), "a").unwrap();
        std::fs::write(dir.join("crash-20240202T000000.000Z.txt"), "b").unwrap();
        clear_all(&dir).unwrap();
        assert!(list_reports(&dir).unwrap().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn crash_reports_enabled_toggle_round_trips() {
        set_crash_reports_enabled(false);
        assert!(!crash_reports_enabled());
        set_crash_reports_enabled(true);
        assert!(crash_reports_enabled());
    }
}
