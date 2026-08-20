# Security & Isolation Model

This document covers privilege separation, sandboxing, and privacy
guarantees. For the threat/mitigation matrix (path traversal, symlink/
TOCTOU, shell injection) see [security-model.md](security-model.md); for
reporting a vulnerability see [SECURITY.md](../SECURITY.md).

## No root by default

Veyra refuses to run with an effective UID of 0 (Kural #20/#23). The check
is a direct `geteuid(2)` call at startup
(`crates/veyra-app/src/root_guard.rs::is_running_as_root`), enforced before
any window or filesystem access happens — there is no `--allow-root` escape
hatch, because a GUI application's entire X11/Wayland/D-Bus/GTK stack
running as root is a far larger attack surface than the specific privileged
operations Veyra actually needs.

## Privileged operations: Polkit, not setuid

The handful of operations that genuinely need elevated privilege — chmod on
a file the user doesn't own, permanent deletion outside their own
directories — go through `pkexec` (`crates/veyra-ui/src/privileged.rs`),
Polkit's authorization mechanism, rather than a setuid binary or an
in-process privilege drop:

- `pkexec` is located by resolving `$PATH` directly (never via a shell,
  Kural #19), the same resolution `terminal.rs` uses for locating a
  terminal emulator.
- Each privileged call builds an explicit argument vector
  (`std::process::Command`) — the target path is passed as one argument,
  never interpolated into a command string.
- Polkit's own authentication agent prompts the user interactively; Veyra
  never caches or handles the password itself.
- Exit codes follow `pkexec`'s convention (126 = authorization denied,
  127 = agent not found), surfaced to the user as an actionable error (see
  [architecture.md](architecture.md) and Faz 51's error UX layer) rather
  than a raw process-exit-code message.
- If `pkexec` isn't on `$PATH` at all (minimal/non-desktop distros), the
  privileged action is unavailable and reports so — Veyra never silently
  falls back to attempting the operation unprivileged.

## Credential and path redaction

Two independent code paths strip sensitive data before it ever reaches
disk — both are pure functions, unit-tested directly, not "best effort"
string scrubbing bolted onto the I/O call site:

- **`veyra-core::logging_sanitizer`** (Faz 54) — every line written to
  `veyra.log` passes through `sanitize_log_line`, which chains:
  - `redact_uri_credentials`: `sftp://alice:secret123@host` →
    `sftp://alice:***@host`, and the SMB equivalent
    (`smb://domain;user:password@host` → `smb://domain;user:***@host`),
    handling multiple URIs on one line.
  - `redact_tokens`: `Bearer <token>` and `token=`/`api_key=`/`apikey=`/
    `access_token=`/`secret=` values (case-insensitive) → `[REDACTED_TOKEN]`.
  - `redact_home_path`: `/home/username/...` → `~/[REDACTED_PATH]/...`.

  This runs on the file writer only — stdout output during development is
  left unsanitized so a developer can still read a raw local log while
  iterating; only the persisted file is scrubbed.

- **`veyra-core::crash_report`** (Faz 53) applies its own
  `redact_home_path` and `filter_env_vars`/`is_sensitive_env_key` (drops
  any environment variable whose name contains `TOKEN`/`KEY`/`SECRET`/
  `AUTH`/`PASS`, case-insensitive) before a `CrashReport` is ever
  serialized — `CrashReport::capture()` applies both automatically, so a
  call site can't forget to redact.

Neither module ever logs raw file *content* — only paths and structured
metadata are recorded (Kural #23).

## Zero telemetry

Veyra makes no network calls of its own and transmits no data anywhere
(Kural #27). Logs (`$XDG_STATE_HOME/veyra/logs/veyra.log`, rotated at 5 MB
with 3 backups kept) and crash reports
(`$XDG_STATE_HOME/veyra/crashes/`, atomically written with `0600`
permissions, capped at 5 retained reports) are local files the user can
open, copy, or delete themselves — the crash dialog's "Copy Report and
Open GitHub Issues" action requires the user to explicitly paste and
submit it; nothing is sent automatically. Both crash reporting and log
retention are visible, user-controllable settings (Preferences → Privacy),
not silent defaults hidden from the user.

## Sandbox boundary (Flatpak)

The Flatpak build's entire permission grant is itemized, justified, and
tested against the manifest in
[flatpak_permissions.md](flatpak_permissions.md) — the short version: GTK
windowing sockets, the one broad `--filesystem=host` grant a file manager
inherently needs to browse arbitrary mounted locations, and D-Bus talk
names for GVfs/UDisks2/notifications/the cross-desktop FileManager1
contract. No network access, no raw device passthrough, no root/setuid
escape from inside the sandbox — privileged operations still route through
the same `pkexec`/Polkit path described above, which itself prompts the
host's Polkit agent rather than anything the sandbox could bypass.

## Temp file safety

Temporary files are created under `$XDG_RUNTIME_DIR/veyra/` with a
restrictive `0700` directory mode, never under a predictable, world-
writable path like a bare `/tmp/veyra-XXXX`.
