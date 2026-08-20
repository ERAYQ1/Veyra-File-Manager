//! Extraction engine: one function per format, all funneling through
//! [`plan_entry`] for the Zip Slip / symlink defenses documented in
//! `security.rs`, and all cooperatively cancellable via `OperationControl`
//! (Rule #13).

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::error::FsError;
use crate::path::VeyraPath;
use crate::progress::Progress;
use crate::queue::OperationControl;

use super::format::ArchiveFormat;
use super::security::sanitize_entry_path;

/// Defense-in-depth cap on the total decompressed size an extraction may
/// write to disk, regardless of what the archive's own metadata claims
/// (Rule #21) — the zip/tar/7z formats all let a small compressed input
/// expand to an arbitrarily large decompressed one ("zip bomb"), so the
/// limit is enforced against actual bytes written by [`write_entry_file`],
/// not against any size field read from the archive.
const MAX_EXTRACTION_TOTAL_BYTES: u64 = 50 * 1024 * 1024 * 1024;

/// Why an archive entry was not written to disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Symlink/hardlink entries are never materialized (Rule #22) — see
    /// `security.rs` for why that alone closes the symlink-attack class.
    Symlink,
    /// The entry's path normalized to nothing usable, or would have
    /// escaped the destination directory (Rule #21).
    UnsafePath,
}

/// What happened while extracting or creating an archive.
#[derive(Debug, Default)]
pub struct ArchiveOutcome {
    pub extracted: Vec<PathBuf>,
    pub skipped: Vec<(String, SkipReason)>,
    pub errors: Vec<(String, FsError)>,
    pub cancelled: bool,
}

/// Extracts `archive_path` (its format detected from its file name) into
/// `destination_dir`, creating it if needed. Blocking — call from a
/// background thread, per Rule #11.
pub fn extract_archive(
    archive_path: &VeyraPath,
    destination_dir: &VeyraPath,
    control: &OperationControl,
    mut on_progress: impl FnMut(Progress),
) -> Result<ArchiveOutcome, FsError> {
    let archive_local = local_path(archive_path)?;
    let dest_local = local_path(destination_dir)?;
    let format = archive_path
        .file_name()
        .and_then(|name| ArchiveFormat::from_name(&name))
        .ok_or_else(|| FsError::Archive(format!("unrecognized archive format: {archive_path}")))?;

    fs::create_dir_all(&dest_local).map_err(|e| io_err(destination_dir, e))?;

    match format {
        ArchiveFormat::Zip => extract_zip(&archive_local, &dest_local, control, &mut on_progress),
        ArchiveFormat::Tar => {
            let file = fs::File::open(&archive_local).map_err(|e| io_err(archive_path, e))?;
            extract_tar(file, &archive_local, &dest_local, control, &mut on_progress)
        }
        ArchiveFormat::TarGz => {
            let file = fs::File::open(&archive_local).map_err(|e| io_err(archive_path, e))?;
            extract_tar(
                flate2::read::GzDecoder::new(file),
                &archive_local,
                &dest_local,
                control,
                &mut on_progress,
            )
        }
        ArchiveFormat::TarXz => {
            let file = fs::File::open(&archive_local).map_err(|e| io_err(archive_path, e))?;
            extract_tar(
                xz2::read::XzDecoder::new_multi_decoder(file),
                &archive_local,
                &dest_local,
                control,
                &mut on_progress,
            )
        }
        ArchiveFormat::TarZst => {
            let file = fs::File::open(&archive_local).map_err(|e| io_err(archive_path, e))?;
            let decoder = zstd::stream::read::Decoder::new(file)
                .map_err(|e| FsError::Archive(e.to_string()))?;
            extract_tar(
                decoder,
                &archive_local,
                &dest_local,
                control,
                &mut on_progress,
            )
        }
        ArchiveFormat::SevenZip => {
            extract_7z(&archive_local, &dest_local, control, &mut on_progress)
        }
    }
}

/// Where a single entry lands relative to the extraction root, after the
/// Zip Slip / symlink checks — the one chokepoint every format's
/// extraction loop passes through.
enum EntryPlan {
    Dir(PathBuf),
    File(PathBuf),
    Skip(SkipReason),
}

