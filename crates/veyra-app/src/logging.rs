use std::fs::OpenOptions;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::Context;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

/// Initializes structured `tracing` logging: human-readable output on stdout
/// plus a plain (non-ANSI), Faz 54-sanitized copy appended to `log_file`
/// under the XDG state directory. Level defaults to [`default_level_filter`]
/// (`DEBUG` for `veyra`'s own crates in a debug build, `INFO` in release),
/// overridable at any time via `RUST_LOG`. Before opening `log_file`, rotates
/// it out to `.1`/`.2`/`.3` if it's grown past
/// `veyra_core::logging_sanitizer::MAX_LOG_SIZE_BYTES` (Faz 54C).
pub fn init(log_file: &Path) -> anyhow::Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level_filter(cfg!(debug_assertions))));

    let stdout_layer = fmt::layer().with_target(false).compact();

    if let Some(parent) = log_file.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create log directory at {}", parent.display()))?;
    }
    if let Err(err) = veyra_core::logging_sanitizer::rotate_log_if_needed(log_file) {
        eprintln!("warning: failed to rotate log file: {err}");
    }

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)
        .with_context(|| format!("failed to open log file at {}", log_file.display()))?;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let sanitizing_writer = SanitizingWriter { file, home };

    let file_layer = fmt::layer()
        .with_target(false)
        .with_ansi(false)
        .with_writer(sanitizing_writer);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer)
        .with(file_layer)
        .try_init()
        .context("failed to install tracing subscriber (already initialized?)")?;

    Ok(())
}

/// The default `EnvFilter` directive string when `RUST_LOG` isn't set:
/// `DEBUG` for every Veyra crate in a development (debug-assertions) build,
/// `INFO` in a production (release) build — third-party crate output stays
/// at `WARN` either way. Pure function of `debug_build` (rather than reading
/// `cfg!(debug_assertions)` directly) so both branches are unit-testable
/// regardless of which profile `cargo test` itself runs under.
fn default_level_filter(debug_build: bool) -> &'static str {
    if debug_build {
        "veyra=debug,veyra_ui=debug,veyra_filesystem=debug,veyra_search=debug,warn"
    } else {
        "veyra=info,veyra_ui=info,veyra_filesystem=info,warn"
    }
}

/// A `tracing-subscriber` writer that runs every buffer through
/// `veyra_core::logging_sanitizer::sanitize_log_line` before it reaches the
/// log file — the single choke point that guarantees a URI password, a
/// bearer token, or the user's home directory never lands on disk, even if
/// the `tracing::info!`/`warn!`/etc. call site that produced the line never
/// thought to redact it itself (Kural #23).
struct SanitizingWriter {
    file: std::fs::File,
    home: PathBuf,
}

/// The per-event handle `SanitizingWriter::make_writer` hands to
/// `tracing-subscriber`, mirroring how `std::fs::File`'s own `MakeWriter`
/// impl hands out a `&File` per call rather than cloning the underlying
/// file descriptor.
struct SanitizingHandle<'a> {
    file: &'a std::fs::File,
    home: &'a Path,
}

impl io::Write for SanitizingHandle<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(buf);
        let sanitized = veyra_core::logging_sanitizer::sanitize_log_line(&text, self.home);
        (&*self.file).write_all(sanitized.as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        (&*self.file).flush()
    }
}

impl<'a> MakeWriter<'a> for SanitizingWriter {
    type Writer = SanitizingHandle<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        SanitizingHandle {
            file: &self.file,
            home: &self.home,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_level_filter_is_debug_for_every_veyra_crate_in_development() {
        let filter = default_level_filter(true);
        assert!(filter.contains("veyra=debug"));
        assert!(filter.contains("veyra_ui=debug"));
        assert!(filter.contains("veyra_filesystem=debug"));
        assert!(filter.contains("veyra_search=debug"));
        assert!(filter.contains("warn"));
    }

    #[test]
    fn default_level_filter_is_info_for_every_veyra_crate_in_production() {
        let filter = default_level_filter(false);
        assert!(filter.contains("veyra=info"));
        assert!(filter.contains("veyra_ui=info"));
        assert!(filter.contains("veyra_filesystem=info"));
        assert!(!filter.contains("debug"));
    }
}
