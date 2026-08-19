//! Faz 49: FTS5 search-query latency benchmark at 10,000 indexed files —
//! the scale the phase spec names (Rule #32). Indexing 10,000 entries one
//! at a time through the public `index_entry` API (each call its own
//! transaction) is itself the dominant cost here and is reported alongside
//! the search timing so the two aren't confused with each other; what this
//! test actually asserts on is query latency, which is what matters for UI
//! responsiveness while the user types.

use std::path::Path;
use std::time::Instant;

use chrono::Utc;
use veyra_search::{parse, IndexedEntry, SearchIndex};

#[test]
fn fts5_search_latency_at_10_000_indexed_files_is_low() {
    let index = SearchIndex::open_in_memory().unwrap();
    let dir = Path::new("/home/user/Documents");

    let index_start = Instant::now();
    for i in 0..10_000usize {
        let name = format!("file_{i:05}.txt");
        let path = format!("/home/user/Documents/{name}");
        index
            .index_entry(&IndexedEntry {
                directory: dir,
                name: &name,
                path: Path::new(&path),
                is_dir: false,
                size_bytes: (i * 100) as u64,
                mime_type: "text/plain",
                is_executable: false,
                modified_unix: Some(1_700_000_000 + i as i64),
            })
            .unwrap();
    }
    println!(
        "indexed 10,000 entries in {:?} (one transaction per entry — see doc comment)",
        index_start.elapsed()
    );

    // A handful of representative queries: an exact-ish filename match, a
    // broad free-text term matching every entry, and a miss — search
    // latency shouldn't depend heavily on how many rows match.
    for (label, query) in [
        ("narrow", "file_05000"),
        ("broad", "file"),
        ("miss", "nonexistent_needle"),
    ] {
        let start = Instant::now();
        let results = index.search(&parse(query), Utc::now()).unwrap();
        let elapsed = start.elapsed();
        println!(
            "search {label:?} ({query:?}): {elapsed:?} ({} results)",
            results.len()
        );
        assert!(
            elapsed.as_millis() < 50,
            "FTS5 search {label:?} over 10,000 indexed files took {elapsed:?}, expected well under 50ms"
        );
    }
}
