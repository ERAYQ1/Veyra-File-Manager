//! Faz 54: privacy-hardening for `tracing` output (Kural #23) — every line
//! that reaches the log file is scrubbed of URI credentials, bearer/API
//! tokens, and the user's home directory before it's written, independent
//! of whether the call site that produced it remembered to sanitize
//! itself. `veyra-app`'s `logging::init` wraps the log file's writer with
//! [`sanitize_log_line`]; this module also owns the file's size-based
//! rotation (`rotate_log_if_needed`), a related but separate on-disk
//! lifecycle concern for the same file.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

/// Replaces the password half of a URI's userinfo (`scheme://user:pass@host`
/// or `scheme://domain;user:pass@host`) with `***`, leaving the username
/// (and everything else) intact — e.g.
/// `sftp://alice:secret123@server.com/share` becomes
/// `sftp://alice:***@server.com/share`. A URI with no `user:pass@`
/// authority (no `@` before the next `/`) passes through unchanged.
pub fn redact_uri_credentials(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut remainder = text;
    loop {
        let Some(scheme_end) = remainder.find("://") else {
            out.push_str(remainder);
            break;
        };
        let split = scheme_end + 3;
        out.push_str(&remainder[..split]);
        let after = &remainder[split..];

        let Some(at_pos) = after.find('@') else {
            out.push_str(after);
            break;
        };
        let authority = &after[..at_pos];

        // A `/` before the `@` means this isn't actually a `user:pass@host`
        // authority (the `@` belongs to a later path segment or another
        // URI entirely) — pass the segment up to the `/` through untouched
        // and keep scanning from there.
        if let Some(slash_pos) = authority.find('/') {
            out.push_str(&authority[..slash_pos]);
            remainder = &after[slash_pos..];
            continue;
        }

        match authority.find(':') {
            Some(colon_pos) => {
                out.push_str(&authority[..=colon_pos]);
                out.push_str("***");
            }
            None => out.push_str(authority),
        }
        out.push('@');
        remainder = &after[at_pos + 1..];
    }
    out
}

/// Markers whose value (up to the next whitespace/separator) is a
/// credential and gets replaced with `[REDACTED_TOKEN]` — the marker text
/// itself is kept so the log line still shows *that* a token was there.
const KEY_VALUE_TOKEN_MARKERS: [&str; 8] = [
    "token=",
    "api_key=",
    "apikey=",
    "access_token=",
    "secret=",
    "password=",
    "passwd=",
    "credential=",
];

/// The `Bearer ` HTTP auth-header scheme — same treatment, but
/// space-separated rather than `=`-separated.
const BEARER_MARKER: &str = "Bearer ";

/// Masks `Bearer <token>` and `key=<token>`-style credentials (`token=`,
/// `api_key=`, `apikey=`, `access_token=`, `secret=`, `password=`,
/// `passwd=`, `credential=`) with `[REDACTED_TOKEN]`, matching each marker
/// case-insensitively but leaving
/// its original casing in the output. A marker with nothing after it (end
/// of string, or immediately followed by a separator) is left alone —
/// there's no value to redact.
pub fn redact_tokens(text: &str) -> String {
    let mut result = redact_after_marker(text, BEARER_MARKER);
    for marker in KEY_VALUE_TOKEN_MARKERS {
        result = redact_after_marker(&result, marker);
    }
    result
}

fn redact_after_marker(text: &str, marker: &str) -> String {
    let marker_lower = marker.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let rest_lower = rest.to_ascii_lowercase();
        let Some(pos) = rest_lower.find(&marker_lower) else {
            out.push_str(rest);
            break;
        };
        let marker_end = pos + marker.len();
        out.push_str(&rest[..marker_end]);
        let value_part = &rest[marker_end..];
        let boundary = value_part
            .find(|c: char| c.is_whitespace() || matches!(c, '&' | '"' | '\'' | ')' | ',' | ';'))
            .unwrap_or(value_part.len());
        if boundary > 0 {
            out.push_str("[REDACTED_TOKEN]");
        }
        rest = &value_part[boundary..];
    }
    out
}

/// The replacement Faz 54's log sanitizer uses for the user's home
/// directory — distinct from Faz 53's `crash_report::redact_home_path`
/// (`~/[REDACTED]`) only in wording; same idea, kept as a separate
/// function since the two modules serve different on-disk artifacts (a
/// one-shot crash report vs. every line of a running log).
pub fn redact_home_path(text: &str, home: &Path) -> String {
    let home_str = home.to_string_lossy();
    if home_str.is_empty() {
        return text.to_string();
    }
    text.replace(home_str.as_ref(), "~/[REDACTED_PATH]")
}

