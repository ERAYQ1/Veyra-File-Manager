# Veyra — Technology Decisions & Crate Justification

**Document Status:** Approved Architecture  
**Policy:** Zero bloat. Every crate added to `Cargo.toml` must have a documented technical justification and license audit.

---

## 1. Core Stack

### 1.1 Programming Language: Rust 2021
- **Justification:** Guarantees memory safety, thread safety without garbage collection overhead, high performance, and seamless C/POSIX bindings for GIO/GTK.
- **License:** Apache 2.0 / MIT.

### 1.2 GUI Framework: GTK4 & Libadwaita
- **Justification:** Native Linux GTK4 integration, modern GNOME Libadwaita design language, high-performance GPU-accelerated rendering, built-in accessibility (AT-SPI), smooth animations, and Wayland/X11 support.
- **License:** LGPL-2.1-or-later.

### 1.3 Linux Integration: GIO / GVfs / GObject Introspection (`gio`, `glib`)
- **Justification:** Official Linux virtual filesystem layer (GIO) for transparent support of local files, mount points, Trash, network shares (SFTP/SMB), MIME type detection, and desktop applications integration.
- **License:** LGPL-2.1-or-later.

---

## 2. Core Dependencies & Justification Matrix

| Crate | Category | Purpose & Justification | License |
| :--- | :--- | :--- | :--- |
| `gtk4` | GUI | Safe Rust bindings for GTK4 windowing, layout, and widgets. | MIT |
| `libadwaita` | GUI | Modern GNOME HIG widgets (AdwApplication, HeaderBar, Toast, Preferences Window). | MIT |
| `gio` | Systems | GIO VFS integration, file monitoring (`GFileMonitor`), app launcher. | MIT |
| `glib` | Infrastructure | Core GLib main loop integration, signals, and GLib async channel. | MIT |
| `tokio` | Async Runtime | Asynchronous task scheduling for background IO operations (features restricted strictly to required ones: `rt-multi-thread`, `sync`, `fs`, `time`). | MIT |
| `rusqlite` | Database | Ergonomic SQLite bindings with bundled `fts5` support for fast local file search. | MIT |
| `serde` & `serde_json` | Serialization | Configuration persistence, settings management, state serialization. | Apache 2.0 / MIT |
| `tracing` & `tracing-subscriber` | Logging | Structured diagnostic logging with configurable log levels (`debug`, `info`, `warn`, `error`). | MIT |
| `thiserror` | Error Handling | Structured, strong-typed enum error definitions for internal library crates (`veyra-core`, `veyra-filesystem`). | Apache 2.0 / MIT |
| `anyhow` | Error Handling | Top-level application error reporting for `veyra-app` binary boundary. | Apache 2.0 / MIT |
| `uuid` | Utilities | Unique task IDs for file operations queue and operation history tracking. | Apache 2.0 / MIT |
| `mime_guess` | Utilities | Fast extension-to-MIME fallback mapping when GIO content-type query is insufficient. | MIT |
| `walkdir` | Filesystem | Efficient directory traversal for deep disk analysis and recursive file counting. | Unlicense / MIT |
| `libc` | Systems | Raw `geteuid(2)` syscall binding to enforce the no-root-by-default startup guard (`veyra-app` only). Std has no portable UID accessor. | MIT / Apache 2.0 |
| `mime_guess` | Utilities | Extension-to-MIME fallback when GIO's `standard::content-type` lookup is unavailable or generic (`veyra-filesystem`). | MIT |
| `tempfile` | Dev-only | Real, auto-cleaned temp directories for `veyra-filesystem` integration tests (permission/edge-case coverage, Rule #34/#35). Never compiled into shipped binaries (`dev-dependencies` only). | MIT / Apache 2.0 |
| `async-channel` | Concurrency | Worker-thread → GTK-main-thread result delivery for async `read_dir` calls (`veyra-ui`). `glib::MainContext::channel` was removed upstream; this is the GNOME/gtk-rs ecosystem's documented replacement, paired with `glib::spawn_future_local` (Rule #14: never block the UI thread). | Apache 2.0 / MIT |

---

## 3. Dependency Addition Policy

Before adding any new dependency to Veyra's Cargo workspace:
1. **Necessity Check:** Can this feature be cleanly implemented using Rust standard library or existing crates (`gio`/`glib`/`tokio`)? If yes, do not add crate.
2. **Maintenance & Audit:** Verify crate activity, security audit record (`cargo-audit`), crate size, and transitively introduced sub-crates.
3. **License Audit:** Crate license must be compatible with LGPL-3.0 / GPL-3.0 / MIT / Apache-2.0. No non-commercial or restrictive licenses.
