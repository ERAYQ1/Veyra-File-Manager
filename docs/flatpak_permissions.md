# Flatpak Sandbox Permissions

Faz 45. This document justifies every entry in
`build-aux/flatpak/io.github.erayq1.Veyra.json`'s `finish-args` — Veyra's
complete Flatpak sandbox permission grant. The rule driving every choice
below is Kural #20 ("no root default; isolate privileged operations"): grant
the smallest permission set that lets Veyra function as a real file manager,
and lean on an XDG portal instead of a raw permission wherever one exists
that covers the same need.

## The permission set

| `finish-args` entry | Why it's needed | What it does *not* grant |
| --- | --- | --- |
| `--share=ipc` | Shared System V/POSIX IPC segment with the host, required by GTK4's software-rendering and font-cache paths (Pango/Fontconfig, some GL drivers) to function inside the sandbox's private IPC namespace. | No filesystem or network access by itself. |
| `--socket=fallback-x11` | Lets Veyra draw its window on an X11 display when Wayland's own socket (below) isn't available — the standard "X11 as fallback, not primary" pattern GNOME apps use so Veyra still runs under a plain Xorg session or through XWayland. | Does not grant direct access to *other* clients' windows; GTK talks to the X server for its own window only. |
| `--socket=wayland` | The primary, preferred windowing socket — every GNOME session offers this, and it's what actually gets used whenever it's present (`fallback-x11` only kicks in on non-Wayland setups). | Same isolation as any other Wayland client: no access to other clients' surfaces. |
| `--filesystem=host` | The one broad grant this manifest makes, and the reason it's called out by name rather than folded into a table row: **a file manager's entire job is browsing, moving, and modifying arbitrary user-chosen paths** — the root filesystem, every mounted drive, every user directory. A narrower `--filesystem=home` would silently break browsing `/mnt`, `/media`, or a second internal drive, which are exactly the locations a file manager (unlike a text editor or an image viewer) is expected to reach. This is the deliberate, well-understood trade-off every Flatpak-packaged file manager (Nautilus, Dolphin, Files) makes for the same reason. | Does **not** grant root/privileged filesystem access — Veyra still refuses to run as UID 0 (`root_guard`, Kural #20) and every privileged operation (`crates/veyra-ui/src/privileged.rs`) still goes through a separate `pkexec`/Polkit helper outside this sandbox grant, not through broadened `--filesystem` scope. |
| `--talk-name=org.freedesktop.FileManager1` | Lets Veyra *own* (register as) the `org.freedesktop.FileManager1` D-Bus name other applications call into for "Show file in file manager"/"Show items" requests — the standard cross-desktop file-manager D-Bus contract. | Does not let Veyra call *other* services under this name; it's the interface Veyra implements, not one it consumes. |
| `--talk-name=org.freedesktop.Notifications` | Desktop notifications for completed background operations (Faz 44's `notify_operation_complete`) — "Copied 1,200 files" while the window isn't focused. Talking to this name is also how `gio::Notification` reaches the host notification daemon from inside a sandbox in the first place. | No access to *other* apps' notifications; only the ability to post Veyra's own. |
| `--talk-name=org.gtk.vfs.*` | GVfs (`crates/veyra-ui/src/network.rs`'s `sftp://`/`smb://`/network-mount support, and GIO's own trash/thumbnail backends) runs as a set of session D-Bus services outside the sandbox; this wildcard lets Veyra reach the GVfs daemons it already depends on for every non-local `VeyraPath::Uri`. | Does not grant filesystem access beyond what GVfs itself mediates — a remote mount still goes through GVfs's own authentication/mounting flow, not a bypass. |
| `--system-talk-name=org.freedesktop.UDisks2` | Drive/partition/USB device enumeration for the sidebar's "Devices" section (`crates/veyra-ui/src/devices.rs`) — UDisks2 is a *system* bus service (hence `--system-talk-name`, not `--talk-name`), since removable-media detection is a privileged, host-wide concern GVfs alone doesn't cover. | Read-only enumeration/mount-request access through UDisks2's own Polkit-gated API — not a blanket system-bus grant, and not root. |

Every other Flatpak permission (`--share=network`, `--device=all`,
`--filesystem=host-os`, arbitrary `--talk-name`s, etc.) is deliberately
**absent**. Veyra makes no network calls of its own (Kural #24 — zero
telemetry — plus no reason for direct network I/O once GVfs handles remote
mounts), needs no GPU device passthrough beyond the windowing sockets above,
and needs no access to the host's `/usr`/`/etc` beyond what `--filesystem=host`
already covers for browsing purposes.

## Role of XDG portals

A sandboxed app talking to `org.freedesktop.portal.Desktop` never needs a
`finish-args` entry for the portal bus name itself — `xdg-desktop-portal` is
always reachable from inside every Flatpak sandbox by design, precisely so
apps *don't* need to request broad permissions for things a portal can do
safely on their behalf. Veyra relies on portal-transparent behavior already
built into the GLib/GIO APIs it uses, rather than calling any portal D-Bus
interface directly:

- **Opening a file in an external application** (`crates/veyra-ui/src/open_with.rs`,
  the "Open With" dialog): `gio::AppInfo::launch` detects the sandbox itself
  (the same `/.flatpak-info` signal `system_integration::is_flatpak_sandbox`
  checks for diagnostics) and transparently routes through the
  `org.freedesktop.portal.OpenURI` portal instead of forking the target
  application directly — which a sandboxed process cannot do to a host
  binary regardless of any `finish-args` grant. No manual portal call is
  written in Veyra's own code; GIO's `GDesktopAppInfo` backend already does
  this for every caller.
- **Desktop notifications**: `gio::Notification` (used by Faz 44's
  `notify_operation_complete`) is likewise sandbox-aware and posts through
  `org.freedesktop.portal.Notification` instead of the plain
  `org.freedesktop.Notifications` name whenever GIO detects it's running
  inside Flatpak — `--talk-name=org.freedesktop.Notifications` above is the
  fallback for the non-sandboxed (native/distro-package) build, which has no
  portal layer to fall back on.
- **Default-application queries/registration**
  (`system_integration::is_default_file_manager`/
  `set_as_default_file_manager`, Faz 44): `gio::AppInfo::default_for_type`/
  `set_as_default_for_type` read and write through the same portal-mediated
  `mimeapps.list` machinery in both the sandboxed and native case — no
  Veyra-side branching needed.

`system_integration::is_flatpak_sandbox()` exists for diagnostics (logged
once at startup) and as a hook for future code that has a genuine reason to
know its sandbox status — not to gate any of the behavior above, all of
which is already correct in both environments without it.

## Verifying this document stays in sync

`crates/veyra-ui/src/system_integration.rs`'s
`manifest_finish_args_match_the_documented_minimal_set` test asserts the
manifest's `finish-args` array is *exactly* the eight entries this document
covers (no more, no fewer) — a future permission addition or removal that
doesn't update both the manifest and this document fails that test.
