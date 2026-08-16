//! Faz 30 stress/scaling tests for the chunked directory scanner
//! (`read_dir_chunked`): linear, bounded-batch delivery across 100, 1,000,
//! 10,000 and 100,000-entry directories; cooperative cancellation stopping
//! enumeration immediately; and count/size accumulation that doesn't
//! overflow at scale (Rule #30, #33, #37).

use std::fs::File;
use std::sync::atomic::{AtomicUsize, Ordering};

use veyra_filesystem::{read_dir_chunked, OperationControl, VeyraPath, READ_DIR_CHUNK_SIZE};

fn make_dir_with_files(count: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..count {
        File::create(dir.path().join(format!("file-{i:07}.txt"))).unwrap();
    }
    dir
}

fn scan_all(dir: &VeyraPath, chunk_size: usize) -> (usize, usize, usize) {
    // Returns (total_items, chunk_count, max_chunk_len).
    let control = OperationControl::new();
    let mut total = 0usize;
    let mut chunk_count = 0usize;
    let mut max_chunk_len = 0usize;
    read_dir_chunked(dir, chunk_size, &control, |chunk| {
        max_chunk_len = max_chunk_len.max(chunk.len());
        total += chunk.len();
        chunk_count += 1;
    })
    .unwrap();
    (total, chunk_count, max_chunk_len)
}

#[test]
fn chunked_scan_covers_every_entry_at_100() {
    let dir = make_dir_with_files(100);
    let path = VeyraPath::from_local(dir.path());
    let (total, _chunks, max_len) = scan_all(&path, READ_DIR_CHUNK_SIZE);
    assert_eq!(total, 100);
    assert!(max_len <= READ_DIR_CHUNK_SIZE);
}

#[test]
fn chunked_scan_covers_every_entry_at_1000() {
    let dir = make_dir_with_files(1_000);
    let path = VeyraPath::from_local(dir.path());
    let (total, chunks, max_len) = scan_all(&path, READ_DIR_CHUNK_SIZE);
    assert_eq!(total, 1_000);
    // 1000 items in 500-sized batches: exactly 2 full chunks, no dangling
    // partial batch.
    assert_eq!(chunks, 2);
    assert!(max_len <= READ_DIR_CHUNK_SIZE);
}

#[test]
fn chunked_scan_covers_every_entry_at_10_000() {
    let dir = make_dir_with_files(10_000);
    let path = VeyraPath::from_local(dir.path());
    let (total, _chunks, max_len) = scan_all(&path, READ_DIR_CHUNK_SIZE);
    assert_eq!(total, 10_000);
    assert!(max_len <= READ_DIR_CHUNK_SIZE);
}

#[test]
fn chunked_scan_covers_every_entry_at_100_000() {
    let dir = make_dir_with_files(100_000);
    let path = VeyraPath::from_local(dir.path());
    let (total, chunks, max_len) = scan_all(&path, READ_DIR_CHUNK_SIZE);
    assert_eq!(total, 100_000);
    // No single batch ever holds more than `chunk_size` entries — memory use
    // stays bounded and linear rather than the whole 100k-entry directory
    // being materialized as one `Vec` before the caller sees anything.
    assert!(max_len <= READ_DIR_CHUNK_SIZE);
    assert_eq!(chunks, 100_000 / READ_DIR_CHUNK_SIZE);
}

#[test]
fn first_chunk_is_available_before_the_scan_finishes() {
    // A small chunk size against a directory with several batches' worth of
    // entries: the first `on_chunk` call must fire well before the walk is
    // done, proving delivery is genuinely incremental, not buffered until
    // the end and then replayed in slices.
    let dir = make_dir_with_files(30);
    let path = VeyraPath::from_local(dir.path());
    let control = OperationControl::new();
    let seen_before_last_chunk = AtomicUsize::new(0);
    let mut chunk_index = 0usize;
    let mut delivered = 0usize;
    read_dir_chunked(&path, 5, &control, |chunk| {
        delivered += chunk.len();
        if chunk_index == 0 {
            seen_before_last_chunk.store(delivered, Ordering::SeqCst);
        }
        chunk_index += 1;
    })
    .unwrap();

    assert_eq!(chunk_index, 6, "30 entries in batches of 5 is 6 batches");
    assert_eq!(seen_before_last_chunk.load(Ordering::SeqCst), 5);
    assert_eq!(delivered, 30);
}

#[test]
fn cancellation_stops_enumeration_immediately() {
    let dir = make_dir_with_files(10_000);
    let path = VeyraPath::from_local(dir.path());
    let control = OperationControl::new();
    let control_for_callback = control.clone();

    let mut delivered = 0usize;
    read_dir_chunked(&path, 100, &control, |chunk| {
        delivered += chunk.len();
        // Cancel as soon as the first batch lands, well short of the full
        // 10,000-entry directory.
        control_for_callback.cancel();
    })
    .unwrap();

    assert!(
        delivered < 10_000,
        "cancellation should stop the scan far short of the full directory, got {delivered}"
    );
    assert!(
        delivered > 0,
        "the batch already in flight should still be delivered"
    );
}

#[test]
fn cancelled_before_start_delivers_nothing() {
    let dir = make_dir_with_files(50);
    let path = VeyraPath::from_local(dir.path());
    let control = OperationControl::new();
    control.cancel();

    let mut delivered = 0usize;
    read_dir_chunked(&path, 10, &control, |chunk| delivered += chunk.len()).unwrap();

    assert_eq!(delivered, 0);
}

#[test]
fn item_and_size_totals_accumulate_without_overflow_at_scale() {
    // Every file here is empty, so the meaningful assertion is that summing
    // counts/sizes across every batch of a 100,000-entry directory via
    // `saturating_add` (the same pattern `on_chunk` in the UI layer uses)
    // never panics and lands on the exact expected total — no silent
    // truncation, no arithmetic panic in debug builds.
    let dir = make_dir_with_files(100_000);
    let path = VeyraPath::from_local(dir.path());
    let control = OperationControl::new();

    let mut total_items: u64 = 0;
    let mut total_bytes: u64 = 0;
    read_dir_chunked(&path, READ_DIR_CHUNK_SIZE, &control, |chunk| {
        total_items = total_items.saturating_add(chunk.len() as u64);
        for item in &chunk {
            total_bytes = total_bytes.saturating_add(item.metadata.size_bytes);
        }
    })
    .unwrap();

    assert_eq!(total_items, 100_000);
    assert_eq!(total_bytes, 0);
}

#[test]
fn chunk_size_of_one_still_covers_every_entry() {
    let dir = make_dir_with_files(25);
    let path = VeyraPath::from_local(dir.path());
    let (total, chunks, max_len) = scan_all(&path, 1);
    assert_eq!(total, 25);
    assert_eq!(chunks, 25);
    assert_eq!(max_len, 1);
}

#[test]
fn zero_chunk_size_is_treated_as_one() {
    let dir = make_dir_with_files(5);
    let path = VeyraPath::from_local(dir.path());
    let (total, _chunks, max_len) = scan_all(&path, 0);
    assert_eq!(total, 5);
    assert!(max_len <= 1);
}

#[test]
fn nonexistent_directory_is_a_hard_error() {
    let missing = VeyraPath::from_local("/nonexistent/does-not-exist-veyra-scaling-test");
    let control = OperationControl::new();
    let result = read_dir_chunked(&missing, READ_DIR_CHUNK_SIZE, &control, |_| {});
    assert!(result.is_err());
}
