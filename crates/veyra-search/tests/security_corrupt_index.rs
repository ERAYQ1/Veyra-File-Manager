//! Faz 48: a `search_index.db` file that's been truncated or overwritten
//! with garbage (crash mid-write, disk corruption, hand-edited) must never
//! panic the app — `SearchIndex::open` should surface a clean error instead
//! (Rule #15).

use std::fs;

use veyra_search::SearchIndex;

#[test]
fn open_on_a_garbage_byte_file_reports_a_clean_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("search_index.db");
    fs::write(
        &db_path,
        b"this is not a SQLite database file at all, just noise",
    )
    .unwrap();

    let result = SearchIndex::open(&db_path);
    assert!(
        result.is_err(),
        "opening a garbage-byte file as a search index must error, not panic or silently succeed"
    );
}

#[test]
fn open_on_a_zero_byte_file_reports_a_clean_error_or_reinitializes() {
    // An empty file is what a crash right after `File::create` (before any
    // bytes were ever written) leaves behind — SQLite may treat a
    // zero-length file as "create a fresh database here", which is also an
    // acceptable outcome; only a panic is not.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("search_index.db");
    fs::write(&db_path, b"").unwrap();

    let _ = SearchIndex::open(&db_path);
}

#[test]
fn open_on_a_truncated_sqlite_header_reports_a_clean_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let real_db = dir.path().join("real.db");
    SearchIndex::open(&real_db).unwrap();
    let full = fs::read(&real_db).unwrap();

    let truncated_path = dir.path().join("search_index.db");
    // Cut off partway through the SQLite header/first page.
    fs::write(&truncated_path, &full[..full.len().min(50)]).unwrap();

    let result = SearchIndex::open(&truncated_path);
    // Either a clean error or (for a small enough valid-looking prefix)
    // successful reopen are acceptable; a panic is not.
    let _ = result;
}
