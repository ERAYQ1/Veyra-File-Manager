# Veyra — Filesystem Abstraction & Handling Model

**Document Status:** Approved Architecture  
**Scope:** File Representations, Metadata Engine, Edge Case Resilience, Operations Pipeline

---

## 1. Unified Filesystem Representation (`VeyraPath` & `FileItem`)

Veyra introduces a unified, location-agnostic filesystem model capable of handling both POSIX local paths and GIO Virtual Filesystems (GVfs remote URI structures like `sftp://`, `smb://`, `trash://`).

```
                    ┌───────────────────────────────────┐
                    │            FileItem               │
                    ├───────────────────────────────────┤
                    │ - path: VeyraPath                 │
                    │ - name: String                    │
                    │ - metadata: FileMetadata          │
                    │ - mime_type: MimeType             │
                    │ - file_kind: FileKind             │
                    │ - permissions: FilePermissions    │
                    │ - target_symlink: Option<Path>    │
                    └───────────────────────────────────┘
```

---

## 2. Supported File Kinds

1. **Regular File:** Standard binary or text data stream.
2. **Directory:** Container listing child entries.
3. **Symlink:** Symbolic link pointing to target path (tracked alongside valid/broken status).
4. **Hidden File:** Dotfiles (`.bashrc`) or items listed in `.hidden` files.
5. **Executable File:** Files with execute permissions set (`+x`).
6. **Unix Socket:** Local IPC communications socket.
7. **Character / Block Device:** Device node files.
8. **Mount Point / Volume:** Storage device or remote network filesystem.
9. **Unknown / Faulty:** Entries with corrupted inode or inaccessible filesystem state.

---

## 3. Metadata Extraction Engine

`FileMetadata` exposes structured, uniform properties regardless of underlying storage protocol:

- **Name:** Filename display string (Unicode sanitized).
- **Path:** Complete canonical URI / Path representation.
- **Size:** Byte size (formatted human-readable: B, KB, MB, GB, TB).
- **Modified Time:** `chrono::DateTime<Utc>` of last file modification.
- **Created / Birth Time:** Available where underlying OS/FS supports birth time (`statx`).
- **Accessed Time:** Last access timestamp.
- **Permissions:** POSIX mode mask (Octal `0755` & rwxr-xr-x presentation).
- **Owner & Group:** Resolved UID/GID strings.
- **MIME Type:** GIO Content Type lookup + fallback extension mapping.
- **Inode / File Identifier:** Hardware filesystem inode number.

---

## 4. Edge Case Handling Strategy

Veyra guarantees zero panics under transient filesystem anomalies:

| Edge Case | Strategy / Behavior |
| :--- | :--- |
| **Unicode / Special Characters** | Full UTF-8 support, raw OS bytes retention (`OsString` / `PathBuf`), lossy fallbacks for presentation without breaking operations. |
| **Spaces & Long Paths** | Escape handling, deep path handling up to `PATH_MAX` and long path handling. |
| **Broken Symlinks** | Rendered with explicit visually distinct warning styling; properties indicate "Broken Link Target"; operations target link file itself. |
| **Permission Denied** | Non-blocking inline error status badge; operation queue reports clean granular permission prompt. |
| **Read-Only Filesystem** | Operations auto-detect read-only flags and disable destructive context actions; friendly alert modal with destination suggestion. |
| **Mount Disappearing** | `GFileMonitor` / GIO Volume Manager signal catches unmount event, cleanly cancels active jobs on path, and smoothly navigates user to safe fallback directory (`~`). |
| **File Disappearing Mid-Operation** | Operations treat missing source entries as skipped items with explicit log notification instead of crashing operation queue thread. |
