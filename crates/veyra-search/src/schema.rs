//! Faz 9 SQLite schema: `directories`, `files`, `metadata`, and the `fts_index`
//! FTS5 virtual table.

/// Creates every table/index if it doesn't already exist. Safe to call on
/// every `SearchIndex::open` — a fresh DB gets built, an existing one is
/// left untouched.
pub(crate) fn init(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;

        CREATE TABLE IF NOT EXISTS directories (
            id   INTEGER PRIMARY KEY,
            path TEXT NOT NULL UNIQUE
        );

        CREATE TABLE IF NOT EXISTS files (
            id            INTEGER PRIMARY KEY,
            directory_id  INTEGER NOT NULL REFERENCES directories(id) ON DELETE CASCADE,
            name          TEXT NOT NULL,
            path          TEXT NOT NULL UNIQUE,
            is_dir        INTEGER NOT NULL,
            size_bytes    INTEGER NOT NULL,
            mime_type     TEXT NOT NULL,
            kind          TEXT NOT NULL,
            modified_unix INTEGER
        );

        CREATE TABLE IF NOT EXISTS metadata (
            file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            key     TEXT NOT NULL,
            value   TEXT NOT NULL,
            PRIMARY KEY (file_id, key)
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS fts_index USING fts5(
            filename,
            path,
            content_metadata,
            file_id UNINDEXED
        );

        CREATE INDEX IF NOT EXISTS idx_files_directory ON files(directory_id);
        CREATE INDEX IF NOT EXISTS idx_files_kind ON files(kind);
        CREATE INDEX IF NOT EXISTS idx_files_size ON files(size_bytes);
        CREATE INDEX IF NOT EXISTS idx_files_modified ON files(modified_unix);
        ",
    )
}
