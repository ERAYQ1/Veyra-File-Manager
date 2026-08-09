//! Maps a MIME type / file name / executable bit to the coarse `type:`
//! classification Faz 9's search filters operate on: `image`, `video`,
//! `document`, `archive`, `executable`, or `"other"` when nothing matches.
//! Kept independent of `veyra_filesystem::FileKind` (that's structural —
//! regular file vs directory vs symlink — this is content classification).

const DOCUMENT_EXTENSIONS: &[&str] = &[
    ".pdf", ".doc", ".docx", ".odt", ".txt", ".md", ".rtf", ".xls", ".xlsx", ".ods", ".ppt",
    ".pptx", ".odp", ".csv",
];

const ARCHIVE_EXTENSIONS: &[&str] = &[
    ".zip", ".tar", ".gz", ".tgz", ".tar.gz", ".bz2", ".tar.bz2", ".xz", ".tar.xz", ".7z", ".rar",
];

/// Classifies a single file. `is_executable` should reflect the POSIX
/// executable permission bits (checked ahead of MIME sniffing, since a
/// shell script's MIME type is `text/plain` but it's still meaningfully
/// "an executable" for `type:executable` searches).
pub fn classify(mime_type: &str, name: &str, is_executable: bool) -> &'static str {
    let mime = mime_type.to_ascii_lowercase();

    if mime.starts_with("image/") {
        return "image";
    }
    if mime.starts_with("video/") {
        return "video";
    }
    // The executable permission bit wins over MIME-based document/archive
    // guesses: a `text/plain`-sniffed shell script with the +x bit set is
    // meaningfully "an executable" for `type:executable`, not a document.
    if is_executable {
        return "executable";
    }
    if is_archive_mime(&mime) || has_extension(name, ARCHIVE_EXTENSIONS) {
        return "archive";
    }
    if is_document_mime(&mime) || has_extension(name, DOCUMENT_EXTENSIONS) {
        return "document";
    }
    "other"
}

fn is_document_mime(mime: &str) -> bool {
    mime.starts_with("text/")
        || mime == "application/pdf"
        || mime.contains("wordprocessing")
        || mime.contains("spreadsheet")
        || mime.contains("presentation")
        || mime.contains("opendocument")
}

fn is_archive_mime(mime: &str) -> bool {
    mime.starts_with("application/zip")
        || mime.starts_with("application/x-tar")
        || mime.starts_with("application/gzip")
        || mime.starts_with("application/x-7z")
        || mime.starts_with("application/x-rar")
        || mime.starts_with("application/x-bzip")
        || mime.starts_with("application/x-xz")
        || mime.contains("compressed")
}

fn has_extension(name: &str, extensions: &[&str]) -> bool {
    let lower = name.to_ascii_lowercase();
    extensions.iter().any(|ext| lower.ends_with(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_images_by_mime() {
        assert_eq!(classify("image/png", "photo.png", false), "image");
        assert_eq!(classify("image/jpeg", "photo.JPG", false), "image");
    }

    #[test]
    fn classifies_videos_by_mime() {
        assert_eq!(classify("video/mp4", "movie.mp4", false), "video");
    }

    #[test]
    fn classifies_documents_by_mime_or_extension() {
        assert_eq!(classify("application/pdf", "report.pdf", false), "document");
        assert_eq!(
            classify("application/octet-stream", "notes.md", false),
            "document"
        );
        assert_eq!(classify("text/plain", "readme.txt", false), "document");
    }

    #[test]
    fn classifies_archives_by_mime_or_extension() {
        assert_eq!(classify("application/zip", "backup.zip", false), "archive");
        assert_eq!(
            classify("application/octet-stream", "data.tar.gz", false),
            "archive"
        );
    }

    #[test]
    fn executable_bit_wins_over_generic_binary_mime() {
        assert_eq!(
            classify("application/x-executable", "myapp", true),
            "executable"
        );
        assert_eq!(
            classify("application/octet-stream", "run.sh", true),
            "executable"
        );
    }

    #[test]
    fn executable_bit_wins_even_over_text_plain_mime() {
        // A shell script mime-sniffed as `text/plain` with the +x bit set
        // should still read as "executable", not silently fall through to
        // "document" just because its MIME looks document-like.
        assert_eq!(classify("text/plain", "deploy.sh", true), "executable");
    }

    #[test]
    fn unrecognized_content_falls_back_to_other() {
        assert_eq!(
            classify("application/octet-stream", "mystery.bin", false),
            "other"
        );
    }
}
