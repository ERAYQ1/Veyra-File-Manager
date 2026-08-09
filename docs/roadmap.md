# 🦀 Veyra — Master Development Roadmap (Phases 0 - 60)

**Goal:** Dolphin-level, ultra-modern, smooth, secure, fast, developer-friendly Linux file manager in Rust + GTK4 + Libadwaita.

---

## VEYRA DEVELOPMENT RULES (60 Core Invariants)

1. Veyra is a serious production-quality Linux file manager.
2. Do not create a toy/demo application.
3. Do not implement everything in one phase.
4. Follow the roadmap phase by phase.
5. Before implementing a phase, inspect the existing architecture.
6. Never destroy working functionality to implement a new feature.
7. Do not use placeholder implementations unless explicitly marked and documented.
8. Do not leave TODOs for core functionality.
9. Do not silently ignore compiler warnings.
10. `cargo fmt`, `cargo clippy` and `cargo test` must remain clean.
11. Prefer safe Rust.
12. Avoid unsafe Rust unless absolutely necessary.
13. If unsafe is required, isolate it and document the safety invariants.
14. Never block the GTK UI thread with filesystem, indexing, thumbnail, archive or network operations.
15. All expensive operations must run asynchronously/background.
16. Handle cancellation correctly.
17. Handle errors explicitly.
18. Never panic because of a normal user filesystem error.
19. Never assume paths are valid forever.
20. Files may disappear between discovery and operation.
21. Handle permission errors gracefully.
22. Never execute shell commands using unsanitized user input.
23. Never run Veyra as root by default.
24. Protect archive extraction against path traversal.
25. Protect against symlink attacks.
26. Do not leak sensitive information through logs.
27. Do not send telemetry without explicit user consent.
28. Do not hard-code the terminal emulator.
29. Do not hard-code desktop-specific assumptions where standards exist.
30. Follow XDG Base Directory specifications.
31. Follow Linux desktop standards.
32. Follow GTK4 and Libadwaita HIG.
33. All interactive controls must have accessible labels.
34. Everything important must be keyboard accessible.
35. UI must remain responsive with very large directories.
36. Use virtualization/lazy loading where necessary.
37. Do not load thousands of thumbnails into memory simultaneously.
38. Do not scan entire disks on the UI thread.
39. Search indexing must be resource-aware.
40. Memory usage must be monitored and optimized.
41. Add tests for every important filesystem operation.
42. Test Unicode and unusual filenames.
43. Test permissions and failure cases.
44. Test large directories.
45. Test concurrent filesystem changes.
46. Never assume a file still exists after reading its metadata.
47. Preserve user data.
48. Destructive operations require explicit confirmation when appropriate.
49. Permanent deletion must never be accidentally triggered by normal Delete.
50. Keep architecture modular.
51. Keep filesystem logic separate from UI logic.
52. Keep search/indexing separate from UI.
53. Keep preview generation separate from UI.
54. Keep network functionality separate from core filesystem logic.
55. Document architectural decisions.
56. Do not copy code from GPL projects into incompatible code.
57. Respect licenses of every referenced open-source project.
58. Prefer original implementations inspired by public architecture/features.
59. Before using an external library, verify its license compatibility.
60. Never claim a feature is complete without testing it.

---

## Execution Sequence Per Phase

Every phase follows a strict 15-step execution protocol:
1. CURRENT STATE ANALYSIS
2. REQUIREMENTS
3. ARCHITECTURE
4. IMPLEMENTATION PLAN
5. IMPLEMENTATION
6. TESTS
7. SECURITY REVIEW
8. PERFORMANCE REVIEW
9. UI/UX REVIEW
10. BUILD
11. CLIPPY
12. TEST
13. FINAL VERIFICATION
14. CHANGELOG
15. NEXT PHASE (Wait for confirmation)

---

## 60 Phase Summary Matrix

