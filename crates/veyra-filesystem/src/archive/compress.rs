//! Compression engine: packs `sources` into a new archive at `output_path`.
//! Writes to a `.tmp` sibling and renames it into place once finished, so a
//! cancelled or failed run never leaves a half-written archive at the
//! requested name (Rule #38). Symlinks among the sources (or found while
//! recursing into a source directory) are never followed — consistent with
//! `extract.rs` never materializing them on the way back out, and it avoids
//! symlink-cycle infinite loops.
//!
//! Every file's size is re-read from the open file handle right before its
//! bytes are streamed into the archive, not trusted from the initial
//! directory walk — sources are user files that can change on disk between
//! being planned and being read (Rule #16/#17).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::error::FsError;
use crate::path::VeyraPath;
use crate::progress::Progress;
use crate::queue::OperationControl;

use super::extract::{ArchiveOutcome, SkipReason};
use super::format::ArchiveFormat;

/// Faz 65: the Preferences "Compression Level" pick (Fast/Balanced/
/// Maximum, mapped to a 0-9 scale), applied to every archive this module
/// writes from here on. A process-wide atomic rather than a parameter
/// threaded through `create_archive`'s dozen-plus call sites (production
/// and test alike) — same rationale as `reflink::ENABLE_REFLINK`: it's a
/// live-updatable user preference, not per-call archive data. Defaults to
/// 6 (`CompressionLevelPref::Balanced`'s level), matching every backend's
/// own previous hardcoded default.
static COMPRESSION_LEVEL: AtomicU32 = AtomicU32::new(6);

/// Sets the compression level every subsequent `create_archive` call uses
/// for the Zip/Gzip/Xz backends (0-9 scale). Called once at startup from
/// `VeyraSettings::compression_level` and again on every Preferences
/// change.
pub fn set_compression_level(level: u32) {
    COMPRESSION_LEVEL.store(level.min(9), Ordering::Relaxed);
}

fn compression_level() -> u32 {
    COMPRESSION_LEVEL.load(Ordering::Relaxed)
}

/// One file discovered while walking `sources`, with the archive-relative
/// path it should be stored under.
struct PlannedFile {
    absolute: PathBuf,
    archive_path: String,
}

/// One directory discovered while walking `sources` (needed so empty
/// directories are preserved in the archive too).
struct PlannedDir {
    archive_path: String,
}

#[derive(Default)]
struct Plan {
    files: Vec<PlannedFile>,
    dirs: Vec<PlannedDir>,
    skipped: Vec<(String, SkipReason)>,
}

/// Packs `sources` into a new archive at `output_path` in `format`.
/// Blocking — call from a background thread, per Rule #11.
pub fn create_archive(
    output_path: &VeyraPath,
    sources: &[VeyraPath],
    format: ArchiveFormat,
    control: &OperationControl,
    mut on_progress: impl FnMut(Progress),
) -> Result<ArchiveOutcome, FsError> {
    let output_local = local_path(output_path)?;
    let mut plan = Plan::default();
    for source in sources {
        let local = local_path(source)?;
        walk_source(&local, &mut plan);
    }

    let file_count = plan.files.len();
    let mut outcome = ArchiveOutcome {
        skipped: std::mem::take(&mut plan.skipped),
        ..Default::default()
    };

    if let Some(parent) = output_local.parent() {
        fs::create_dir_all(parent).map_err(|e| local_io_err(parent, e))?;
    }
    let tmp_path = tmp_sibling(&output_local);

    let result = write_archive(
        &tmp_path,
        &plan,
        format,
        control,
        file_count,
        &mut on_progress,
    );

    let cancelled = match result {
        Ok(cancelled) => cancelled,
        Err(e) => {
            let _ = fs::remove_file(&tmp_path);
            return Err(e);
        }
    };

    if cancelled {
        let _ = fs::remove_file(&tmp_path);
        outcome.cancelled = true;
        return Ok(outcome);
    }

    fs::rename(&tmp_path, &output_local).map_err(|e| local_io_err(&output_local, e))?;
    outcome.extracted = plan
        .files
        .iter()
        .map(|f| PathBuf::from(&f.archive_path))
        .collect();
    Ok(outcome)
}

