//! Faz 47: `name:`/`size:`/`type:`/`modified:` search syntax exercised
//! end-to-end against a real FTS5 index (not just `query::parse` in
//! isolation), plus empty/special-character/Unicode query robustness
//! (Rule #34/#35).

use std::path::Path;

use chrono::{TimeZone, Utc};
use veyra_search::{parse, FileTypeFilter, IndexedEntry, SearchIndex};

fn entry<'a>(
    dir: &'a Path,
    name: &'a str,
    path: &'a Path,
    size_bytes: u64,
    mime_type: &'a str,
    modified_unix: Option<i64>,
) -> IndexedEntry<'a> {
    IndexedEntry {
        directory: dir,
        name,
        path,
        is_dir: false,
        size_bytes,
        mime_type,
        is_executable: false,
        modified_unix,
    }
}

fn seeded_index() -> SearchIndex {
    let index = SearchIndex::open_in_memory().unwrap();
    let dir = Path::new("/home/user/Documents");
    let now = Utc.with_ymd_and_hms(2026, 8, 19, 12, 0, 0).unwrap();
    let today = now.timestamp();
    let long_ago = Utc
        .with_ymd_and_hms(2020, 1, 1, 0, 0, 0)
        .unwrap()
        .timestamp();

    index
        .index_entry(&entry(
            dir,
            "report_final.pdf",
            Path::new("/home/user/Documents/report_final.pdf"),
            5 * 1024 * 1024,
            "application/pdf",
            Some(today),
        ))
        .unwrap();
    index
        .index_entry(&entry(
            dir,
            "report_draft.pdf",
            Path::new("/home/user/Documents/report_draft.pdf"),
            200 * 1024,
            "application/pdf",
            Some(long_ago),
        ))
        .unwrap();
    index
        .index_entry(&entry(
            dir,
            "vacation.png",
            Path::new("/home/user/Documents/vacation.png"),
            12 * 1024 * 1024,
            "image/png",
            Some(today),
        ))
        .unwrap();
    index
        .index_entry(&entry(
            dir,
            "türkçe_rapor_şık.doc",
            Path::new("/home/user/Documents/türkçe_rapor_şık.doc"),
            1024,
            "application/msword",
            Some(today),
        ))
        .unwrap();

    index
}

#[test]
fn combined_type_size_and_free_text_filters_narrow_to_the_exact_match() {
    let index = seeded_index();
    let now = Utc.with_ymd_and_hms(2026, 8, 19, 12, 0, 0).unwrap();

    let results = index
        .search(&parse("type:document size:>1MB report"), now)
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "report_final.pdf");
}

#[test]
fn name_filter_matches_via_fts5_filename_column() {
    let index = seeded_index();
    let results = index.search(&parse("name:vacation"), Utc::now()).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "vacation.png");
}

#[test]
fn modified_today_excludes_the_long_ago_entry() {
    let index = seeded_index();
    let now = Utc.with_ymd_and_hms(2026, 8, 19, 12, 0, 0).unwrap();

    let results = index
        .search(&parse("type:document modified:today"), now)
        .unwrap();
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"report_final.pdf"));
    assert!(!names.contains(&"report_draft.pdf"));
}

#[test]
fn empty_and_whitespace_only_queries_return_everything_without_erroring() {
    let index = seeded_index();
    let results = index.search(&parse(""), Utc::now()).unwrap();
    assert_eq!(results.len(), 4);

    let results = index.search(&parse("   "), Utc::now()).unwrap();
    assert_eq!(results.len(), 4);
}

#[test]
fn fts5_special_characters_in_free_text_never_error() {
    let index = seeded_index();
    for query in [
        "report\"final",
        "report*",
        "report AND draft",
        "report OR vacation",
        "(report)",
        "report-draft",
        "NEAR(report draft)",
        "\"",
        "***",
    ] {
        index
            .search(&parse(query), Utc::now())
            .unwrap_or_else(|e| panic!("query {query:?} errored: {e}"));
    }
}

#[test]
fn unicode_query_matches_unicode_indexed_name() {
    let index = seeded_index();
    let results = index.search(&parse("türkçe"), Utc::now()).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "türkçe_rapor_şık.doc");
}

#[test]
fn unknown_type_value_degrades_to_free_text_search_instead_of_erroring() {
    let index = seeded_index();
    // "type:spreadsheet" isn't a recognized FileTypeFilter, so `parse`
    // treats the whole token as free text; that free text matches nothing
    // in the seeded index, so the search must return zero rows cleanly
    // rather than error.
    let parsed = parse("type:spreadsheet");
    assert_eq!(parsed.file_type, None);
    let results = index.search(&parsed, Utc::now()).unwrap();
    assert!(results.is_empty());
}

#[test]
fn size_filter_alone_with_no_free_text_or_name_skips_the_fts_join() {
    let index = seeded_index();
    let results = index.search(&parse("size:>10MB"), Utc::now()).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "vacation.png");
}

#[test]
fn name_filter_matches_case_insensitively() {
    let index = seeded_index();
    let results = index.search(&parse("name:VACATION"), Utc::now()).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "vacation.png");
}

#[test]
fn size_less_than_filter_returns_only_the_smaller_file() {
    let index = seeded_index();
    let results = index
        .search(&parse("type:document size:<1MB report"), Utc::now())
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "report_draft.pdf");
}

#[test]
fn combined_name_and_free_text_terms_are_anded_together() {
    let index = seeded_index();
    // "report" matches both PDFs; adding "draft" as free text narrows it
    // to the one whose filename also contains "draft".
    let results = index
        .search(&parse("name:report draft"), Utc::now())
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "report_draft.pdf");
}

#[test]
fn removed_path_no_longer_appears_in_search_results() {
    let index = seeded_index();
    assert_eq!(
        index.search(&parse("vacation"), Utc::now()).unwrap().len(),
        1
    );

    index
        .remove_path(Path::new("/home/user/Documents/vacation.png"))
        .unwrap();

    assert!(index
        .search(&parse("vacation"), Utc::now())
        .unwrap()
        .is_empty());
}

#[test]
fn type_filter_variant_round_trips_through_as_kind_str() {
    assert_eq!(FileTypeFilter::Document.as_kind_str(), "document");
    assert_eq!(FileTypeFilter::Image.as_kind_str(), "image");
}
