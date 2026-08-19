# Flatpak packaging

`io.github.erayq1.Veyra.json` is the Flatpak manifest (Faz 45). Sandbox
permission rationale lives in `docs/flatpak_permissions.md`, not here.

## Building

```sh
flatpak-builder --user --install --force-clean build-dir \
    build-aux/flatpak/io.github.erayq1.Veyra.json
```

Requires the `org.gnome.Platform`/`org.gnome.Sdk` (branch matching the
manifest's `runtime-version`) and the `org.freedesktop.Sdk.Extension.rust-stable`
extension installed from Flathub:

```sh
flatpak install flathub org.gnome.Platform//47 org.gnome.Sdk//47 \
    org.freedesktop.Sdk.Extension.rust-stable//47
```

## `cargo-sources.json`

`cargo` modules can't reach the network inside `flatpak-builder`'s sandboxed
build (by design — see `docs/flatpak_permissions.md`), so every crate
dependency has to be pre-declared as an offline-fetchable Flatpak source.
`cargo-sources.json` is that declaration: one entry per crate in
`Cargo.lock`, generated (not hand-written) by
[`flatpak-cargo-generator.py`](https://github.com/flatpak/flatpak-builder-tools/tree/master/cargo).

Regenerate it after any `Cargo.lock` change (new dependency, version bump):

```sh
python3 -m venv /tmp/fcg-venv && /tmp/fcg-venv/bin/pip install aiohttp tomlkit
curl -sL -o /tmp/flatpak-cargo-generator.py \
    https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
/tmp/fcg-venv/bin/python /tmp/flatpak-cargo-generator.py \
    Cargo.lock -o build-aux/flatpak/cargo-sources.json
```

`crates/veyra-ui/src/system_integration.rs`'s
`cargo_sources_is_valid_json_and_non_empty` test only checks the file is
well-formed and non-empty, not that it matches the current `Cargo.lock` byte
for byte — a stale (but valid) `cargo-sources.json` after a dependency bump
will still pass `cargo test` but fail an actual `flatpak-builder` run with a
missing-crate error, which is the real signal to regenerate.
