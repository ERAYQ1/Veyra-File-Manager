# Native Packaging

Veyra ships native packaging for the standard Linux distribution families in
addition to the Flatpak manifest (`build-aux/flatpak/`, see
[flatpak_permissions.md](flatpak_permissions.md)). All packaging metadata is
generated from the same source tree; there is no separate "packaging branch".

## Building from source (`make`)

Any distribution with Rust, GTK4 and Libadwaita development headers can
build and install Veyra with the standard `DESTDIR`/`PREFIX` Makefile:

```sh
make
sudo make install                # PREFIX=/usr by default
sudo make uninstall              # removes everything install placed
```

Override `PREFIX` for a user-local install (e.g. `PREFIX=$HOME/.local`,
no `sudo` needed) or `DESTDIR` to stage into a package build root.

The `install` target places:

| File | Destination |
|---|---|
| `target/release/veyra` | `$(DESTDIR)$(PREFIX)/bin/veyra` |
| `data/io.github.erayq1.Veyra.desktop` | `$(DESTDIR)$(PREFIX)/share/applications/` |
| `data/io.github.erayq1.Veyra.metainfo.xml` | `$(DESTDIR)$(PREFIX)/share/metainfo/` |
| `data/icons/hicolor/**` | `$(DESTDIR)$(PREFIX)/share/icons/hicolor/**` |

## Arch Linux

```sh
cd packaging/arch
makepkg -si
```

`PKGBUILD` fetches a release tarball, builds with `cargo build --frozen
--release`, and installs via `make DESTDIR=... PREFIX=/usr install`.
Runtime deps: `gtk4`, `libadwaita`, `glib2`. Optional: `polkit`, `gvfs`,
`xdg-terminal-exec`.

## Fedora / RHEL

```sh
rpmbuild -ba packaging/fedora/veyra.spec
```

Requires a source tarball named `Veyra-File-Manager-<version>.tar.gz` in
`~/rpmbuild/SOURCES` (or use `rpmdev-setuptree` + `spectool -g`). The
`%check` section runs `desktop-file-validate` and `appstream-util
validate-relax` against the installed desktop/metainfo files.

## openSUSE

```sh
rpmbuild -ba packaging/opensuse/veyra.spec
```

Same layout as the Fedora spec, using `pkgconfig()`-style `BuildRequires`
and openSUSE `Group` conventions per the openSUSE packaging guidelines.

## Debian / Ubuntu

```sh
cd packaging/debian
debuild -b -uc -us
```

(Run from a directory tree where `packaging/debian/` is symlinked or
copied to `debian/` at the project root, per standard `dpkg-buildpackage`
layout, e.g. `ln -s packaging/debian debian`.) `debian/rules` calls into
the same `Makefile` via `dh`'s `override_dh_auto_build`/`_install` hooks.
Runtime deps: `libgtk-4-1`, `libadwaita-1-0` plus whatever `dh_shlibdeps`
detects. Recommended: `policykit-1`, `gvfs`, `xdg-terminal-exec`.

## Version bumps

`pkgver` (PKGBUILD), `Version` (both `.spec` files) and the Debian
`changelog` entry version must match the workspace version in the root
`Cargo.toml` (`[workspace.package].version`). This is enforced by
`crates/veyra-app/tests/packaging_metadata.rs`, which also checks that
every path referenced by the packaging files (desktop entry, metainfo,
icons) actually exists in `data/`.