/// Recursively walks `source`, appending its files/dirs to `plan` under an
/// archive-relative path rooted at `source`'s own file name (so compressing
/// `/home/user/Photos` produces entries like `Photos/beach.jpg`, not bare
/// `beach.jpg`). Symlinks are recorded as skipped rather than followed.
fn walk_source(source: &Path, plan: &mut Plan) {
    let Some(name) = source.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return;
    };
    walk_recursive(source, &name, plan);
}

fn walk_recursive(path: &Path, archive_path: &str, plan: &mut Plan) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };

    if metadata.is_symlink() {
        plan.skipped
            .push((archive_path.to_string(), SkipReason::Symlink));
        return;
    }

    if metadata.is_dir() {
        plan.dirs.push(PlannedDir {
            archive_path: archive_path.to_string(),
        });
        let Ok(children) = fs::read_dir(path) else {
            return;
        };
        for child in children.flatten() {
            let child_path = child.path();
            let Some(child_name) = child_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
            else {
                continue;
            };
            let child_archive_path = format!("{archive_path}/{child_name}");
            walk_recursive(&child_path, &child_archive_path, plan);
        }
    } else {
        plan.files.push(PlannedFile {
            absolute: path.to_path_buf(),
            archive_path: archive_path.to_string(),
        });
    }
}

/// Dispatches to the format-specific writer. Returns `Ok(true)` if the
/// operation was cancelled partway through (caller discards the `.tmp`
/// file in that case rather than treating it as an error).
fn write_archive(
    tmp_path: &Path,
    plan: &Plan,
    format: ArchiveFormat,
    control: &OperationControl,
    file_count: usize,
    on_progress: &mut impl FnMut(Progress),
) -> Result<bool, FsError> {
    match format {
        ArchiveFormat::Zip => write_zip(tmp_path, plan, control, file_count, on_progress),
        ArchiveFormat::SevenZip => write_7z(tmp_path, plan, control, file_count, on_progress),
        ArchiveFormat::Tar => {
            let file = fs::File::create(tmp_path).map_err(|e| local_io_err(tmp_path, e))?;
            let Some(file) = write_tar(file, plan, control, file_count, on_progress)? else {
                return Ok(true);
            };
            file.sync_all().map_err(|e| local_io_err(tmp_path, e))?;
            Ok(false)
        }
        ArchiveFormat::TarGz => {
            let file = fs::File::create(tmp_path).map_err(|e| local_io_err(tmp_path, e))?;
            let encoder =
                flate2::write::GzEncoder::new(file, flate2::Compression::new(compression_level()));
            let Some(encoder) = write_tar(encoder, plan, control, file_count, on_progress)? else {
                return Ok(true);
            };
            encoder.finish().map_err(|e| local_io_err(tmp_path, e))?;
            Ok(false)
        }
        ArchiveFormat::TarXz => {
            let file = fs::File::create(tmp_path).map_err(|e| local_io_err(tmp_path, e))?;
            let encoder = xz2::write::XzEncoder::new(file, compression_level());
            let Some(encoder) = write_tar(encoder, plan, control, file_count, on_progress)? else {
                return Ok(true);
            };
            encoder.finish().map_err(|e| local_io_err(tmp_path, e))?;
            Ok(false)
        }
        ArchiveFormat::TarZst => {
            let file = fs::File::create(tmp_path).map_err(|e| local_io_err(tmp_path, e))?;
            let encoder = zstd::stream::write::Encoder::new(file, 0)
                .map_err(|e| FsError::Archive(e.to_string()))?;
            let Some(encoder) = write_tar(encoder, plan, control, file_count, on_progress)? else {
                return Ok(true);
            };
            encoder.finish().map_err(|e| local_io_err(tmp_path, e))?;
            Ok(false)
        }
    }
}