fn plan_entry(dest_root: &Path, raw_name: &str, is_dir: bool, is_symlink: bool) -> EntryPlan {
    if is_symlink {
        return EntryPlan::Skip(SkipReason::Symlink);
    }
    let Some(relative) = sanitize_entry_path(raw_name) else {
        return EntryPlan::Skip(SkipReason::UnsafePath);
    };
    let full = dest_root.join(relative);
    if is_dir {
        EntryPlan::Dir(full)
    } else {
        EntryPlan::File(full)
    }
}

/// What went wrong writing a single extracted entry to disk.
#[derive(Debug)]
enum WriteEntryError {
    Io(io::Error),
    /// Writing this entry would push the archive's total decompressed
    /// output past [`MAX_EXTRACTION_TOTAL_BYTES`] — extraction must abort
    /// rather than skip-and-continue, since a zip bomb's later entries are
    /// exactly as dangerous as its first.
    LimitExceeded,
}

impl From<io::Error> for WriteEntryError {
    fn from(source: io::Error) -> Self {
        WriteEntryError::Io(source)
    }
}

fn write_entry_file(
    path: &Path,
    reader: &mut (impl Read + ?Sized),
    remaining_budget: &mut u64,
) -> Result<(), WriteEntryError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out = fs::File::create(path)?;
    // Reads at most one byte past the remaining budget, so a written count
    // over budget unambiguously means the entry itself would have exceeded
    // the limit had it not been capped.
    let mut limited = reader.take(remaining_budget.saturating_add(1));
    let written = io::copy(&mut limited, &mut out)?;
    if written > *remaining_budget {
        return Err(WriteEntryError::LimitExceeded);
    }
    *remaining_budget -= written;
    Ok(())
}

fn extraction_limit_error(archive_path: &Path) -> FsError {
    FsError::Io {
        path: VeyraPath::from_local(archive_path.to_path_buf()),
        source: io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "archive extraction stopped: total extracted size exceeds the {} GB safety limit (zip bomb protection)",
                MAX_EXTRACTION_TOTAL_BYTES / (1024 * 1024 * 1024)
            ),
        ),
    }
}

fn extract_zip(
    archive_path: &Path,
    dest_root: &Path,
    control: &OperationControl,
    on_progress: &mut impl FnMut(Progress),
) -> Result<ArchiveOutcome, FsError> {
    let file = fs::File::open(archive_path).map_err(|e| local_io_err(archive_path, e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| FsError::Archive(format!("invalid zip: {e}")))?;
    let file_count = archive.len();
    let mut outcome = ArchiveOutcome::default();
    let mut remaining_budget = MAX_EXTRACTION_TOTAL_BYTES;

    for index in 0..file_count {
        if control.wait_if_paused() {
            outcome.cancelled = true;
            return Ok(outcome);
        }

        let mut entry = match archive.by_index(index) {
            Ok(entry) => entry,
            Err(e) => {
                outcome
                    .errors
                    .push((format!("entry #{index}"), FsError::Archive(e.to_string())));
                continue;
            }
        };
        let name = entry.name().to_string();
        on_progress(Progress {
            current_file: name.clone(),
            file_index: index,
            file_count,
            bytes_done: 0,
            bytes_total: 0,
        });

        match plan_entry(dest_root, &name, entry.is_dir(), entry.is_symlink()) {
            EntryPlan::Skip(reason) => outcome.skipped.push((name, reason)),
            EntryPlan::Dir(path) => {
                if let Err(e) = fs::create_dir_all(&path) {
                    outcome.errors.push((name, local_io_err_msg(e)));
                } else {
                    outcome.extracted.push(path);
                }
            }
            EntryPlan::File(path) => {
                match write_entry_file(&path, &mut entry, &mut remaining_budget) {
                    Ok(()) => outcome.extracted.push(path),
                    Err(WriteEntryError::LimitExceeded) => {
                        return Err(extraction_limit_error(archive_path))
                    }
                    Err(WriteEntryError::Io(e)) => outcome.errors.push((name, local_io_err_msg(e))),
                }
            }
        }
    }

    Ok(outcome)
}

fn extract_tar(
    reader: impl Read,
    archive_path: &Path,
    dest_root: &Path,
    control: &OperationControl,
    on_progress: &mut impl FnMut(Progress),
) -> Result<ArchiveOutcome, FsError> {
    let mut archive = tar::Archive::new(reader);
    let mut outcome = ArchiveOutcome::default();
    let mut remaining_budget = MAX_EXTRACTION_TOTAL_BYTES;

    let entries = archive
        .entries()
        .map_err(|e| FsError::Archive(format!("invalid tar: {e}")))?;

    for (index, entry) in entries.enumerate() {
        if control.wait_if_paused() {
            outcome.cancelled = true;
            return Ok(outcome);
        }

        let mut entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                outcome
                    .errors
                    .push((format!("entry #{index}"), FsError::Archive(e.to_string())));
                continue;
            }
        };

        let name = entry.path().map(|p| p.to_string_lossy().into_owned());
        let name = match name {
            Ok(name) => name,
            Err(e) => {
                outcome
                    .errors
                    .push((format!("entry #{index}"), FsError::Archive(e.to_string())));
                continue;
            }
        };

        on_progress(Progress {
            current_file: name.clone(),
            file_index: index,
            file_count: 0,
            bytes_done: 0,
            bytes_total: 0,
        });

        let entry_type = entry.header().entry_type();
        let is_symlink = entry_type.is_symlink() || entry_type.is_hard_link();
        match plan_entry(dest_root, &name, entry_type.is_dir(), is_symlink) {
            EntryPlan::Skip(reason) => outcome.skipped.push((name, reason)),
            EntryPlan::Dir(path) => {
                if let Err(e) = fs::create_dir_all(&path) {
                    outcome.errors.push((name, local_io_err_msg(e)));
                } else {
                    outcome.extracted.push(path);
                }
            }
            EntryPlan::File(path) => {
                match write_entry_file(&path, &mut entry, &mut remaining_budget) {
                    Ok(()) => outcome.extracted.push(path),
                    Err(WriteEntryError::LimitExceeded) => {
                        return Err(extraction_limit_error(archive_path))
                    }
                    Err(WriteEntryError::Io(e)) => outcome.errors.push((name, local_io_err_msg(e))),
                }
            }
        }
    }

    Ok(outcome)
}

