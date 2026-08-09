# Veyra — System Architecture Document

**System Version:** 0.1.0  
**Target Platform:** Linux Desktop (GTK4 + Libadwaita)  
**Primary Language:** Rust 2021 Edition  

---

## 1. High-Level Architecture Overview

Veyra is structured as a modular Cargo Workspace designed for extreme responsiveness, memory safety, and thread isolation. The application explicitly separates UI rendering from heavy I/O, search indexing, metadata extraction, and network operations.

```
                               ┌────────────────────────────────┐
                               │           veyra-app            │
                               │  (Application Entry, Main)     │
                               └───────────────┬────────────────┘
                                               │
                               ┌───────────────▼────────────────┐
                               │            veyra-ui            │
                               │  (GTK4 / Libadwaita Widgets)   │
                               └───────┬────────────────┬───────┘
                                       │                │
                      ┌────────────────┘                └────────────────┐
                      │ Async Event Channel                              │ Direct Calls / Worker Calls
                      ▼                                                  ▼
       ┌──────────────────────────────┐                   ┌──────────────────────────────┐
       │       veyra-filesystem       │                   │          veyra-core          │
       │ (GIO/GVfs, POSIX, Ops Queue) │                   │ (Models, Config, Traits, Log)│
       └──────────────────────────────┘                   └──────────────────────────────┘
```

---

## 2. Cargo Workspace Architecture

| Crate Name | Primary Purpose | Allowed Dependencies |
| :--- | :--- | :--- |
| `veyra-core` | Data models (`FileItem`, `VeyraPath`), configuration, logging, event definitions, shared errors, utility traits. | `serde`, `thiserror`, `tracing`, `uuid`, `chrono` |
| `veyra-filesystem` | Abstract filesystem operations (read dir, copy, move, trash, metadata, permissions, GVfs/GIO bridge, operation queue, undo history). | `veyra-core`, `gio`, `glib`, `tokio`, `walkdir`, `mime_guess` |
| `veyra-ui` | GTK4 & Libadwaita views, widgets, tabs, split views, context menus, command palette, signal handlers. | `veyra-core`, `veyra-filesystem`, `gtk4`, `libadwaita`, `gio`, `glib` |
| `veyra-app` | Binary target, application lifecycle, CLI parsing, Single Instance D-Bus handling, crash handling. | `veyra-core`, `veyra-filesystem`, `veyra-ui`, `libadwaita`, `tracing-subscriber` |

---

## 3. Concurrency & Thread Boundaries

To guarantee a **60-120 FPS fluid UI** under heavy folder loads (e.g., 100,000 files), Veyra enforces strict concurrency boundaries:

1. **GTK Main Loop Thread (Thread 0)**:
   - Responsible strictly for rendering, event dispatching, and widget tree updates.
   - **Rule:** Zero synchronous I/O operations (file reading, stat calls, network calls, DB queries) allowed on Thread 0.

2. **Tokio Async Worker Pool**:
   - Handles background tasks: file operations (copy, move, delete), search indexing, thumbnail generation, network filesystem queries (SFTP/SMB).
   - Communicates with GTK Main Loop using `glib::MainContext::channel` or `gio::Task`.

3. **Operation Queue & Cancellation Engine**:
   - All bulk file tasks run inside cancellation-aware worker streams (`tokio::sync::mpsc`, `tokio_util::sync::CancellationToken`).
   - Supports Pause, Resume, and Granular Conflict Resolution (Overwrite, Rename, Skip).

---

## 4. Subsystem Breakdown

### 4.1 Search & Indexing Engine (`veyra-search`)
- Embedded SQLite with FTS5 (Full Text Search) extension.
- Low-priority background indexing thread using Linux `nice` / IO scheduling (`ioprio_set`).

### 4.2 Thumbnail & Preview Engine (`veyra-preview`)
- Asynchronous thumbnail rendering pipeline with two-level caching:
  - Level 1: In-memory LRU Cache (capped at user memory budget).
  - Level 2: On-disk XDG Thumbnail Specification standard cache (`~/.cache/thumbnails/` & `~/.cache/veyra/thumbnails/`).

### 4.3 Network & Remote Filesystems (`veyra-network`)
- GIO / GVfs abstraction layer providing unified `VeyraPath` access to SFTP, SMB, FTP, WebDAV, and SSH locations.

### 4.4 Plugin Architecture (`veyra-plugin`)
- Modular extension interface with sandboxed capability-based permissions (no unrestricted raw disk access to untrusted plugins).

---

## 5. Architectural Invariants

1. **Immutability of UI State**: UI state reflects core filesystem model events via explicit data binding or reactive signal streams.
2. **Crash Resilience**: Component failures (e.g. thumbnail decoder crash or network drop) must be caught locally without crashing the main application binary.
