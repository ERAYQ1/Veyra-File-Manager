# Building Veyra

This covers building from a source checkout for development. For
distro-package builds (Arch/Fedora/openSUSE/Debian) and the Flatpak
manifest, see [packaging.md](packaging.md).

## System dependencies

Veyra needs Rust 1.85+ (the workspace's `rust-version`) and the GTK4 /
Libadwaita development headers — GIO/GLib come bundled with GTK4's dev
package on every distro below.

| Distro | Command |
| :--- | :--- |
| Arch Linux | `sudo pacman -S rust gtk4 libadwaita` |
| Fedora / RHEL | `sudo dnf install rust cargo gtk4-devel libadwaita-devel` |
| Debian / Ubuntu | `sudo apt install cargo rustc libgtk-4-dev libadwaita-1-dev pkg-config` |
| openSUSE | `sudo zypper install cargo rust gtk4-devel libadwaita-devel` |

Optional runtime integrations that aren't build-time dependencies but are
worth having installed while developing: `polkit` (privileged operations),
`gvfs` (network filesystems, trash), `xdg-terminal-exec` (Faz 23's terminal
launcher).

## Debug build

```sh
git clone https://github.com/ERAYQ1/Veyra-File-Manager.git
cd Veyra-File-Manager
cargo build --workspace
cargo run --bin veyra-app
```

A debug build defaults its log level to `DEBUG` for every Veyra crate (see
[docs/architecture.md](architecture.md) and
`crates/veyra-app/src/logging.rs::default_level_filter`); override with
`RUST_LOG=veyra=trace` etc. for finer control.

## Release build

```sh
cargo build --workspace --release
./target/release/veyra
```

The release profile (`Cargo.toml`'s `[profile.release]`) enables `lto =
true` and `codegen-units = 1` for maximum runtime performance at the cost
of a slower build — expect several minutes on a first release build of the
full workspace.

## Installing to your system

```sh
make                    # cargo build --release --workspace
sudo make install       # PREFIX=/usr by default
sudo make uninstall     # removes exactly what install placed
```

Override `PREFIX` for a user-local, no-`sudo` install
(`make install PREFIX=$HOME/.local`) or `DESTDIR` to stage into a package
build root. See the `Makefile` at the repo root for the exact file list
`install` places.

## Workspace layout

```
Cargo.toml                # workspace manifest, shared [workspace.dependencies]
crates/
├── veyra-core             # models, config, logging, crash reporting — no GTK dependency
├── veyra-filesystem       # GIO/GVfs abstraction, operation queue, undo engine
├── veyra-search           # SQLite + FTS5 search/indexing — no UI dependency
├── veyra-ui                # GTK4 + Libadwaita widgets, views, dialogs, i18n
└── veyra-app                # binary entry point, CLI, D-Bus single-instance, panic hook
```

Building a single crate (faster iteration on `veyra-filesystem`/
`veyra-search` logic, which has no GTK dependency and no display needed):

```sh
cargo build -p veyra-filesystem
cargo test -p veyra-search
```

## Common build issues

- **`error: failed to run custom build command for 'gtk4-sys'` /
  `Package gtk4 was not found`** — the GTK4/Libadwaita `-dev`/`-devel`
  package for your distro (table above) is missing, or `pkg-config` can't
  see it. Confirm with `pkg-config --modversion gtk4 libadwaita-1`.
- **Wrong GTK4/Libadwaita version** — Veyra targets a GNOME 47-era
  Libadwaita (see [flatpak_permissions.md](flatpak_permissions.md) and the
  Flatpak manifest's runtime version). Older distro packages may be missing
  APIs Veyra calls; check your distro's GTK4/Libadwaita package version
  against the Flatpak manifest's `runtime-version` first.
- **Slow release builds** — `lto = true` + `codegen-units = 1` trade build
  time for runtime speed; use a plain `cargo build` (debug) while iterating
  and only build `--release` when you need to measure performance or
  package.
