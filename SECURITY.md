# Security Policy

Veyra is a file manager: it runs with the user's own filesystem permissions,
extracts untrusted archives, follows symlinks, and mounts network shares. A
vulnerability here can mean data loss or code execution under the user's
account, so security reports get priority handling.

## Supported Versions

Veyra is pre-1.0 (`0.1.0`). Until a stable `1.0` release, only the latest
commit on `main` is supported — there are no maintained release branches to
backport fixes to yet. See [docs/roadmap.md](docs/roadmap.md) for the path
to `1.0` (Faz 60).

## Reporting a Vulnerability

**Do not open a public GitHub issue for a security vulnerability.**

Report privately through
[GitHub Security Advisories](https://github.com/ERAYQ1/Veyra-File-Manager/security/advisories/new)
for this repository. This creates a private discussion visible only to you
and the maintainers until a fix is ready and coordinated disclosure happens.

Include, where applicable:

- The affected file(s)/function(s) and Veyra commit or version.
- Reproduction steps (a minimal archive, path, or filename triggers most
  filesystem-layer issues here).
- The actual vs. expected behavior, and the impact (e.g. path escape,
  privilege escalation, information disclosure, denial of service).

You should get an initial response within a few days. There is no bug
bounty program — this is a community project.

## Threat Model

Veyra's security posture is documented in full in
[docs/security-model.md](docs/security-model.md) and
[docs/security.md](docs/security.md); summarized here:

- **No root by default.** Veyra refuses to start as UID 0
  (`root_guard`, Kural #20). Privileged operations (permission changes
  beyond the owner's own files, etc.) go through Polkit/D-Bus, never a
  setuid binary or an in-process privilege drop.
- **Path traversal defense.** Archive extraction (ZIP/TAR/GZ/XZ/ZST/7Z)
  canonicalizes every entry path and rejects any entry that would resolve
  outside the extraction target ("Zip Slip" and equivalents).
- **Symlink / TOCTOU protection.** Filesystem operations use `lstat`/
  `O_NOFOLLOW` semantics on the operated-on path rather than trusting a
  path resolved earlier, and recursive walks are depth-limited against
  symlink cycles.
- **No shell interpolation.** External processes (terminal launch, "Open
  With") are always invoked via argument vectors
  (`std::process::Command`/`GAppInfo`), never through a concatenated
  `sh -c` string.
- **Credential and path redaction.** Log files and crash reports never
  contain raw file content, and both a structured sanitizer
  (`veyra-core::logging_sanitizer`) and the crash reporter
  (`veyra-core::crash_report`) strip URI credentials, bearer/API tokens,
  and the user's home directory prefix before anything reaches disk.
- **Zero telemetry.** Veyra makes no network calls of its own and sends
  no data anywhere. Crash reports and logs are local-only files the user
  can inspect, copy, or delete themselves (Faz 53/54).
- **Sandboxed by default in Flatpak.** The Flatpak build's permission set
  is documented and tested entry-by-entry in
  [docs/flatpak_permissions.md](docs/flatpak_permissions.md); a test
  (`manifest_finish_args_match_the_documented_minimal_set`) fails the
  build if the manifest and that document drift apart.

## Scope

In scope: anything in this repository — the `veyra-core`,
`veyra-filesystem`, `veyra-search`, `veyra-ui`, `veyra-app` crates, the
Flatpak manifest, and the native packaging scripts (`packaging/`).

Out of scope: vulnerabilities in GTK4, Libadwaita, GLib/GIO, GVfs, SQLite,
or other upstream dependencies — please report those to the respective
upstream project instead.
