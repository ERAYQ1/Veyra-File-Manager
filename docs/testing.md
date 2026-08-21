# Testing Guide

Veyra's workspace carries 624 passing tests (`cargo test --workspace`,
verified against this document's own commit — re-run the count yourself
with the command below, since it grows every phase):

```sh
cargo test --workspace 2>&1 | grep "test result:"
```

Kural #48 ("never claim a feature is complete without testing it") and
Kural #34/#35/#36/#37 (test every filesystem operation, Unicode/unusual
filenames, permission failures, large directories, concurrent changes) are
the standing bar every phase is held to — `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test
--workspace` all have to be clean before a phase (or a PR) is considered
done.

## Running tests

```sh
cargo test --workspace                       # everything
cargo test -p veyra-filesystem                # one crate
cargo test -p veyra-filesystem --test crud     # one integration test file
cargo test unicode                            # by name substring, across crates
cargo test -- --nocapture                     # show println!/benchmark output
```

Tests that print timing numbers (the `benchmarks_*` files) are only
meaningful with `--nocapture`; see [benchmarks.md](benchmarks.md) for a
worked example of interpreting the output.

## Where tests live

- **Unit tests** live inline in each module (`#[cfg(test)] mod tests`),
  next to the code they cover — most of `veyra-ui`'s 278 tests are here,
  since a lot of UI logic (empty-state selection, error classification,
  i18n table completeness) is deliberately factored into plain functions
  that don't need a GTK main loop to test.
- **Integration tests** live in each crate's `tests/` directory and exercise
  the crate's public API end-to-end against a real, temporary filesystem:

  | File | Crate | Covers |
  | :--- | :--- | :--- |
  | `crud.rs` | `veyra-filesystem` | Create/read/update/delete/rename/copy/move |
  | `read_dir.rs` | `veyra-filesystem` | Directory listing, chunked reads |
  | `edge_cases.rs` | `veyra-filesystem` | Broken symlinks, permission-denied, disappearing files |
  | `symlinks.rs` | `veyra-filesystem` | Symlink targets, loops, dangling links |
  | `unicode_adversarial.rs` | `veyra-filesystem` | Unicode/RTL/control-character/emoji filenames |
  | `security.rs`, `security_adversarial.rs` | `veyra-filesystem` | Path traversal, TOCTOU, zip-slip-style boundary tests |
  | `huge_dir.rs`, `scaling.rs`, `benchmarks_scaling.rs` | `veyra-filesystem` | 100 to 1,000,000-entry directories |
  | `trash.rs` | `veyra-filesystem` | Move-to-trash / restore semantics |
  | `operations_lifecycle.rs` | `veyra-filesystem` | Queue, cancellation, pause/resume |
  | `query_syntax_search.rs` | `veyra-search` | FTS5 query parsing and matching |
  | `security_corrupt_index.rs` | `veyra-search` | Malformed/corrupt SQLite index recovery |
  | `benchmarks_search_latency.rs` | `veyra-search` | Search latency at scale |
  | `packaging_metadata.rs` | `veyra-app` | Version/path consistency across `Cargo.toml`, `.spec`, `PKGBUILD`, `debian/changelog`, `data/` |

## Adversarial and edge-case testing

Kural #35/#36/#37 aren't optional extras — most bugs in a file manager live
exactly here:

- **Unicode and unusual filenames**: RTL override characters, combining
  diacritics, emoji, embedded NUL-adjacent bytes where the OS allows them,
  filenames that are valid `OsString` but not valid UTF-8. See
  `unicode_adversarial.rs`.
- **Permission failures**: read-only directories, files owned by another
  UID, revoked mid-operation. Filesystem operations must degrade to a
  reported error, never a panic (Kural #15/#18).
- **Large directories**: `huge_dir.rs`/`scaling.rs` assert throughput floors
  (conservatively below what's actually measured — see
  [benchmarks.md](benchmarks.md) — to avoid CI flakiness on slower
  runners/disks) at 100 through 1,000,000 entries.
- **Concurrency**: files disappearing between listing and operating on
  them, directories mutating mid-scan — `edge_cases.rs` and
  `operations_lifecycle.rs`.

When adding a new filesystem-touching feature, add at minimum: one happy
path test, one Unicode-filename test, one permission-denied test, and one
"target disappeared mid-operation" test if the feature involves more than
one filesystem call.

## Temp directory hygiene

Integration tests use the `tempfile` crate (dev-dependency only — see
[technology-decisions.md](technology-decisions.md)) rather than fixed paths
under `/tmp`. Every `tempfile::tempdir()` is deleted automatically when it
drops at the end of the test, even on panic/assertion failure, so a test
run never leaves stray files on disk regardless of pass/fail outcome, and
tests can run in parallel (`cargo test`'s default) without colliding on
shared paths.

## GTK-dependent tests

`veyra-ui` widget construction generally can't be unit-tested directly —
Libadwaita requires a running GTK main loop and a display, which conflicts
with `cargo test`'s one-thread-per-test model. The convention used
throughout `veyra-ui` (see Faz 50/51/52's changelog entries) is to factor
the *decision logic* (which empty state to show, how to classify an error,
which recovery actions apply) into plain functions with no GTK types in
their signature, and unit-test those directly. Actual widget wiring is then
verified manually — see [running the app](../README.md#-hızlı-kurulum-ve-başlatma)
in the README.

## CI

There is no CI pipeline yet — GitHub Actions (`fmt`/`clippy`/`test` across
a distro build matrix) is planned for Faz 56. Until then, the three
commands above are run locally before every commit/PR is considered done.
