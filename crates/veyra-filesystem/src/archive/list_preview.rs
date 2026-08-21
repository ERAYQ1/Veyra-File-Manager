//! Faz 60: Quick Look's archive card — lists the first `limit` entries of an
//! archive (name, size, directory-or-not) without extracting anything to
//! disk. Read-only metadata scan, unlike `extract_archive`, so none of the
//! Zip Slip / symlink hardening in `security.rs` applies here (nothing is
//! ever written).

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::FsError;
use crate::path::VeyraPath;

use super::format::ArchiveFormat;

/// One archive entry as shown in the Quick Look preview list.
#[derive(Debug, Clone)]
pub struct ArchivePreviewEntry {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

/// Lists up to `limit` entries from `archive_path` (format detected from its
/// file name). Blocking — call from a background thread, per Rule #11.
/// `Err` for 7z (no cheap metadata-only listing available via
/// `sevenz-rust`) and for non-local/unrecognized archives; callers should
/// treat that as "no listing available" rather than a hard failure.
pub fn list_preview(
    archive_path: &VeyraPath,
    limit: usize,
) -> Result<Vec<ArchivePreviewEntry>, FsError> {
    let local = local_path(archive_path)?;
    let format = archive_path
        .file_name()
        .and_then(|name| ArchiveFormat::from_name(&name))
        .ok_or_else(|| FsError::Archive(format!("unrecognized archive format: {archive_path}")))?;

    match format {
        ArchiveFormat::Zip => list_zip(&local, limit),
        ArchiveFormat::Tar => {
            let file = fs::File::open(&local).map_err(|e| io_err(archive_path, e))?;
            list_tar(file, limit)
        }
        ArchiveFormat::TarGz => {
            let file = fs::File::open(&local).map_err(|e| io_err(archive_path, e))?;
            list_tar(flate2::read::GzDecoder::new(file), limit)
        }
        ArchiveFormat::TarXz => {
            let file = fs::File::open(&local).map_err(|e| io_err(archive_path, e))?;
            list_tar(xz2::read::XzDecoder::new_multi_decoder(file), limit)
        }
        ArchiveFormat::TarZst => {
            let file = fs::File::open(&local).map_err(|e| io_err(archive_path, e))?;
            let decoder = zstd::stream::read::Decoder::new(file)
                .map_err(|e| FsError::Archive(e.to_string()))?;
            list_tar(decoder, limit)
        }
        ArchiveFormat::SevenZip => Err(FsError::Archive(
            "7z preview listing not supported".to_string(),
        )),
    }
}

fn list_zip(archive_path: &Path, limit: usize) -> Result<Vec<ArchivePreviewEntry>, FsError> {
    let file = fs::File::open(archive_path).map_err(|e| local_io_err(archive_path, e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| FsError::Archive(format!("invalid zip: {e}")))?;
    let count = archive.len().min(limit);
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let entry = archive
            .by_index(index)
            .map_err(|e| FsError::Archive(e.to_string()))?;
        entries.push(ArchivePreviewEntry {
            name: entry.name().to_string(),
            size: entry.size(),
            is_dir: entry.is_dir(),
        });
    }
    Ok(entries)
}

fn list_tar(reader: impl Read, limit: usize) -> Result<Vec<ArchivePreviewEntry>, FsError> {
    let mut archive = tar::Archive::new(reader);
    let raw_entries = archive
        .entries()
        .map_err(|e| FsError::Archive(format!("invalid tar: {e}")))?;

    let mut entries = Vec::with_capacity(limit);
    for entry in raw_entries.take(limit) {
        let entry = entry.map_err(|e| FsError::Archive(e.to_string()))?;
        let is_dir = entry.header().entry_type().is_dir();
        let size = entry.header().size().unwrap_or(0);
        let name = entry
            .path()
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| FsError::Archive(e.to_string()))?;
        entries.push(ArchivePreviewEntry { name, size, is_dir });
    }
    Ok(entries)
}

fn local_path(path: &VeyraPath) -> Result<PathBuf, FsError> {
    path.as_local_path()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| {
            FsError::Archive(format!(
                "archives are only supported on local paths, not {path}"
            ))
        })
}

fn io_err(path: &VeyraPath, err: std::io::Error) -> FsError {
    FsError::Io {
        path: path.clone(),
        source: err,
    }
}

fn local_io_err(path: &Path, err: std::io::Error) -> FsError {
    FsError::Io {
        path: VeyraPath::from_local(path),
        source: err,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::create_archive;
    use crate::queue::OperationControl;

    fn build_fixture(dir: &std::path::Path, format: ArchiveFormat) -> VeyraPath {
        let source = dir.join("hello.txt");
        std::fs::write(&source, b"hello world").unwrap();
        let source_path = VeyraPath::from_local(&source);

        let archive_path = VeyraPath::from_local(dir.join(format!("out{}", format.extension())));
        let control = OperationControl::new();
        create_archive(&archive_path, &[source_path], format, &control, |_| {}).unwrap();
        archive_path
    }

    #[test]
    fn lists_zip_entries_with_name_and_size() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = build_fixture(dir.path(), ArchiveFormat::Zip);

        let entries = list_preview(&archive_path, 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "hello.txt");
        assert_eq!(entries[0].size, 11);
        assert!(!entries[0].is_dir);
    }

    #[test]
    fn lists_tar_gz_entries_with_name_and_size() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = build_fixture(dir.path(), ArchiveFormat::TarGz);

        let entries = list_preview(&archive_path, 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "hello.txt");
        assert_eq!(entries[0].size, 11);
    }

    #[test]
    fn respects_the_entry_limit() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..5 {
            std::fs::write(dir.path().join(format!("f{i}.txt")), b"x").unwrap();
        }
        let sources: Vec<VeyraPath> = (0..5)
            .map(|i| VeyraPath::from_local(dir.path().join(format!("f{i}.txt"))))
            .collect();
        let archive_path = VeyraPath::from_local(dir.path().join("many.zip"));
        let control = OperationControl::new();
        create_archive(
            &archive_path,
            &sources,
            ArchiveFormat::Zip,
            &control,
            |_| {},
        )
        .unwrap();

        let entries = list_preview(&archive_path, 2).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn sevenzip_listing_is_reported_unsupported_not_a_hard_error() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = VeyraPath::from_local(dir.path().join("out.7z"));
        std::fs::write(archive_path.as_local_path().unwrap(), b"not a real 7z").unwrap();

        let err = list_preview(&archive_path, 10).unwrap_err();
        assert!(matches!(err, FsError::Archive(_)));
    }
}