/// Applies every Faz 54 redaction to one log line, in order: URI
/// credentials, then tokens, then the home directory. Called on every
/// buffer `logging::init`'s file writer receives, so no individual
/// `tracing::info!`/`warn!`/etc. call site needs to remember to sanitize
/// itself — this is the single choke point every line passes through
/// before touching disk.
pub fn sanitize_log_line(text: &str, home: &Path) -> String {
    let text = redact_uri_credentials(text);
    let text = redact_tokens(&text);
    redact_home_path(&text, home)
}

/// Log file rotation: once `veyra.log` exceeds this size, it's rotated out
/// to `veyra.log.1` before a fresh `veyra.log` is opened.
pub const MAX_LOG_SIZE_BYTES: u64 = 5 * 1024 * 1024;

/// How many rotated backups (`veyra.log.1` .. `veyra.log.N`) are kept;
/// anything older is deleted outright.
pub const MAX_LOG_BACKUPS: usize = 3;

/// If `log_file` exists and is at or over [`MAX_LOG_SIZE_BYTES`], shifts
/// every existing backup up by one suffix (`.2` → `.3`, `.1` → `.2`,
/// dropping whatever was at `.MAX_LOG_BACKUPS`) and renames `log_file`
/// itself to `.1`, leaving `log_file`'s path free for `logging::init` to
/// open fresh. A no-op (not an error) if `log_file` doesn't exist yet or
/// is still under the size limit.
pub fn rotate_log_if_needed(log_file: &Path) -> io::Result<()> {
    let Ok(metadata) = std::fs::metadata(log_file) else {
        return Ok(());
    };
    if metadata.len() < MAX_LOG_SIZE_BYTES {
        return Ok(());
    }

    let oldest = backup_path(log_file, MAX_LOG_BACKUPS);
    if oldest.exists() {
        std::fs::remove_file(&oldest)?;
    }
    for index in (1..MAX_LOG_BACKUPS).rev() {
        let src = backup_path(log_file, index);
        if src.exists() {
            std::fs::rename(&src, backup_path(log_file, index + 1))?;
        }
    }
    std::fs::rename(log_file, backup_path(log_file, 1))
}

