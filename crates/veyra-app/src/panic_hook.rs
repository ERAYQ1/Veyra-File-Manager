use std::panic;
use std::path::{Path, PathBuf};

/// Installs a panic hook that logs panics through `tracing` (so they land in
/// the structured log file, not just stderr), writes a privacy-friendly
/// crash report to `state_dir`'s `crashes/` directory (Faz 53 — local-only,
/// never transmitted, Kural #24), then falls back to Rust's default panic
/// report. A panic here means an unrecoverable programming error, not a
/// user-facing filesystem failure (Rule #18): those must be handled with
/// `Result`, never by panicking.
pub fn install(state_dir: PathBuf) {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_string());
        let message = panic_message(info);

        tracing::error!(location = %location, message = %message, "unrecoverable panic");

        write_crash_report(&state_dir, &message, &location);

        default_hook(info);
    }));
}

fn panic_message(info: &panic::PanicHookInfo<'_>) -> String {
    if let Some(s) = info.payload().downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = info.payload().downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

/// Captures and writes the crash report, swallowing (only logging) any
/// failure — a panic hook that itself panics or aborts the process would
/// hide the original panic's report entirely.
fn write_crash_report(state_dir: &Path, message: &str, location: &str) {
    if !veyra_core::crash_report::crash_reports_enabled() {
        return;
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let backtrace = std::backtrace::Backtrace::force_capture().to_string();
    let (gtk_version, adwaita_version) = veyra_ui::runtime_library_versions();
    let report = veyra_core::crash_report::CrashReport::capture(
        &home,
        env!("CARGO_PKG_VERSION"),
        os_info(),
        kernel_info(),
        gtk_version,
        adwaita_version,
        message,
        location,
        &backtrace,
        std::env::vars().collect(),
    );
    if let Err(err) = report.write(&state_dir.join("crashes")) {
        tracing::error!(error = %err, "failed to write crash report");
    }
}

/// A human-readable OS description: `/etc/os-release`'s `PRETTY_NAME` when
/// present (covers every major distro), falling back to the bare platform
/// name.
fn os_info() -> String {
    if let Ok(contents) = std::fs::read_to_string("/etc/os-release") {
        for line in contents.lines() {
            if let Some(value) = line.strip_prefix("PRETTY_NAME=") {
                return value.trim_matches('"').to_string();
            }
        }
    }
    std::env::consts::OS.to_string()
}

/// The running kernel's release string (e.g. `"6.10.0-arch1-1"`), read via
/// `uname(2)`.
fn kernel_info() -> String {
    // Safety: `uname` writes into a caller-owned, stack-allocated struct
    // and takes no other input; a zeroed `utsname` is always a valid
    // argument, and the call cannot fail for that reason.
    unsafe {
        let mut uts: libc::utsname = std::mem::zeroed();
        if libc::uname(&mut uts) != 0 {
            return "unknown".to_string();
        }
        cstr_field(&uts.release)
    }
}

/// Decodes a fixed-size `c_char` array field of `libc::utsname` (as
/// returned by `uname(2)`) into an owned `String`, stopping at the first
/// NUL terminator.
fn cstr_field(field: &[std::ffi::c_char]) -> String {
    let bytes: Vec<u8> = field
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_info_returns_a_non_empty_string() {
        assert!(!kernel_info().is_empty());
    }

    #[test]
    fn os_info_returns_a_non_empty_string() {
        assert!(!os_info().is_empty());
    }
}
