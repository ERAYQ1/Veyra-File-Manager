//! The Faz 9 search index: a SQLite + FTS5 database (`directories`,
//! `files`, `metadata`, `fts_index`) behind a mutex, so the background
//! indexer thread and interactive search queries can share one connection
//! safely.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

use crate::classify::classify;
use crate::error::SearchError;
use crate::query::{ParsedQuery, SizeOp};

/// A single file/directory to (re)index. Re-indexing the same `path` again
/// (e.g. after an edit) overwrites the previous row — `path` is the unique
/// key.
pub struct IndexedEntry<'a> {
    pub directory: &'a Path,
    pub name: &'a str,
    pub path: &'a Path,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub mime_type: &'a str,
    pub is_executable: bool,
    /// Unix seconds, when known.
    pub modified_unix: Option<i64>,
}

/// One row of a search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub mime_type: String,
    pub kind: String,
    pub modified_unix: Option<i64>,
}

/// The maximum number of rows a single `search` call returns — keeps a
/// broad/unfiltered query cheap to render even against a huge index.
const RESULT_LIMIT: i64 = 500;

pub struct SearchIndex {
    conn: Mutex<Connection>,
}

impl SearchIndex {
    /// Opens (creating if needed) the on-disk index at `db_path`.
    pub fn open(db_path: &Path) -> Result<Self, SearchError> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| SearchError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let conn = Connection::open(db_path)?;
        crate::schema::init(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// An ephemeral in-memory index — used by tests and available to
    /// callers that want search without persisting to disk.
    pub fn open_in_memory() -> Result<Self, SearchError> {
        let conn = Connection::open_in_memory()?;
        crate::schema::init(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Indexes (or re-indexes) a single entry.
    pub fn index_entry(&self, entry: &IndexedEntry<'_>) -> Result<(), SearchError> {
        let mut conn = self.conn.lock().expect("search index mutex poisoned");
        let tx = conn.transaction()?;

        let dir_str = entry.directory.to_string_lossy();
        tx.execute(
            "INSERT INTO directories(path) VALUES (?1) ON CONFLICT(path) DO NOTHING",
            [dir_str.as_ref()],
        )?;
        let directory_id: i64 = tx.query_row(
            "SELECT id FROM directories WHERE path = ?1",
            [dir_str.as_ref()],
            |row| row.get(0),
        )?;

        let kind = classify(entry.mime_type, entry.name, entry.is_executable);
        let path_str = entry.path.to_string_lossy();

        tx.execute(
            "INSERT INTO files(directory_id, name, path, is_dir, size_bytes, mime_type, kind, modified_unix)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(path) DO UPDATE SET
                 directory_id = excluded.directory_id,
                 name = excluded.name,
                 is_dir = excluded.is_dir,
                 size_bytes = excluded.size_bytes,
                 mime_type = excluded.mime_type,
                 kind = excluded.kind,
                 modified_unix = excluded.modified_unix",
            params![
                directory_id,
                entry.name,
                path_str.as_ref(),
                entry.is_dir as i64,
                entry.size_bytes as i64,
                entry.mime_type,
                kind,
                entry.modified_unix,
            ],
        )?;
        let file_id: i64 = tx.query_row(
            "SELECT id FROM files WHERE path = ?1",
            [path_str.as_ref()],
            |row| row.get(0),
        )?;

        tx.execute("DELETE FROM fts_index WHERE file_id = ?1", [file_id])?;
        tx.execute(
            "INSERT INTO fts_index(filename, path, content_metadata, file_id) VALUES (?1, ?2, ?3, ?4)",
            params![entry.name, path_str.as_ref(), kind, file_id],
        )?;

        tx.commit()?;
        Ok(())
    }

    /// Removes a previously indexed path (e.g. after a delete/move) so
    /// stale rows don't linger in search results.
    pub fn remove_path(&self, path: &Path) -> Result<(), SearchError> {
        let conn = self.conn.lock().expect("search index mutex poisoned");
        let path_str = path.to_string_lossy();
        conn.execute("DELETE FROM files WHERE path = ?1", [path_str.as_ref()])?;
        Ok(())
    }

    /// Runs a parsed query against the index. `now` resolves any
    /// `modified:` preset to a concrete date range (see
    /// `ModifiedPreset::resolve`) — passed explicitly rather than read from
    /// the system clock so this stays deterministically testable.
    pub fn search(
        &self,
        query: &ParsedQuery,
        now: DateTime<Utc>,
    ) -> Result<Vec<SearchResult>, SearchError> {
        let conn = self.conn.lock().expect("search index mutex poisoned");

        let mut sql = String::from(
            "SELECT f.path, f.name, f.is_dir, f.size_bytes, f.mime_type, f.kind, f.modified_unix FROM files f",
        );
        let mut conditions: Vec<String> = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if !query.free_text.is_empty() || query.name.is_some() {
            sql.push_str(" JOIN fts_index ON fts_index.file_id = f.id");

            let mut match_terms: Vec<String> = query
                .free_text
                .iter()
                .map(|term| fts5_quote(term))
                .collect();
            if let Some(name) = &query.name {
                match_terms.push(format!("filename:{}", fts5_quote(name)));
            }
            conditions.push("fts_index MATCH ?".to_string());
            params.push(Box::new(match_terms.join(" ")));
        }

        if let Some(file_type) = query.file_type {
            conditions.push("f.kind = ?".to_string());
            params.push(Box::new(file_type.as_kind_str().to_string()));
        }

        if let Some(size) = query.size {
            let op = match size.op {
                SizeOp::GreaterThan => ">",
                SizeOp::LessThan => "<",
                SizeOp::Equal => "=",
            };
            conditions.push(format!("f.size_bytes {op} ?"));
            params.push(Box::new(size.bytes as i64));
        }

        if let Some(preset) = query.modified {
            let (start, end) = preset.resolve(now);
            conditions.push("f.modified_unix >= ? AND f.modified_unix < ?".to_string());
            params.push(Box::new(start.timestamp()));
            params.push(Box::new(end.timestamp()));
        }

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        sql.push_str(" ORDER BY f.name LIMIT ?");
        params.push(Box::new(RESULT_LIMIT));

        let mut stmt = conn.prepare(&sql)?;
        let params_ref: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_ref.as_slice(), |row| {
            Ok(SearchResult {
                path: PathBuf::from(row.get::<_, String>(0)?),
                name: row.get(1)?,
                is_dir: row.get::<_, i64>(2)? != 0,
                size_bytes: row.get::<_, i64>(3)? as u64,
                mime_type: row.get(4)?,
                kind: row.get(5)?,
                modified_unix: row.get(6)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

/// Wraps `term` as a quoted FTS5 phrase, escaping embedded quotes. FTS5's
/// `MATCH` string has its own operator syntax (`AND`, `-`, `*`, `(`...);
/// quoting every user-supplied term as a literal phrase means arbitrary
/// search input can never be (mis)interpreted as FTS5 query syntax.
fn fts5_quote(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::{parse, FileTypeFilter};
    use chrono::TimeZone;

    fn entry<'a>(
        directory: &'a Path,
        name: &'a str,
        path: &'a Path,
        size_bytes: u64,
        mime_type: &'a str,
        modified_unix: Option<i64>,
    ) -> IndexedEntry<'a> {
        IndexedEntry {
            directory,
            name,
            path,
            is_dir: false,
            size_bytes,
            mime_type,
            is_executable: false,
            modified_unix,
        }
    }

    #[test]
    fn indexes_and_finds_by_free_text() {
        let index = SearchIndex::open_in_memory().unwrap();
        let dir = Path::new("/home/user/Photos");
        let path = Path::new("/home/user/Photos/vacation.png");
        index
            .index_entry(&entry(dir, "vacation.png", path, 2048, "image/png", None))
            .unwrap();

        let results = index.search(&parse("vacation"), Utc::now()).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "vacation.png");
        assert_eq!(results[0].kind, "image");
    }

    #[test]
    fn search_by_type_filter() {
        let index = SearchIndex::open_in_memory().unwrap();
        let dir = Path::new("/d");
        index
            .index_entry(&entry(
                dir,
                "a.png",
                Path::new("/d/a.png"),
                10,
                "image/png",
                None,
            ))
            .unwrap();
        index
            .index_entry(&entry(
                dir,
                "b.pdf",
                Path::new("/d/b.pdf"),
                10,
                "application/pdf",
                None,
            ))
            .unwrap();

        let query = ParsedQuery {
            file_type: Some(FileTypeFilter::Image),
            ..Default::default()
        };
        let results = index.search(&query, Utc::now()).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "a.png");
    }

    #[test]
    fn search_by_size_greater_than() {
        let index = SearchIndex::open_in_memory().unwrap();
        let dir = Path::new("/d");
        index
            .index_entry(&entry(
                dir,
                "small.txt",
                Path::new("/d/small.txt"),
                100,
                "text/plain",
                None,
            ))
            .unwrap();
        index
            .index_entry(&entry(
                dir,
                "big.txt",
                Path::new("/d/big.txt"),
                10_000_000,
                "text/plain",
                None,
            ))
            .unwrap();

        let results = index.search(&parse("size:>1MB"), Utc::now()).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "big.txt");
    }

    #[test]
    fn search_by_modified_preset() {
        let index = SearchIndex::open_in_memory().unwrap();
        let dir = Path::new("/d");
        let now = Utc.with_ymd_and_hms(2026, 8, 9, 12, 0, 0).unwrap();
        let today_ts = now.timestamp();
        let long_ago_ts = Utc
            .with_ymd_and_hms(2020, 1, 1, 0, 0, 0)
            .unwrap()
            .timestamp();

        index
            .index_entry(&entry(
                dir,
                "today.txt",
                Path::new("/d/today.txt"),
                10,
                "text/plain",
                Some(today_ts),
            ))
            .unwrap();
        index
            .index_entry(&entry(
                dir,
                "old.txt",
                Path::new("/d/old.txt"),
                10,
                "text/plain",
                Some(long_ago_ts),
            ))
            .unwrap();

        let results = index.search(&parse("modified:today"), now).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "today.txt");
    }

    #[test]
    fn combined_filters_narrow_results() {
        let index = SearchIndex::open_in_memory().unwrap();
        let dir = Path::new("/d");
        index
            .index_entry(&entry(
                dir,
                "big.png",
                Path::new("/d/big.png"),
                20_000_000,
                "image/png",
                None,
            ))
            .unwrap();
        index
            .index_entry(&entry(
                dir,
                "small.png",
                Path::new("/d/small.png"),
                100,
                "image/png",
                None,
            ))
            .unwrap();
        index
            .index_entry(&entry(
                dir,
                "big.pdf",
                Path::new("/d/big.pdf"),
                20_000_000,
                "application/pdf",
                None,
            ))
            .unwrap();

        let results = index
            .search(&parse("type:image size:>10MB"), Utc::now())
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "big.png");
    }

    #[test]
    fn reindexing_same_path_overwrites_instead_of_duplicating() {
        let index = SearchIndex::open_in_memory().unwrap();
        let dir = Path::new("/d");
        let path = Path::new("/d/file.txt");
        index
            .index_entry(&entry(dir, "file.txt", path, 10, "text/plain", None))
            .unwrap();
        index
            .index_entry(&entry(
                dir,
                "file.txt",
                path,
                20_000_000,
                "text/plain",
                None,
            ))
            .unwrap();

        let results = index.search(&parse("file"), Utc::now()).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].size_bytes, 20_000_000);
    }

    #[test]
    fn remove_path_drops_it_from_results() {
        let index = SearchIndex::open_in_memory().unwrap();
        let dir = Path::new("/d");
        let path = Path::new("/d/gone.txt");
        index
            .index_entry(&entry(dir, "gone.txt", path, 10, "text/plain", None))
            .unwrap();
        index.remove_path(path).unwrap();

        let results = index.search(&parse("gone"), Utc::now()).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn fts_special_characters_in_query_do_not_error() {
        let index = SearchIndex::open_in_memory().unwrap();
        let dir = Path::new("/d");
        index
            .index_entry(&entry(
                dir,
                "notes.txt",
                Path::new("/d/notes.txt"),
                10,
                "text/plain",
                None,
            ))
            .unwrap();

        // FTS5 operator syntax in raw user input must be treated as a
        // literal phrase, not crash the query.
        let results = index
            .search(&parse("AND OR NOT * -notes"), Utc::now())
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn empty_query_returns_all_entries() {
        let index = SearchIndex::open_in_memory().unwrap();
        let dir = Path::new("/d");
        index
            .index_entry(&entry(
                dir,
                "a.txt",
                Path::new("/d/a.txt"),
                10,
                "text/plain",
                None,
            ))
            .unwrap();
        index
            .index_entry(&entry(
                dir,
                "b.txt",
                Path::new("/d/b.txt"),
                10,
                "text/plain",
                None,
            ))
            .unwrap();

        let results = index.search(&ParsedQuery::default(), Utc::now()).unwrap();
        assert_eq!(results.len(), 2);
    }
}
