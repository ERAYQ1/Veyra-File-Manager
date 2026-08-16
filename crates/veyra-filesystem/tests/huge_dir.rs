//! Faz 31 stress/virtualization tests: `read_dir_chunked`'s lazy-metadata
//! contract (`FAST_ATTRIBUTES`) at scale, and that the on-demand full-stat
//! upgrade path (`stat`) still recovers the fields the fast scan skips.

use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;

use veyra_filesystem::{read_dir_chunked, stat, OperationControl, VeyraPath, READ_DIR_CHUNK_SIZE};

fn make_dir_with_files(count: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..count {
        File::create(dir.path().join(format!("file-{i:07}.txt"))).unwrap();
    }
    dir
}

#[test]
fn fast_scan_of_100_000_entries_skips_costly_attributes() {
    let dir = make_dir_with_files(100_000);
    let path = VeyraPath::from_local(dir.path());
    let control = OperationControl::new();

    let mut total = 0usize;
    let mut any_permissions_set = false;
    let mut any_owner_set = false;
    read_dir_chunked(&path, READ_DIR_CHUNK_SIZE, &control, |chunk| {
        total += chunk.len();
        for item in &chunk {
            // Listing-essential fields must still be populated correctly.
            assert!(!item.name().is_empty());
            assert!(!item.metadata.mime_type.is_empty());
            assert!(item.metadata.modified.is_some());

            any_permissions_set |= item.metadata.permissions.is_some();
            any_owner_set |= item.metadata.owner.is_some();
        }
    })
    .unwrap();

    assert_eq!(total, 100_000);
    // Faz 31: `read_dir_chunked` deliberately never resolves unix::mode or
    // owner::user for the bulk listing path (Rule #33) — those are the
    // fields a lazy `stat()` upgrades on demand instead.
    assert!(!any_permissions_set);
    assert!(!any_owner_set);
}

#[test]
fn lazy_stat_recovers_permissions_and_ownership_for_a_single_entry() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("run.sh");
    fs::write(&script, b"#!/bin/sh\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let dir_path = VeyraPath::from_local(dir.path());
    let control = OperationControl::new();

    let mut fast_item = None;
    read_dir_chunked(&dir_path, READ_DIR_CHUNK_SIZE, &control, |chunk| {
        fast_item = chunk.into_iter().find(|i| i.name() == "run.sh");
    })
    .unwrap();
    let fast_item = fast_item.expect("run.sh present in fast scan");
    assert!(fast_item.metadata.permissions.is_none());

    // The lazy upgrade path (Properties dialog / selection) re-stats the
    // same entry and recovers what the fast scan skipped.
    let full_item = stat(&fast_item.path).unwrap();
    assert!(full_item.metadata.permissions.is_some());
    assert_eq!(
        full_item.metadata.permissions.unwrap().symbolic_string(),
        "rwxr-xr-x"
    );
    assert!(full_item.metadata.owner.is_some());
}

#[test]
fn fast_scan_still_reports_hidden_flag_and_kind_correctly() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".hidden"), b"x").unwrap();
    fs::create_dir(dir.path().join("subdir")).unwrap();

    let dir_path = VeyraPath::from_local(dir.path());
    let control = OperationControl::new();
    let mut items = Vec::new();
    read_dir_chunked(&dir_path, READ_DIR_CHUNK_SIZE, &control, |chunk| {
        items.extend(chunk)
    })
    .unwrap();

    let hidden = items.iter().find(|i| i.name() == ".hidden").unwrap();
    assert!(hidden.metadata.is_hidden);
    let subdir = items.iter().find(|i| i.name() == "subdir").unwrap();
    assert!(subdir.kind().is_directory());
}

#[test]
fn fast_scan_of_100_000_entries_completes_well_under_the_full_scan_budget() {
    // Not a tight perf assertion (CI hardware varies), just a regression
    // guard: a fast-attribute scan of 100k trivial files must not take
    // longer than a generous ceiling, catching an accidental reintroduction
    // of `FULL_ATTRIBUTES` on this path.
    let dir = make_dir_with_files(100_000);
    let path = VeyraPath::from_local(dir.path());
    let control = OperationControl::new();

    let start = std::time::Instant::now();
    let mut total = 0usize;
    read_dir_chunked(&path, READ_DIR_CHUNK_SIZE, &control, |chunk| {
        total += chunk.len();
    })
    .unwrap();
    let elapsed = start.elapsed();

    assert_eq!(total, 100_000);
    assert!(
        elapsed.as_secs() < 30,
        "fast scan of 100,000 entries took {elapsed:?}, expected well under 30s"
    );
}
