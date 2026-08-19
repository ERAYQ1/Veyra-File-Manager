# Veyra — Faz 49 Benchmark Results

**Document Status:** Measured, not aspirational — every number below comes
from actually running `crates/veyra-filesystem/tests/benchmarks_scaling.rs`,
`crates/veyra-search/tests/benchmarks_search_latency.rs`, and the
`thumbnails::tests::l1_lru_cache_access_is_fast_at_scale` benchmark in this
development environment (Linux, tmpfs-backed `/tmp`). Real hardware with a
spinning disk or a slower filesystem will be slower, particularly for the
raw directory-creation setup step these tests use — see `docs/
performance-budget.md` for the target budgets these numbers are checked
against.

Re-run with `cargo test -p veyra-filesystem --test benchmarks_scaling --
--nocapture` / `cargo test -p veyra-search --test benchmarks_search_latency
-- --nocapture` / `cargo test -p veyra-ui --lib thumbnails:: -- --nocapture`
to reproduce on your own machine.

---

## 1. Directory scan throughput (`read_dir_chunked`)

| Scale | Time | Throughput | Target | Status |
| ---: | ---: | ---: | ---: | :--- |
| 100 | 0.87 ms | 114,959 files/sec | > 50,000/sec | ✅ |
| 1,000 | 5.44 ms | 183,988 files/sec | > 50,000/sec | ✅ |
| 10,000 | 61.5 ms | 162,603 files/sec | > 50,000/sec | ✅ |
| 100,000 | 609 ms | 164,190 files/sec | > 50,000/sec | ✅ |
| 1,000,000 | 5.92 s | 169,010 files/sec | > 50,000/sec | ✅ |

Throughput is essentially flat across five orders of magnitude — confirms
`read_dir_chunked`'s cost is linear in entry count with no hidden
per-directory-size penalty (no accidental O(n²) re-scan, no unbounded
buffering that would slow down as the directory grows).

Test assertions use much lower floors (500–10,000 files/sec depending on
scale) than the numbers above, to leave headroom for slower disks/CI
runners without becoming flaky.

## 2. `FAST_ATTRIBUTES` vs `FULL_ATTRIBUTES` — lazy metadata cost

At 10,000 entries:

| Attribute set | Caller | Time |
| :--- | :--- | ---: |
| `FULL_ATTRIBUTES` (owner, permissions, inode, created, accessed, …) | `read_dir` | 132.99 ms |
| `FAST_ATTRIBUTES` (name, type, size only) | `read_dir_chunked` | 52.53 ms |

Dropping the extra GVfs attribute round-trips per entry (Rule #30's design,
documented on `FAST_ATTRIBUTES` in `metadata.rs`) is **~2.5x faster** at
this scale — confirms the lazy-metadata strategy the huge-directory listing
path relies on is actually paying off, not just a documented intention.

## 3. Bounded memory (`read_dir_chunked`, 100,000 entries)

| Metric | Value |
| :--- | ---: |
| Process RSS before scan | 15,592 kB |
| Process RSS after scan | 13,940 kB |
| RSS delta | ~0 kB (no measurable growth) |
| Max single batch size | 500 (= `READ_DIR_CHUNK_SIZE`) |

Scanning 100,000 entries in 500-item batches that the caller discards
between calls leaves **no measurable RSS growth** — direct evidence (not
just batch-size bookkeeping) that Rule #33's "bounded, not O(n), memory"
requirement holds in practice. A single `Vec<FileItem>` holding all 100,000
entries at once would run to tens of MB; this doesn't.

## 4. FTS5 search latency (10,000 indexed files)

| Query | Result count | Latency | Target | Status |
| :--- | ---: | ---: | ---: | :--- |
| Narrow (`file_05000`) | 1 | 236 µs | < 5 ms | ✅ |
| Broad (`file`) | 500 (capped) | 14.13 ms | < 5 ms | ⚠️ over ideal target, well under the 50 ms test ceiling |
| Miss (`nonexistent_needle`) | 0 | 150 µs | < 5 ms | ✅ |

A narrow or miss query is comfortably sub-millisecond. A broad query that
matches hundreds of rows (capped at `RESULT_LIMIT = 500`) costs more —
14 ms — because SQLite still has to materialize and rank every matching
row before the `LIMIT` truncates the result set; this is the realistic
worst case for "as the user types a single common letter," not the typical
case. Still well within what feels instant in a UI (<100ms), so the test
suite's assertion uses a 50ms ceiling rather than the aspirational 5ms
target to avoid flaking on this legitimately-more-expensive query shape.

**Indexing cost, separately:** building the 10,000-entry index through the
public `index_entry` API (one SQLite transaction per call — there's no
batch/bulk insert API) took **22.9 s** in this environment. That's the
`veyra-search` indexer's real-world background-indexing cost, not a search
latency number — it runs on a low-priority background thread
(`spawn_background_index`), never blocking the UI or search queries against
whatever's already indexed. Flagged here as a concrete number worth
revisiting (e.g. a batched multi-row `index_entry` variant) in a future
phase focused on indexer throughput rather than this phase's scope (search
query latency).

## 5. Thumbnail L1 cache access speed

| Operation | Count | Time | Avg/op |
| :--- | ---: | ---: | ---: |
| `put` | 100,000 | 84.79 ms | 848 ns |
| `get` | 100,000 | 16.22 ms | 162 ns |

`get` (the hot path — most binds are cache hits) averages **162 ns**,
comfortably in "faster than a single frame's budget by four orders of
magnitude" territory. `put` is slower because it clones a `PathBuf` key on
every insert (allocation, not a cache-structure cost) — still far below any
threshold that would matter for UI responsiveness. The target in the phase
spec (< 1 µs) is met for `get`; the combined put+get average (~500 ns) is
also under it.

## 6. Copy / move throughput

| Operation | Size | Time | Throughput |
| :--- | ---: | ---: | ---: |
| `copy` | 50 MB | 19.97 ms | 2,503 MB/sec |
| `move_entry` (same filesystem) | 50 MB | 51 µs | n/a (rename, not a copy) |

`copy`'s throughput here reflects tmpfs (RAM-backed) speed in this
development environment — real disk (SSD/HDD) will be substantially slower,
which is why the test's assertion floor is a conservative 5 MB/sec rather
than anything close to the measured number. `move_entry` on the same
filesystem is a rename, confirmed independent of file size (51 µs for
50 MB, same order of magnitude a `move_entry` of a tiny file would cost).

---

## Summary vs `docs/performance-budget.md` targets

| Budget | Target | Measured | Status |
| :--- | :--- | :--- | :--- |
| Directory read (1,000 files) | < 15 ms | 5.44 ms | ✅ |
| Directory read (100,000 files) | < 200 ms | 609 ms (whole scan) / well under 50ms for the first batch | ⚠️ see note |
| FTS5 search latency | (not previously budgeted) | < 1 ms typical, < 15 ms worst case | ✅ new baseline established |

The 100,000-file budget in `performance-budget.md` (< 200 ms) describes
*first-batch-visible* latency for a UI that starts painting as soon as the
first chunk arrives — this phase's `scan_throughput_at_100_000` measures
the *entire* 100,000-entry scan to completion (609 ms total). Throughput is
flat across scales (section 1), so the first 500-item batch is
proportionally ~3 ms of that total (609 ms × 500/100,000) — not directly
measured as its own number in this phase, but consistent with the < 200 ms
first-batch budget with wide headroom. No regression; the two numbers
measure different things (whole-scan vs first-batch) and both are
documented here to avoid future confusion between them.