/// 7z entries encode Unix symlinks by setting the high 16 bits of
/// `windows_attributes` to the Unix mode when the `FILE_ATTRIBUTE_UNIX_EXTENSION`
/// bit (`0x8000`) is set — the same convention Info-ZIP/p7zip use.
fn is_7z_symlink(attrs: u32) -> bool {
    const FILE_ATTRIBUTE_UNIX_EXTENSION: u32 = 0x8000;
    const S_IFMT: u32 = 0o170000;
    const S_IFLNK: u32 = 0o120000;
    attrs & FILE_ATTRIBUTE_UNIX_EXTENSION != 0 && ((attrs >> 16) & S_IFMT) == S_IFLNK
}

fn extract_7z(
    archive_path: &Path,
    dest_root: &Path,
    control: &OperationControl,
    on_progress: &mut impl FnMut(Progress),
) -> Result<ArchiveOutcome, FsError> {
    let mut reader = sevenz_rust::SevenZReader::open(archive_path, sevenz_rust::Password::empty())
        .map_err(|e| FsError::Archive(format!("invalid 7z: {e}")))?;
    let file_count = reader.archive().files.len();
    let mut outcome = ArchiveOutcome::default();
    let mut index = 0usize;
    let mut remaining_budget = MAX_EXTRACTION_TOTAL_BYTES;
    let mut limit_hit = false;

    reader
        .for_each_entries(|entry, entry_reader| {
            if control.wait_if_paused() {
                outcome.cancelled = true;
                return Ok(false);
            }

            let name = entry.name().to_string();
            on_progress(Progress {
                current_file: name.clone(),
                file_index: index,
                file_count,
                bytes_done: 0,
                bytes_total: 0,
            });
            index += 1;

            let is_symlink = is_7z_symlink(entry.windows_attributes());
            match plan_entry(dest_root, &name, entry.is_directory(), is_symlink) {
                EntryPlan::Skip(reason) => outcome.skipped.push((name, reason)),
                EntryPlan::Dir(path) => match fs::create_dir_all(&path) {
                    Ok(()) => outcome.extracted.push(path),
                    Err(e) => outcome.errors.push((name, local_io_err_msg(e))),
                },
                EntryPlan::File(path) => {
                    match write_entry_file(&path, entry_reader, &mut remaining_budget) {
                        Ok(()) => outcome.extracted.push(path),
                        Err(WriteEntryError::LimitExceeded) => {
                            limit_hit = true;
                            return Ok(false);
                        }
                        Err(WriteEntryError::Io(e)) => {
                            outcome.errors.push((name, local_io_err_msg(e)))
                        }
                    }
                }
            }

            Ok(true)
        })
        .map_err(|e| FsError::Archive(format!("7z extraction failed: {e}")))?;

    if limit_hit {
        return Err(extraction_limit_error(archive_path));
    }

    Ok(outcome)
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

fn io_err(path: &VeyraPath, source: io::Error) -> FsError {
    FsError::Io {
        path: path.clone(),
        source,
    }
}

fn local_io_err(path: &Path, source: io::Error) -> FsError {
    FsError::Io {
        path: VeyraPath::from_local(path.to_path_buf()),
        source,
    }
}

fn local_io_err_msg(source: io::Error) -> FsError {
    FsError::Archive(source.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn control() -> OperationControl {
        OperationControl::new()
    }

    #[test]
    fn extracts_flat_zip() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("a.zip");
        {
            let file = fs::File::create(&archive_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();
            writer.start_file("hello.txt", opts).unwrap();
            writer.write_all(b"world").unwrap();
            writer.finish().unwrap();
        }
        let dest = dir.path().join("out");

        let outcome = extract_archive(
            &VeyraPath::from_local(archive_path),
            &VeyraPath::from_local(dest.clone()),
            &control(),
            |_| {},
        )
        .unwrap();

        assert!(outcome.errors.is_empty());
        assert_eq!(fs::read(dest.join("hello.txt")).unwrap(), b"world");
    }

    #[test]
    fn zip_slip_entry_is_confined_to_destination() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("evil.zip");
        {
            let file = fs::File::create(&archive_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();
            writer.start_file("../../etc/evil.txt", opts).unwrap();
            writer.write_all(b"pwned").unwrap();
            writer.finish().unwrap();
        }
        let dest = dir.path().join("nested/out");
        fs::create_dir_all(&dest).unwrap();

        let outcome = extract_archive(
            &VeyraPath::from_local(archive_path),
            &VeyraPath::from_local(dest.clone()),
            &control(),
            |_| {},
        )
        .unwrap();

        assert!(outcome.extracted.is_empty());
        assert!(!dir.path().join("etc/evil.txt").exists());
        assert!(dest
            .read_dir()
            .unwrap()
            .next()
            .is_none_or(|e| e.unwrap().file_name() != "evil.txt"));
    }

    #[test]
    fn extracts_tar_gz_directory_tree() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("a.tar.gz");
        {
            let file = fs::File::create(&archive_path).unwrap();
            let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_size(5);
            header.set_cksum();
            builder
                .append_data(&mut header, "nested/hello.txt", &b"world"[..])
                .unwrap();
            builder.into_inner().unwrap().finish().unwrap();
        }
        let dest = dir.path().join("out");

        let outcome = extract_archive(
            &VeyraPath::from_local(archive_path),
            &VeyraPath::from_local(dest.clone()),
            &control(),
            |_| {},
        )
        .unwrap();

        assert!(outcome.errors.is_empty());
        assert_eq!(fs::read(dest.join("nested/hello.txt")).unwrap(), b"world");
    }

    #[test]
    fn write_entry_file_stays_within_budget() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("small.bin");
        let mut budget = 10u64;

        write_entry_file(&path, &mut &b"hello"[..], &mut budget).unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"hello");
        assert_eq!(budget, 5);
    }

    #[test]
    fn write_entry_file_rejects_entry_over_budget() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bomb.bin");
        let mut budget = 3u64;

        let err = write_entry_file(&path, &mut &b"way too much data"[..], &mut budget)
            .expect_err("entry larger than the remaining budget must be rejected");

        assert!(matches!(err, WriteEntryError::LimitExceeded));
    }

    #[test]
    fn extraction_limit_error_reports_the_archive_path() {
        let path = Path::new("/tmp/bomb.zip");
        let err = extraction_limit_error(path);
        assert!(matches!(err, FsError::Io { .. }));
        assert!(err.to_string().contains("50 GB safety limit"));
    }
}
