//! Faz 49: performance benchmarks at 100 / 1,000 / 10,000 / 100,000 /
//! 1,000,000-entry scale. Faz 30's `scaling.rs` already proves *correctness*
//! at these scales (every entry seen exactly once, no batch ever exceeding
//! `chunk_size`); this file measures and asserts *performance*: directory
//! scan throughput, `FAST_ATTRIBUTES` vs `FULL_ATTRIBUTES` metadata cost,
//! bounded-memory scanning (via real process RSS, not just batch-size
//! bookkeeping), and copy/move throughput (Rule #30-#33).
//!
//! Thresholds are deliberately far below what this suite actually measures
//! in development (see `docs/benchmarks.md` for real numbers) — generous
//! enough to survive a slower disk or a loaded CI runner without becoming
//! flaky, while still catching an actual regression (e.g. an accidental
//! O(n²) loop, or a batch that silently buffers the whole directory).

use std::fs::File;
use std::io::Write;
use std::time::{Duration, Instant};

use veyra_filesystem::{
    copy, move_entry, read_dir, read_dir_chunked, OperationControl, VeyraPath, READ_DIR_CHUNK_SIZE,
};

fn make_dir_with_files(count: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..count {
        File::create(dir.path().join(format!("file-{i:07}.txt"))).unwrap();
    }
    dir
}

fn scan_throughput(path: &VeyraPath, chunk_size: usize) -> (usize, Duration) {
    let control = OperationControl::new();
    let mut total = 0usize;
    let start = Instant::now();
    read_dir_chunked(path, chunk_size, &control, |chunk| total += chunk.len()).unwrap();
    (total, start.elapsed())
}

/// Current process RSS in kB, parsed from `/proc/self/status` — no new
/// dependency needed (Linux-only, matching Veyra's own platform scope) for
/// a real memory measurement rather than inferring boundedness only from
/// batch-size bookkeeping.
fn rss_kb() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|rest| rest.trim().trim_end_matches("kB").trim().parse().ok())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------
// A. Directory scan throughput at each scale.
// ---------------------------------------------------------------------

#[test]
fn scan_throughput_at_100() {
    let dir = make_dir_with_files(100);
    let (total, elapsed) = scan_throughput(&VeyraPath::from_local(dir.path()), READ_DIR_CHUNK_SIZE);
    assert_eq!(total, 100);
    let files_per_sec = total as f64 / elapsed.as_secs_f64().max(0.000_001);
    println!("scan_throughput_at_100: {elapsed:?} ({files_per_sec:.0} files/sec)");
    assert!(
        files_per_sec > 500.0,
        "100-entry scan should comfortably exceed 500 files/sec, got {files_per_sec:.0}"
    );
}

#[test]
fn scan_throughput_at_1_000() {
    let dir = make_dir_with_files(1_000);
    let (total, elapsed) = scan_throughput(&VeyraPath::from_local(dir.path()), READ_DIR_CHUNK_SIZE);
    assert_eq!(total, 1_000);
    let files_per_sec = total as f64 / elapsed.as_secs_f64().max(0.000_001);
    println!("scan_throughput_at_1_000: {elapsed:?} ({files_per_sec:.0} files/sec)");
    assert!(files_per_sec > 2_000.0);
}

#[test]
fn scan_throughput_at_10_000() {
    let dir = make_dir_with_files(10_000);
    let (total, elapsed) = scan_throughput(&VeyraPath::from_local(dir.path()), READ_DIR_CHUNK_SIZE);
    assert_eq!(total, 10_000);
    let files_per_sec = total as f64 / elapsed.as_secs_f64().max(0.000_001);
    println!("scan_throughput_at_10_000: {elapsed:?} ({files_per_sec:.0} files/sec)");
    assert!(files_per_sec > 5_000.0);
}

#[test]
fn scan_throughput_at_100_000() {
    let dir = make_dir_with_files(100_000);
    let (total, elapsed) = scan_throughput(&VeyraPath::from_local(dir.path()), READ_DIR_CHUNK_SIZE);
    assert_eq!(total, 100_000);
    let files_per_sec = total as f64 / elapsed.as_secs_f64().max(0.000_001);
    println!("scan_throughput_at_100_000: {elapsed:?} ({files_per_sec:.0} files/sec)");
    assert!(files_per_sec > 10_000.0);
}

#[test]
fn scan_throughput_at_1_000_000() {
    // The full 1M-scale requirement. Real files, not simulated — confirmed
    // tractable in development (~2s to create + ~5s to scan on tmpfs; see
    // docs/benchmarks.md) and kept as a normal (non-`#[ignore]`) test since
    // it stays well within a reasonable suite-wide time budget.
    let dir = make_dir_with_files(1_000_000);
    let (total, elapsed) = scan_throughput(&VeyraPath::from_local(dir.path()), READ_DIR_CHUNK_SIZE);
    assert_eq!(total, 1_000_000);
    let files_per_sec = total as f64 / elapsed.as_secs_f64().max(0.000_001);
    println!("scan_throughput_at_1_000_000: {elapsed:?} ({files_per_sec:.0} files/sec)");
    assert!(
        files_per_sec > 10_000.0,
        "1M-entry scan should exceed 10,000 files/sec even on modest hardware, got {files_per_sec:.0}"
    );
}

// ---------------------------------------------------------------------
// B. FAST_ATTRIBUTES vs FULL_ATTRIBUTES lazy-metadata cost.
// ---------------------------------------------------------------------