/// Writes `plan` as a tar stream into `writer` (raw `.tar`, or wrapped in a
/// Gz/Xz/Zst encoder by the caller). Returns the inner writer on success, or
/// `None` if cancelled partway through.
fn write_tar<W: Write>(
    writer: W,
    plan: &Plan,
    control: &OperationControl,
    file_count: usize,
    on_progress: &mut impl FnMut(Progress),
) -> Result<Option<W>, FsError> {
    let mut builder = tar::Builder::new(writer);
    builder.mode(tar::HeaderMode::Deterministic);

    for dir in &plan.dirs {
        if control.wait_if_paused() {
            return Ok(None);
        }
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_mode(0o755);
        header
            .set_path(format!("{}/", dir.archive_path))
            .map_err(|e| FsError::Archive(e.to_string()))?;
        header.set_cksum();
        builder
            .append(&header, io::empty())
            .map_err(local_io_err_msg)?;
    }

    for (index, planned) in plan.files.iter().enumerate() {
        if control.wait_if_paused() {
            return Ok(None);
        }
        on_progress(Progress {
            current_file: planned.archive_path.clone(),
            file_index: index,
            file_count,
            bytes_done: 0,
            bytes_total: 0,
        });

        let mut file =
            fs::File::open(&planned.absolute).map_err(|e| local_io_err(&planned.absolute, e))?;
        let size = file
            .metadata()
            .map_err(|e| local_io_err(&planned.absolute, e))?
            .len();

        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(size);
        header.set_mode(0o644);
        header
            .set_path(&planned.archive_path)
            .map_err(|e| FsError::Archive(e.to_string()))?;
        header.set_cksum();
        builder
            .append(&header, &mut file)
            .map_err(local_io_err_msg)?;
    }

    builder.into_inner().map(Some).map_err(local_io_err_msg)
}

fn write_zip(
    tmp_path: &Path,
    plan: &Plan,
    control: &OperationControl,
    file_count: usize,
    on_progress: &mut impl FnMut(Progress),
) -> Result<bool, FsError> {
    let file = fs::File::create(tmp_path).map_err(|e| local_io_err(tmp_path, e))?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(compression_level() as i64));

    for dir in &plan.dirs {
        if control.wait_if_paused() {
            return Ok(true);
        }
        writer
            .add_directory(format!("{}/", dir.archive_path), options)
            .map_err(|e| FsError::Archive(e.to_string()))?;
    }

    for (index, planned) in plan.files.iter().enumerate() {
        if control.wait_if_paused() {
            return Ok(true);
        }
        on_progress(Progress {
            current_file: planned.archive_path.clone(),
            file_index: index,
            file_count,
            bytes_done: 0,
            bytes_total: 0,
        });

        writer
            .start_file(&planned.archive_path, options)
            .map_err(|e| FsError::Archive(e.to_string()))?;
        let mut file =
            fs::File::open(&planned.absolute).map_err(|e| local_io_err(&planned.absolute, e))?;
        io::copy(&mut file, &mut writer).map_err(|e| local_io_err(&planned.absolute, e))?;
    }

    writer
        .finish()
        .map_err(|e| FsError::Archive(e.to_string()))?;
    Ok(false)
}

fn write_7z(
    tmp_path: &Path,
    plan: &Plan,
    control: &OperationControl,
    file_count: usize,
    on_progress: &mut impl FnMut(Progress),
) -> Result<bool, FsError> {
    let mut writer =
        sevenz_rust::SevenZWriter::create(tmp_path).map_err(|e| FsError::Archive(e.to_string()))?;

    for dir in &plan.dirs {
        if control.wait_if_paused() {
            return Ok(true);
        }
        let mut entry = sevenz_rust::SevenZArchiveEntry::new();
        entry.name = format!("{}/", dir.archive_path);
        entry.is_directory = true;
        writer
            .push_archive_entry::<&[u8]>(entry, None)
            .map_err(|e| FsError::Archive(e.to_string()))?;
    }

    for (index, planned) in plan.files.iter().enumerate() {
        if control.wait_if_paused() {
            return Ok(true);
        }
        on_progress(Progress {
            current_file: planned.archive_path.clone(),
            file_index: index,
            file_count,
            bytes_done: 0,
            bytes_total: 0,
        });

        let mut entry = sevenz_rust::SevenZArchiveEntry::new();
        entry.name = planned.archive_path.clone();
        let file =
            fs::File::open(&planned.absolute).map_err(|e| local_io_err(&planned.absolute, e))?;
        writer
            .push_archive_entry(entry, Some(file))
            .map_err(|e| FsError::Archive(e.to_string()))?;
    }

    writer
        .finish()
        .map_err(|e| FsError::Archive(e.to_string()))?;
    Ok(false)
}