fn backup_path(log_file: &Path, index: usize) -> PathBuf {
    let mut name: OsString = log_file.as_os_str().to_os_string();
    name.push(format!(".{index}"));
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "veyra-logging-sanitizer-test-{label}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn redact_uri_credentials_masks_sftp_password() {
        assert_eq!(
            redact_uri_credentials("sftp://alice:secret123@server.com/share"),
            "sftp://alice:***@server.com/share"
        );
    }

    #[test]
    fn redact_uri_credentials_masks_smb_domain_user_password() {
        assert_eq!(
            redact_uri_credentials("smb://domain;user:password@host"),
            "smb://domain;user:***@host"
        );
    }

    #[test]
    fn redact_uri_credentials_masks_http_password() {
        assert_eq!(
            redact_uri_credentials("http://bob:hunter2@example.com/path"),
            "http://bob:***@example.com/path"
        );
    }

    #[test]
    fn redact_uri_credentials_leaves_uris_without_credentials_alone() {
        assert_eq!(
            redact_uri_credentials("sftp://server.com/share"),
            "sftp://server.com/share"
        );
        assert_eq!(
            redact_uri_credentials("https://example.com/a@b/c"),
            "https://example.com/a@b/c"
        );
    }

    #[test]
    fn redact_uri_credentials_handles_multiple_uris_in_one_line() {
        let text = "copy sftp://a:pw1@h1/x to smb://d;u:pw2@h2/y";
        assert_eq!(
            redact_uri_credentials(text),
            "copy sftp://a:***@h1/x to smb://d;u:***@h2/y"
        );
    }

    #[test]
    fn redact_tokens_masks_bearer_header() {
        assert_eq!(
            redact_tokens("Authorization: Bearer abc123.def456"),
            "Authorization: Bearer [REDACTED_TOKEN]"
        );
    }

    #[test]
    fn redact_tokens_masks_key_value_style_credentials() {
        assert_eq!(
            redact_tokens("request?token=abc123&user=bob"),
            "request?token=[REDACTED_TOKEN]&user=bob"
        );
        assert_eq!(redact_tokens("api_key=sk-xyz"), "api_key=[REDACTED_TOKEN]");
        assert_eq!(
            redact_tokens("password=hunter2"),
            "password=[REDACTED_TOKEN]"
        );
        assert_eq!(redact_tokens("passwd=hunter2"), "passwd=[REDACTED_TOKEN]");
        assert_eq!(
            redact_tokens("credential=abc123"),
            "credential=[REDACTED_TOKEN]"
        );
    }

    #[test]
    fn redact_tokens_is_case_insensitive_on_the_marker() {
        assert_eq!(redact_tokens("API_KEY=sk-xyz"), "API_KEY=[REDACTED_TOKEN]");
    }

    #[test]
    fn redact_tokens_leaves_ordinary_text_alone() {
        assert_eq!(
            redact_tokens("hello world, no secrets here"),
            "hello world, no secrets here"
        );
    }

    #[test]
    fn redact_tokens_leaves_a_marker_with_no_value_alone() {
        assert_eq!(redact_tokens("token="), "token=");
    }

    #[test]
    fn redact_home_path_masks_home_but_keeps_the_rest_of_the_path() {
        let home = Path::new("/home/alice");
        assert_eq!(
            redact_home_path("/home/alice/Documents/report.pdf", home),
            "~/[REDACTED_PATH]/Documents/report.pdf"
        );
    }

    #[test]
    fn sanitize_log_line_applies_every_redaction_together() {
        let home = Path::new("/home/alice");
        let line = "connecting to sftp://alice:secret123@server.com/share, Bearer abc123, reading /home/alice/private.txt";
        let sanitized = sanitize_log_line(line, home);
        assert!(sanitized.contains("sftp://alice:***@server.com/share"));
        assert!(sanitized.contains("Bearer [REDACTED_TOKEN]"));
        assert!(sanitized.contains("~/[REDACTED_PATH]/private.txt"));
        assert!(!sanitized.contains("secret123"));
        assert!(!sanitized.contains("abc123"));
        assert!(!sanitized.contains("/home/alice"));
    }

    #[test]
    fn rotate_log_if_needed_is_a_no_op_when_the_file_does_not_exist() {
        let dir = unique_dir("missing");
        std::fs::create_dir_all(&dir).unwrap();
        let log_file = dir.join("veyra.log");
        assert!(rotate_log_if_needed(&log_file).is_ok());
        assert!(!log_file.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rotate_log_if_needed_is_a_no_op_under_the_size_limit() {
        let dir = unique_dir("small");
        std::fs::create_dir_all(&dir).unwrap();
        let log_file = dir.join("veyra.log");
        std::fs::write(&log_file, b"small log").unwrap();
        rotate_log_if_needed(&log_file).unwrap();
        assert_eq!(std::fs::read(&log_file).unwrap(), b"small log");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rotate_log_if_needed_renames_the_oversized_file_to_dot_one() {
        let dir = unique_dir("rotate-one");
        std::fs::create_dir_all(&dir).unwrap();
        let log_file = dir.join("veyra.log");
        std::fs::write(&log_file, vec![b'x'; MAX_LOG_SIZE_BYTES as usize]).unwrap();
        rotate_log_if_needed(&log_file).unwrap();
        assert!(!log_file.exists());
        assert!(dir.join("veyra.log.1").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rotate_log_if_needed_shifts_existing_backups_and_drops_the_oldest() {
        let dir = unique_dir("rotate-shift");
        std::fs::create_dir_all(&dir).unwrap();
        let log_file = dir.join("veyra.log");
        std::fs::write(&log_file, vec![b'x'; MAX_LOG_SIZE_BYTES as usize]).unwrap();
        std::fs::write(dir.join("veyra.log.1"), b"backup 1").unwrap();
        std::fs::write(dir.join("veyra.log.2"), b"backup 2").unwrap();
        std::fs::write(dir.join("veyra.log.3"), b"backup 3 (should be dropped)").unwrap();

        rotate_log_if_needed(&log_file).unwrap();

        assert!(!log_file.exists());
        assert_eq!(
            std::fs::read(dir.join("veyra.log.1")).unwrap(),
            vec![b'x'; MAX_LOG_SIZE_BYTES as usize]
        );
        assert_eq!(std::fs::read(dir.join("veyra.log.2")).unwrap(), b"backup 1");
        assert_eq!(std::fs::read(dir.join("veyra.log.3")).unwrap(), b"backup 2");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
