# Contributing to Veyra

Thanks for considering a contribution. Veyra is a production-quality Linux
file manager, not a demo — see the ground rules in [AGENTS.md](AGENTS.md)
(60 development rules) before starting non-trivial work; this document
covers the mechanics of getting a change in.

By participating you agree to follow the
[Code of Conduct](CODE_OF_CONDUCT.md).

## Development environment

You need Rust 1.85+ and the GTK4/Libadwaita development headers.

```sh
# Arch
sudo pacman -S rust gtk4 libadwaita

# Fedora
sudo dnf install rust cargo gtk4-devel libadwaita-devel

# Debian / Ubuntu
sudo apt install cargo rustc libgtk-4-dev libadwaita-1-dev

# openSUSE
sudo zypper install cargo rust gtk4-devel libadwaita-devel
```

Then:

```sh
git clone https://github.com/ERAYQ1/Veyra-File-Manager.git
cd Veyra-File-Manager
cargo build --workspace
cargo run --bin veyra-app
```

See [docs/building.md](docs/building.md) for release builds, distro
packaging builds, and troubleshooting.

## Before opening a PR

Every change must pass all three, clean, with no exceptions:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Zero compiler/clippy warnings is a hard requirement (Kural #7/#9/#10), not a
style preference — CI (once Faz 56 lands) will reject anything that doesn't
pass all three.

Additional expectations, straight from [AGENTS.md](AGENTS.md):

- **No placeholder/TODO code** for core functionality. If something is
  intentionally partial, document it explicitly.
- **Never block the GTK main thread.** Filesystem, indexing, thumbnail,
  archive, and network work belongs on a background worker
  (`glib::spawn_future_local` + a channel back to the main context, or a
  `tokio` task) — see [docs/architecture.md](docs/architecture.md).
- **Add tests for filesystem-touching changes.** Unicode/special-character
  filenames, permission failures, and large-directory behavior are the
  edge cases most bugs hide in — see [docs/testing.md](docs/testing.md).
- **Keep crate boundaries decoupled** (`veyra-core` has no GTK dependency,
  `veyra-filesystem` has no UI dependency, etc.) — see the workspace layout
  in [docs/architecture.md](docs/architecture.md).
- **Never add a dependency stdlib or an existing workspace crate already
  covers.** New dependencies need a license check (GPL-3.0/LGPL/MIT/
  Apache-2.0 compatible) — see
  [docs/technology-decisions.md](docs/technology-decisions.md).
- **User-visible strings go through `i18n::t()`**, with both an `EN` and a
  `TR` table entry — see [docs/translation.md](docs/translation.md).

## Commit messages

Veyra follows [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <short summary>

<optional body — the "why", not a restatement of the diff>
```

Common types: `feat`, `fix`, `refactor`, `test`, `docs`, `perf`, `chore`.
Scope is usually the crate or subsystem touched, e.g. `feat(search): ...`,
`fix(filesystem): ...`, `docs(architecture): ...`.

## Pull request process

1. Fork the repository and branch off `main`.
2. Make your change, keeping it scoped — a bug fix doesn't need surrounding
   refactors, and a new feature doesn't need to land in one giant PR if it
   can be split into reviewable steps.
3. Run the three commands above locally; a PR with a red `fmt`/`clippy`/
   `test` won't be reviewed until it's green.
4. Update [CHANGELOG.md](CHANGELOG.md) if the change is user-visible.
5. Open the PR against `main` with a description of *why*, not just *what*
   — link any related issue.
6. Address review feedback with new commits (no force-push mid-review
   unless asked); a maintainer will squash or merge once approved.

## Reporting bugs

Open a [GitHub issue](https://github.com/ERAYQ1/Veyra-File-Manager/issues)
with reproduction steps, your distro/GTK4/Libadwaita versions, and — if
relevant — the sanitized contents of `$XDG_STATE_HOME/veyra/logs/veyra.log`
(paths and credentials are already redacted before being written to that
file, see [docs/security.md](docs/security.md)).

**Do not open a public issue for a security vulnerability** — see
[SECURITY.md](SECURITY.md) instead.

## Where things live

New to the codebase? Start with
[docs/architecture.md](docs/architecture.md) for the crate layout and
thread-boundary rules, then [docs/roadmap.md](docs/roadmap.md) for what's
built and what's planned (Veyra ships in numbered phases, "Faz 0"–"Faz
60").