/// A `.tmp` path alongside `path`, e.g. `archive.zip` -> `archive.zip.tmp`.
fn tmp_sibling(path: &Path) -> PathBuf {
    let mut os_string = path.as_os_str().to_owned();
    os_string.push(".tmp");
    PathBuf::from(os_string)
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

    fn control() -> OperationControl {
        OperationControl::new()
    }

    #[test]
    fn zip_round_trips_a_directory_tree() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("project");
        fs::create_dir_all(src.join("nested")).unwrap();
        fs::write(src.join("top.txt"), b"top").unwrap();
        fs::write(src.join("nested/inner.txt"), b"inner").unwrap();

        let output = dir.path().join("out.zip");
        let outcome = create_archive(
            &VeyraPath::from_local(output.clone()),
            &[VeyraPath::from_local(src)],
            ArchiveFormat::Zip,
            &control(),
            |_| {},
        )
        .unwrap();

        assert!(outcome.errors.is_empty());
        assert!(output.is_file());

        let extract_dest = dir.path().join("roundtrip");
        let extracted = super::super::extract::extract_archive(
            &VeyraPath::from_local(output),
            &VeyraPath::from_local(extract_dest.clone()),
            &control(),
            |_| {},
        )
        .unwrap();
        assert!(extracted.errors.is_empty());
        assert_eq!(
            fs::read(extract_dest.join("project/top.txt")).unwrap(),
            b"top"
        );
        assert_eq!(
            fs::read(extract_dest.join("project/nested/inner.txt")).unwrap(),
            b"inner"
        );
    }

    #[test]
    fn tar_gz_round_trips_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        fs::write(&src, b"hello").unwrap();

        let output = dir.path().join("out.tar.gz");
        create_archive(
            &VeyraPath::from_local(output.clone()),
            &[VeyraPath::from_local(src)],
            ArchiveFormat::TarGz,
            &control(),
            |_| {},
        )
        .unwrap();

        let extract_dest = dir.path().join("roundtrip");
        let outcome = super::super::extract::extract_archive(
            &VeyraPath::from_local(output),
            &VeyraPath::from_local(extract_dest.clone()),
            &control(),
            |_| {},
        )
        .unwrap();
        assert!(outcome.errors.is_empty());
        assert_eq!(fs::read(extract_dest.join("a.txt")).unwrap(), b"hello");
    }

    #[test]
    fn sevenzip_round_trips_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        fs::write(&src, b"hello 7z").unwrap();

        let output = dir.path().join("out.7z");
        create_archive(
            &VeyraPath::from_local(output.clone()),
            &[VeyraPath::from_local(src)],
            ArchiveFormat::SevenZip,
            &control(),
            |_| {},
        )
        .unwrap();

        let extract_dest = dir.path().join("roundtrip");
        let outcome = super::super::extract::extract_archive(
            &VeyraPath::from_local(output),
            &VeyraPath::from_local(extract_dest.clone()),
            &control(),
            |_| {},
        )
        .unwrap();
        assert!(outcome.errors.is_empty());
        assert_eq!(fs::read(extract_dest.join("a.txt")).unwrap(), b"hello 7z");
    }

    #[test]
    fn compress_never_follows_symlinks() {
        #[cfg(unix)]
        {
            let dir = tempfile::tempdir().unwrap();
            let real = dir.path().join("real.txt");
            fs::write(&real, b"real").unwrap();
            let link = dir.path().join("link.txt");
            std::os::unix::fs::symlink(&real, &link).unwrap();

            let output = dir.path().join("out.zip");
            let outcome = create_archive(
                &VeyraPath::from_local(output),
                &[VeyraPath::from_local(link)],
                ArchiveFormat::Zip,
                &control(),
                |_| {},
            )
            .unwrap();

            assert!(outcome.extracted.is_empty());
            assert_eq!(outcome.skipped.len(), 1);
            assert_eq!(outcome.skipped[0].1, SkipReason::Symlink);
        }
    }

    #[test]
    fn cancel_removes_the_tmp_file_and_leaves_no_output() {
        let dir = tempfile::tempdir().unwrap();
        let mut sources = Vec::new();
        for name in ["a.txt", "b.txt", "c.txt"] {
            let path = dir.path().join(name);
            fs::write(&path, b"payload").unwrap();
            sources.push(VeyraPath::from_local(path));
        }

        let output = dir.path().join("out.zip");
        let control = control();
        let control_for_progress = control.clone();
        let outcome = create_archive(
            &VeyraPath::from_local(output.clone()),
            &sources,
            ArchiveFormat::Zip,
            &control,
            move |_| control_for_progress.cancel(),
        )
        .unwrap();

        assert!(outcome.cancelled);
        assert!(!output.exists());
        assert!(!tmp_sibling(&output).exists());
    }
}
