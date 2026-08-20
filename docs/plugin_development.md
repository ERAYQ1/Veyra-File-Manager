# Extending Veyra: Current Integration Points & the Planned Plugin System

**Status check first:** Veyra does not have a plugin/extension API yet. The
roadmap's Faz 38 ("Plugin System — Sandboxed extension API with capability
isolation") is planned but not implemented as of this document — see
[roadmap.md](roadmap.md). This document is honest about that split: what
you can hook into *today* without writing Veyra's own code, and what's
coming with Faz 38.

## What exists today: application-level integration

Veyra doesn't need a plugin API for the most common "extend the file
manager" use cases, because it integrates with two standard Linux
mechanisms instead of inventing its own:

### 1. "Open With" — MIME type associations

Right-click → *Open With* (`crates/veyra-ui/src/open_with.rs`,
`crates/veyra-ui/src/dialogs/open_with_dialog.rs`) lists every application
registered for a file's MIME type through the standard freedesktop.org
association mechanism (`gio::AppInfo::all_for_type`,
`.desktop`/`mimeapps.list`). **To add your own application to this menu**,
install a standard `.desktop` file with the right `MimeType=` entry — no
Veyra-specific code needed:

```ini
# ~/.local/share/applications/my-tool.desktop
[Desktop Entry]
Type=Application
Name=My Tool
Exec=my-tool %f
MimeType=text/x-log;
```

Then `update-desktop-database ~/.local/share/applications` (or just wait
for the next GIO cache refresh) makes it appear in Veyra's Open With
submenu and dialog immediately, alongside every other file manager on the
system (Nautilus, Dolphin, etc.) that reads the same association database.

### 2. Terminal integration

*Open Terminal Here* (Faz 23) never hardcodes a specific terminal emulator
(Kural #28) — it resolves through `xdg-terminal-exec` when present, falling
back to a small list of common terminals otherwise. Setting your preferred
terminal system-wide (`xdg-terminal-exec`'s own configuration, or your
desktop environment's default-terminal setting) changes what Veyra launches
without any Veyra-side configuration.

### 3. Context menu items that are *not* extensible today

The right-click menu (`crates/veyra-ui/src/context_menu.rs`) is currently a
fixed, compile-time-defined set of actions (Copy/Move/Rename/Trash/
Properties/Compress/Extract/Open With/…). There is no user-facing
mechanism yet to add a custom command to it — that is exactly the gap Faz
38's plugin system is scoped to close.

## Planned: Faz 38 Plugin System

Per [roadmap.md](roadmap.md) and Kural #43/#54 (decouple previews and
network functionality from the UI so they *can* eventually be
pluggable), the design direction for Faz 38 is:

- **Capability-scoped permissions**, not unrestricted raw disk access — a
  plugin declares what it needs (e.g. "read selected file paths", "add a
  context menu entry") and is denied everything else, consistent with
  Kural #20's no-unnecessary-privilege stance applied to third-party code.
- **Process isolation** over in-process dynamic loading — likely subprocess
  or sandboxed-runtime based, so a plugin crash or misbehavior can't take
  down the main `veyra-app` binary (Kural #14's "never block/crash the UI
  thread" extended to third-party code).
- **A defined action-registration surface** for context-menu additions,
  since that's the most requested extension point and the one the fixed
  menu above currently can't serve.

None of this is implemented yet. If you're looking to extend Veyra's
right-click behavior *right now*, the only supported path is the standard
`.desktop`-file MIME association mechanism described above, or
contributing a change directly to `context_menu.rs` — see
[CONTRIBUTING.md](../CONTRIBUTING.md).

This document will be rewritten with the actual plugin API, permission
manifest format, and a worked example once Faz 38 lands.