- **FAZ 0 — Araştırma ve Teknik Kararlar (COMPLETED)**
- **FAZ 1 — Proje Altyapısı (Cargo workspace, initial app window, logging, XDG paths)**
- **FAZ 2 — Dosya Sistemi Çekirdeği (Read dir, CRUD operations, metadata engine, edge cases)**
- **FAZ 3 — İlk Gerçek UI (HeaderBar, Sidebar, View Modes: Icon, Compact, Details)**
- **FAZ 4 — Navigation (Back/Forward/Up/Home, Breadcrumbs, Address mode, Keyboard shortcuts)**
- **FAZ 5 — Dosya İşlemleri Sistemi (Async Copy/Move/Delete/Trash, Progress UI, Conflict resolution)**
- **FAZ 6 — Context Menu (Dynamic right-click actions, Open With, Properties, Create Folder)**
- **FAZ 7 — Tabs (Multi-tab management, location history per tab, state isolation)**
- **FAZ 8 — Split View (Dual-panel navigation, active panel focus, cross-panel operations)**
- **FAZ 9 — Search Engine (SQLite FTS5, advanced filters, background indexing)**
- **FAZ 10 — File Preview (Right preview sidebar, images, text, markdown, JSON, PDF)**
- **FAZ 11 — Thumbnail Engine (Async thumbnail generation, dual-level LRU caching)**
- **FAZ 12 — Properties Window (Detailed metadata, permissions editor, checksums, symlink target)**
- **FAZ 13 — Sorting & Filtering (Multi-column sort, folders first, extension/type filtering)**
- **FAZ 14 — Hidden Files (Ctrl+H toggle, .hidden file support, visual distinction)**
- **FAZ 15 — Recent Files (XDG Recent files tracking, clear history, privacy mode)**
- **FAZ 16 — Favorites / Bookmarks (Sidebar drag & drop bookmarking, custom places)**
- **FAZ 17 — Devices & Volumes (GIO Volume Monitor, mount/unmount/eject, usage indicators)**
- **FAZ 18 — Trash Integration (Move to trash, restore to original location, empty trash)**
- **FAZ 19 — Archive Manager (ZIP, TAR, GZ, XZ, ZST, 7Z create/extract with traversal protection)**
- **FAZ 20 — Disk Analyzer (Interactive treemap/folder size usage, largest file discovery)**
- **FAZ 21 — Network Filesystems (GVfs SFTP, SMB, FTP, WebDAV remote location browser)**
- **FAZ 22 — Open With (System application association parser, default app selector)**
- **FAZ 23 — Terminal Integration (Open Terminal Here, XDG terminal configuration)**
- **FAZ 24 — Command Palette (Ctrl+K fuzzy action search modal)**
- **FAZ 25 — Keyboard-First Polish (Configurable shortcuts, full AT-SPI accessibility)**
- **FAZ 26 — Drag & Drop (Cross-window, cross-panel, desktop drag & drop copy/move/link)**
- **FAZ 27 — File Associations (MIME handling, default apps, desktop entry parser)**
- **FAZ 28 — Permissions & Privilege Escalation (Linux mode bits, Polkit/D-Bus isolated root helper)**
- **FAZ 29 — Security Audit & Hardening (TOCTOU, path traversal, symlink attack mitigations)**
- **FAZ 30 — Performance Optimization (Startup <350ms, 60-120 FPS UI loop hardening)**
- **FAZ 31 — Huge Directory Engine (100,000+ files virtualized scrolling & incremental metadata)**
- **FAZ 32 — File Operation Queue (Central operation manager, pause/resume/cancel, conflict matrix)**
- **FAZ 33 — Undo / Redo Engine (Ctrl+Z history for rename/move/copy/trash/create)**
- **FAZ 34 — Settings Subsystem (AdwPreferencesWindow, appearance, behavior, keybindings)**
- **FAZ 35 — Themes & Customization (Libadwaita styling, dark/light modes, custom accent colors)**
- **FAZ 36 — Accessibility Polish (AT-SPI screen reader audit, high contrast, focus indicators)**
- **FAZ 37 — Internationalization (gettext / fluent localization: English, Turkish, etc.)**
- **FAZ 38 — Plugin System (Sandboxed extension API with capability isolation)**
- **FAZ 39 — Developer Mode (Copy absolute path/URI, inode viewer, MIME inspector, checksums)**
- **FAZ 40 — Git Integration (Read-only repo status badges: modified, untracked, branch name)**
- **FAZ 41 — Checksum Generator (SHA-256, SHA-512, MD5 calculation workers)**
- **FAZ 42 — Duplicate Finder (Hash-based duplicate file detection with safe review UI)**
- **FAZ 43 — Smart Storage Dashboard (Disk health, space breakdown, storage insights)**
- **FAZ 44 — System Integration (Desktop entry, MIME handler, D-Bus service, notifications)**
- **FAZ 45 — Flatpak Packaging (Flatpak manifest, strict sandbox permissions, portal usage)**
- **FAZ 46 — Native Packaging (Arch PKGBUILD, Debian/Ubuntu .deb, Fedora .rpm manifests)**
- **FAZ 47 — Comprehensive Testing Suite (Unit, integration, property-based tests)**
- **FAZ 48 — Security Penetration Testing (Fuzzing, broken symlinks, corrupt archives)**
- **FAZ 49 — Performance Benchmarks (Directory load benchmarks, memory footprint tracking)**
- **FAZ 50 — Final UI/UX Audit (Pixel-perfect polish, empty states, loading indicators)**
- **FAZ 51 — Error UX & Recovery (User-friendly action-oriented error dialogs)**
- **FAZ 52 — Empty States & Guidance (Empty folder, search result, network location views)**
- **FAZ 53 — Privacy-Friendly Crash Reporting (Opt-in local crash logs, path sanitization)**
- **FAZ 54 — Tracing & Diagnostic Logging (Structured tracing subscriber with log rotation)**
- **FAZ 55 — Documentation Suite (API docs, user manual, plugin dev guide, contributing)**
- **FAZ 56 — CI/CD Pipeline (GitHub Actions fmt, clippy, test, multi-distro build matrix)**
- **FAZ 57 — Release Management (SemVer tagging, automated release notes generation)**
- **FAZ 58 — Final Comprehensive Audit (Architecture, Security, Perf, UI 0-100 scoring)**
- **FAZ 59 — Dolphin Feature Parity & Superiority Matrix (Dolphin vs Veyra comparison)**
- **FAZ 60 — Veyra 1.0 Release (Final production readiness certification)**
