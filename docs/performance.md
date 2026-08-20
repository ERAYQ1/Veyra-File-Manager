# Performance & Scaling

This document is the developer-facing entry point for Veyra's performance
work: what's *targeted* vs. what's *measured*. For the raw budget numbers
see [performance-budget.md](performance-budget.md); for the actual
benchmark run this document summarizes, see
[benchmarks.md](benchmarks.md) — that file states plainly it reflects one
measured run on tmpfs in a development environment, not a guaranteed
number on every disk/CPU combination.

## Directory scan throughput

`read_dir_chunked` (`veyra-filesystem`) streams a directory in 500-entry
batches over an async channel rather than collecting the whole listing
before the UI can paint anything — the measured throughput is flat across
five orders of magnitude:

| Entries | Time | Throughput |
| ---: | ---: | ---: |
| 100 | 0.87 ms | 114,959 files/sec |
| 1,000 | 5.44 ms | 183,988 files/sec |
| 10,000 | 61.5 ms | 162,603 files/sec |
| 100,000 | 609 ms | 164,190 files/sec |
| 1,000,000 | 5.92 s | 169,010 files/sec |

Flat throughput across scale is the important property here, not the
absolute number (tmpfs-backed, so real disk will be slower) — it confirms
there's no hidden O(n²) re-scan or unbounded buffering that would slow the
*rate* down as a directory grows. Reproduce with:

```sh
cargo test -p veyra-filesystem --test benchmarks_scaling -- --nocapture
```

## Lazy metadata: `FAST_ATTRIBUTES` vs `FULL_ATTRIBUTES`

The huge-directory listing path (`read_dir_chunked`) requests only name,
type, and size per entry (`FAST_ATTRIBUTES`) rather than the full GIO
attribute set (owner, permissions, inode, timestamps —
`FULL_ATTRIBUTES`, used by the non-chunked `read_dir` for smaller,
detail-view-ready listings). At 10,000 entries this is measured at ~2.5x
faster (52.53 ms vs. 132.99 ms) — detailed metadata for on-screen rows is
fetched lazily instead, per Kural #30's "huge directory virtualization"
requirement.

## Bounded memory

Scanning 100,000 entries in 500-item batches (discarding each batch after
the caller consumes it) shows no measurable RSS growth in the benchmark
run — direct evidence, not just batch-size bookkeeping, that Kural #33's
"bounded, not O(n), memory" requirement holds for the huge-directory path.
A single `Vec<FileItem>` holding all 100,000 entries at once would run to
tens of MB; the chunked path doesn't.

## Search latency (FTS5)

| Query shape | Result count | Latency |
| :--- | ---: | ---: |
| Narrow, exact match | 1 | 236 µs |
| Broad (common substring, 500-row cap) | 500 | 14.13 ms |
| Miss | 0 | 150 µs |

A broad query costs more because SQLite still ranks every matching row
before `LIMIT` truncates the result set — the realistic worst case for
"typed one common letter," not the typical case, and still comfortably
under the <100ms "feels instant" threshold. Indexing 10,000 entries through
the one-transaction-per-call `index_entry` API took 22.9s in the same run
— a background-indexer cost (runs on a low-priority thread, never blocks
search or the UI), flagged as a concrete target for a future batched-insert
API rather than something the search *query* path needs to absorb.

## Thumbnail cache

The L1 in-memory LRU cache (`veyra-ui::thumbnails`) measures ~162 ns/`get`
and ~848 ns/`put` at 100,000 operations — the `put` cost is a `PathBuf`
clone on insert, not a cache-structure cost, and both are far below any
threshold that matters for a single 16ms UI frame budget.

## Copy/move throughput

`move_entry` on the same filesystem is a rename (51 µs regardless of file
size — 50 MB measured). `copy` reflects the backing storage's raw
throughput (2,503 MB/sec on tmpfs in the benchmark run; a real disk will
be substantially slower, which is why the test assertion floor is a
conservative 5 MB/sec rather than anything close to the tmpfs number).

## Where the budgets come from

[performance-budget.md](performance-budget.md) sets the *targets* (cold
start < 350ms, idle RAM < 50MB, 60–120 FPS, directory read latency by
scale) this benchmark run is checked against — read that file for the
budget table itself, and [benchmarks.md](benchmarks.md) for the full
methodology notes (including the one deliberate mismatch: the 100,000-file
budget describes *first-batch-visible* latency for a streaming UI, while
the raw scan-throughput benchmark measures the *entire* scan to
completion — both numbers are documented together there to avoid
confusing the two in a future regression check).

## Reproducing these numbers yourself

```sh
cargo test -p veyra-filesystem --test benchmarks_scaling -- --nocapture
cargo test -p veyra-search --test benchmarks_search_latency -- --nocapture
cargo test -p veyra-ui --lib thumbnails:: -- --nocapture
```

Numbers will differ from [benchmarks.md](benchmarks.md) based on your
CPU, disk (tmpfs vs. spinning disk vs. SSD), and system load — the test
assertions themselves use conservative floors specifically so they don't
flake on slower hardware or CI runners; only the numbers *printed* via
`--nocapture` are the interesting comparison point, not the pass/fail
result.

## Background task scheduling

The search indexer's background thread (`veyra-search::indexer::
lower_priority`) calls `libc::nice(19)` — the lowest standard Unix
niceness — on itself before indexing, so a sustained background index of a
large tree never competes with interactive work for CPU time. This is
implemented for the indexer specifically today; the performance-budget
target of extending the same treatment (plus I/O-priority scheduling) to
thumbnail generation and checksum calculation is not yet implemented — see
[performance-budget.md](performance-budget.md) for the full target and
Kural #32/#39 for the underlying rule.