#[test]
fn fast_attributes_scan_is_not_slower_than_full_attributes_listing_at_10_000() {
    // `read_dir` requests the full attribute set (owner/permissions/inode/
    // created/accessed) for every entry; `read_dir_chunked` requests only
    // what a listing row needs to paint (Rule #30's lazy-metadata design —
    // see `FAST_ATTRIBUTES`'s doc comment in `metadata.rs`). The full set
    // costs real extra GVfs round-trips per entry, so FAST must never come
    // out slower — a generous 1.5x margin absorbs run-to-run noise without
    // masking an actual regression back to eager full-metadata cost.
    let dir = make_dir_with_files(10_000);
    let path = VeyraPath::from_local(dir.path());

    let start = Instant::now();
    let full = read_dir(&path).unwrap();
    let full_elapsed = start.elapsed();
    assert_eq!(full.len(), 10_000);

    let control = OperationControl::new();
    let start = Instant::now();
    let mut fast_total = 0usize;
    read_dir_chunked(&path, READ_DIR_CHUNK_SIZE, &control, |chunk| {
        fast_total += chunk.len()
    })
    .unwrap();
    let fast_elapsed = start.elapsed();
    assert_eq!(fast_total, 10_000);

    println!(
        "FULL_ATTRIBUTES read_dir: {full_elapsed:?}, FAST_ATTRIBUTES read_dir_chunked: {fast_elapsed:?}"
    );
    assert!(
        fast_elapsed.as_secs_f64() <= full_elapsed.as_secs_f64() * 1.5,
        "FAST_ATTRIBUTES listing ({fast_elapsed:?}) should not be slower than \
         FULL_ATTRIBUTES ({full_elapsed:?}) beyond noise margin"
    );
}

// ---------------------------------------------------------------------
// C. Bounded memory (real process RSS, not just batch bookkeeping).
// ---------------------------------------------------------------------

#[test]
fn process_rss_growth_stays_bounded_scanning_100_000_entries() {
    let dir = make_dir_with_files(100_000);
    let path = VeyraPath::from_local(dir.path());
    let control = OperationControl::new();

    let before_kb = rss_kb();
    let mut max_batch = 0usize;
    let mut total = 0usize;
    read_dir_chunked(&path, READ_DIR_CHUNK_SIZE, &control, |chunk| {
        max_batch = max_batch.max(chunk.len());
        total += chunk.len();
    })
    .unwrap();
    let after_kb = rss_kb();

    assert_eq!(total, 100_000);
    assert!(max_batch <= READ_DIR_CHUNK_SIZE);
    let delta_kb = after_kb.saturating_sub(before_kb);
    println!(
        "process_rss_growth_stays_bounded_scanning_100_000_entries: before={before_kb}kB after={after_kb}kB delta={delta_kb}kB"
    );
    // A 100,000-`FileItem` `Vec` held all at once would run to tens of MB
    // (each carries several `String`/`PathBuf`/`Option` fields); streaming
    // in `READ_DIR_CHUNK_SIZE`-sized batches that the caller discards
    // between calls (as this test does) should leave RSS growth in the low
    // single-digit MB at most — 50 MB is a generous ceiling that would
    // still catch a regression back to buffering the whole directory.
    assert!(
        delta_kb < 50_000,
        "RSS grew by {delta_kb}kB scanning 100,000 entries in {READ_DIR_CHUNK_SIZE}-sized \
         batches — memory use should stay bounded, not scale with directory size"
    );
}

// ---------------------------------------------------------------------
// D. Copy / move throughput.
// ---------------------------------------------------------------------

#[test]
fn copy_throughput_of_a_50mb_file_meets_a_minimum_bound() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.bin");
    {
        let mut file = File::create(&source).unwrap();
        let chunk = vec![0x5Au8; 1024 * 1024];
        for _ in 0..50 {
            file.write_all(&chunk).unwrap();
        }
    }
    let dest = dir.path().join("dest.bin");

    let start = Instant::now();
    copy(
        &VeyraPath::from_local(&source),
        &VeyraPath::from_local(&dest),
        false,
    )
    .unwrap();
    let elapsed = start.elapsed();

    assert_eq!(std::fs::metadata(&dest).unwrap().len(), 50 * 1024 * 1024);
    let mb_per_sec = 50.0 / elapsed.as_secs_f64().max(0.000_001);
    println!("copy_throughput_of_a_50mb_file: {elapsed:?} ({mb_per_sec:.1} MB/sec)");
    assert!(
        mb_per_sec > 5.0,
        "50MB copy should exceed 5 MB/sec even on modest storage, got {mb_per_sec:.1}"
    );
}

#[test]
fn move_entry_of_a_large_file_on_the_same_filesystem_is_near_instant() {
    // A same-filesystem move is a rename, not a byte-for-byte copy — its
    // cost should be independent of file size.
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.bin");
    {
        let mut file = File::create(&source).unwrap();
        let chunk = vec![0x5Au8; 1024 * 1024];
        for _ in 0..50 {
            file.write_all(&chunk).unwrap();
        }
    }
    let dest = dir.path().join("dest.bin");

    let start = Instant::now();
    move_entry(
        &VeyraPath::from_local(&source),
        &VeyraPath::from_local(&dest),
        false,
    )
    .unwrap();
    let elapsed = start.elapsed();

    assert!(!source.exists());
    assert_eq!(std::fs::metadata(&dest).unwrap().len(), 50 * 1024 * 1024);
    println!("move_entry_of_a_large_file_on_the_same_filesystem: {elapsed:?}");
    assert!(
        elapsed < Duration::from_secs(2),
        "a same-filesystem rename of a 50MB file should be near-instant, took {elapsed:?}"
    );
}
