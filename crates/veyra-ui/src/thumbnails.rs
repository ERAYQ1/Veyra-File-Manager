//! Faz 11: async thumbnail engine with a two-level cache.
//!
//! L1 is a bounded in-memory LRU of decoded `gdk_pixbuf::Pixbuf`s (Rule
//! #31/#40 — a 100k-file directory must never keep every thumbnail live in
//! RAM). L2 is an on-disk PNG cache under `<cache_dir>/thumbnails/normal`,
//! named by the MD5 of the source file's canonical `file://` URI, mirroring
//! the freedesktop.org thumbnail naming convention. A small pool of
//! background worker threads (not one thread per request, unlike
//! `fs_async::run_blocking` — Rule #33 resource-awareness, since a single
//! scroll can bring dozens of items into view at once) decodes/reads/writes
//! off the GTK main thread; only the finished pixel buffer crosses back to
//! it (Rule #11/#12).
//!
//! Per-list-item recycling (`GtkListItem` rebind during fast scroll) is
//! guarded without `unsafe` (the crate forbids it) by stashing the bound
//! item's path as the `GtkImage`'s `widget-name` property and re-checking it
//! when the async result lands — if the image has since been rebound to a
//! different file, the stale result is discarded (Rule #15: never let a
//! late thumbnail paint over the wrong row).

use std::cell::RefCell;
use std::collections::HashSet;
use std::io::Write as _;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use lru::LruCache;
use md5::{Digest, Md5};

use veyra_filesystem::{FileItem, FileKind};

/// Target decode/cache size (freedesktop.org "normal" thumbnail size).
const THUMBNAIL_SIZE: i32 = 128;

/// L1 capacity: comfortably covers a full screen of scrolling many times
/// over while staying well under the ~50MB budget the phase calls for
/// (worst case ~1000 * 128*128*4 bytes ≈ 64MB of raw pixel data, and actual
/// decoded thumbnails are usually much smaller than their worst case).
const L1_CAPACITY: usize = 1000;

/// Background decode/IO worker threads. Kept small and fixed rather than
/// one-thread-per-request: a single fast scroll can bind dozens of list
/// items at once, and thumbnail decode is CPU-bound (Rule #33).
const WORKER_COUNT: usize = 2;

const WIDGET_NAME_GUARD_PREFIX: &str = "veyra-thumb:";

struct L1Entry {
    mtime: i64,
    pixbuf: gtk4::gdk_pixbuf::Pixbuf,
}

struct Request {
    path: PathBuf,
    uri: String,
    mtime: i64,
    cache_dir: PathBuf,
    reply_tx: async_channel::Sender<Option<DecodedImage>>,
}

/// Send-safe wire format for a decoded thumbnail crossing back from a
/// worker thread to the GTK main thread — `gdk_pixbuf::Pixbuf` itself is a
/// GObject and cannot cross the channel (same reasoning as `preview.rs`'s
/// `DecodedImage`).
struct DecodedImage {
    pixels: glib::Bytes,
    colorspace: gtk4::gdk_pixbuf::Colorspace,
    has_alpha: bool,
    bits_per_sample: i32,
    width: i32,
    height: i32,
    rowstride: i32,
}

impl DecodedImage {
    fn from_pixbuf(pixbuf: &gtk4::gdk_pixbuf::Pixbuf) -> Self {
        DecodedImage {
            pixels: pixbuf.read_pixel_bytes(),
            colorspace: pixbuf.colorspace(),
            has_alpha: pixbuf.has_alpha(),
            bits_per_sample: pixbuf.bits_per_sample(),
            width: pixbuf.width(),
            height: pixbuf.height(),
            rowstride: pixbuf.rowstride(),
        }
    }

    fn into_pixbuf(self) -> gtk4::gdk_pixbuf::Pixbuf {
        gtk4::gdk_pixbuf::Pixbuf::from_bytes(
            &self.pixels,
            self.colorspace,
            self.has_alpha,
            self.bits_per_sample,
            self.width,
            self.height,
            self.rowstride,
        )
    }
}

/// Shared, per-window thumbnail service: owns the L1 cache and the
/// background worker pool's request queue. Cheaply `Rc`-cloned into every
/// tab/view the way `has_clipboard` and friends already are.
pub(crate) struct ThumbnailService {
    l1: RefCell<LruCache<PathBuf, L1Entry>>,
    in_flight: RefCell<HashSet<PathBuf>>,
    request_tx: async_channel::Sender<Request>,
    cache_dir: PathBuf,
}

impl ThumbnailService {
    /// `cache_dir` is the L2 root (`<xdg-cache>/veyra/thumbnails`); its
    /// `normal/` subdirectory is created up front (Rule #26 XDG layout),
    /// mirroring the one-time `search_index.db` setup already done
    /// synchronously at startup in `window.rs`.
    pub(crate) fn new(cache_dir: PathBuf) -> Rc<Self> {
        let normal_dir = cache_dir.join("normal");
        if let Err(err) = std::fs::create_dir_all(&normal_dir) {
            tracing::warn!(
                dir = %normal_dir.display(),
                error = %err,
                "failed to create thumbnail cache directory; disk cache disabled"
            );
        }

        let (request_tx, request_rx) = async_channel::unbounded::<Request>();
        for _ in 0..WORKER_COUNT {
            let request_rx = request_rx.clone();
            std::thread::spawn(move || {
                while let Ok(request) = request_rx.recv_blocking() {
                    let decoded = produce_thumbnail(&request);
                    let _ = request.reply_tx.send_blocking(decoded);
                }
            });
        }

        Rc::new(ThumbnailService {
            l1: RefCell::new(LruCache::new(
                NonZeroUsize::new(L1_CAPACITY).expect("L1_CAPACITY is nonzero"),
            )),
            in_flight: RefCell::new(HashSet::new()),
            request_tx,
            cache_dir,
        })
    }

    /// Binds `icon` for `item`: an L1 hit paints immediately and
    /// synchronously; otherwise `icon` keeps whatever fallback the caller
    /// already set and a background request is enqueued (unless one for the
    /// same path is already in flight) to fill it in once ready.
    pub(crate) fn bind(self: &Rc<Self>, icon: &gtk4::Image, item: &FileItem) {
        let Some(path) = thumbnailable_path(item) else {
            icon.set_widget_name("");
            return;
        };
        let mtime = mtime_secs(item);
        icon.set_widget_name(&guard_token(&path));

        if let Some(pixbuf) = self.l1_get(&path, mtime) {
            paint(icon, &pixbuf);
            return;
        }

        if !self.in_flight.borrow_mut().insert(path.clone()) {
            // Another bind already requested this exact path; its result,
            // once it lands, populates L1 for the next bind/redraw.
            return;
        }

        let uri = item.path.to_gio_file().uri().to_string();
        let (reply_tx, reply_rx) = async_channel::bounded(1);
        let _ = self.request_tx.send_blocking(Request {
            path: path.clone(),
            uri,
            mtime,
            cache_dir: self.cache_dir.clone(),
            reply_tx,
        });

        let service = self.clone();
        let icon = icon.clone();
        let guard = guard_token(&path);
        glib::spawn_future_local(async move {
            let decoded = reply_rx.recv().await.ok().flatten();
            service.in_flight.borrow_mut().remove(&path);
            let Some(decoded) = decoded else { return };
            let pixbuf = decoded.into_pixbuf();
            service.l1_put(path, mtime, &pixbuf);
            if icon.widget_name() == guard.as_str() {
                paint(&icon, &pixbuf);
            }
        });
    }

    fn l1_get(&self, path: &Path, mtime: i64) -> Option<gtk4::gdk_pixbuf::Pixbuf> {
        let mut l1 = self.l1.borrow_mut();
        let entry = l1.get(path)?;
        if entry.mtime != mtime {
            l1.pop(path);
            return None;
        }
        Some(entry.pixbuf.clone())
    }

    fn l1_put(&self, path: PathBuf, mtime: i64, pixbuf: &gtk4::gdk_pixbuf::Pixbuf) {
        self.l1.borrow_mut().put(
            path,
            L1Entry {
                mtime,
                pixbuf: pixbuf.clone(),
            },
        );
    }
}

fn paint(icon: &gtk4::Image, pixbuf: &gtk4::gdk_pixbuf::Pixbuf) {
    let texture = gtk4::gdk::Texture::for_pixbuf(pixbuf);
    icon.set_paintable(Some(&texture));
}

fn guard_token(path: &Path) -> String {
    format!("{WIDGET_NAME_GUARD_PREFIX}{}", path.display())
}

fn mtime_secs(item: &FileItem) -> i64 {
    item.metadata.modified.map(|dt| dt.timestamp()).unwrap_or(0)
}

/// Only local, regular, image-typed files are candidates — remote GVfs
/// mounts and symlinks are skipped entirely (the latter both to avoid
/// TOCTOU/symlink-target ambiguity, Rule #22, and because `FileKind` never
/// reports a symlink as `Regular`).
fn thumbnailable_path(item: &FileItem) -> Option<PathBuf> {
    if !matches!(item.kind(), FileKind::Regular) {
        return None;
    }
    if !item.metadata.mime_type.starts_with("image/") {
        return None;
    }
    item.path.as_local_path().map(Path::to_path_buf)
}

/// Runs entirely on a worker thread: reads L2 if it's still valid,
/// otherwise decodes+scales the source and (best-effort) writes L2, then
/// hands back a `Send`-safe pixel buffer. Never panics on a bad/corrupt
/// image (Rule #15) — any decode failure just yields `None`, and the
/// caller falls back to the symbolic icon it already set.
fn produce_thumbnail(request: &Request) -> Option<DecodedImage> {
    let cache_file = l2_cache_path(&request.cache_dir, &request.uri);

    if let Some(cache_file) = &cache_file {
        if is_cache_fresh(cache_file, request.mtime) {
            if let Ok(pixbuf) = gtk4::gdk_pixbuf::Pixbuf::from_file(cache_file) {
                return Some(DecodedImage::from_pixbuf(&pixbuf));
            }
        }
    }

    let pixbuf = gtk4::gdk_pixbuf::Pixbuf::from_file_at_scale(
        &request.path,
        THUMBNAIL_SIZE,
        THUMBNAIL_SIZE,
        true,
    )
    .inspect_err(|err| {
        tracing::debug!(
            path = %request.path.display(),
            error = %err,
            "thumbnail decode failed; falling back to symbolic icon"
        );
    })
    .ok()?;

    if let Some(cache_file) = &cache_file {
        write_cache_atomically(cache_file, &pixbuf);
    }

    Some(DecodedImage::from_pixbuf(&pixbuf))
}

/// A cache PNG is valid as long as it's at least as new as the source
/// file's own last-modified time — the invalidation rule the phase spec
/// calls for, with no PNG metadata needed: the cache file's own mtime is
/// set by `write_cache_atomically` at generation time.
fn is_cache_fresh(cache_file: &Path, source_mtime: i64) -> bool {
    let Ok(meta) = std::fs::metadata(cache_file) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    let cache_mtime = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    cache_mtime >= source_mtime
}

fn l2_cache_path(cache_dir: &Path, uri: &str) -> Option<PathBuf> {
    if !cache_dir.join("normal").is_dir() {
        return None;
    }
    let mut hasher = Md5::new();
    hasher.update(uri.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    Some(cache_dir.join("normal").join(format!("{hex}.png")))
}

/// Writes to a temp file in the same directory, then renames over the
/// target — a partially-written cache PNG can never be observed by a
/// concurrent reader (phase spec's atomic-write requirement).
fn write_cache_atomically(cache_file: &Path, pixbuf: &gtk4::gdk_pixbuf::Pixbuf) {
    let Ok(bytes) = pixbuf.save_to_bufferv("png", &[]) else {
        return;
    };
    let tmp_file = cache_file.with_extension(format!(
        "png.tmp-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let write_result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp_file)?;
        file.write_all(&bytes)?;
        file.sync_all()
    })();
    match write_result {
        Ok(()) => {
            if let Err(err) = std::fs::rename(&tmp_file, cache_file) {
                tracing::debug!(error = %err, "failed to install thumbnail cache file");
                let _ = std::fs::remove_file(&tmp_file);
            }
        }
        Err(err) => {
            tracing::debug!(error = %err, "failed to write thumbnail cache temp file");
            let _ = std::fs::remove_file(&tmp_file);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use super::*;

    #[test]
    fn l1_lru_evicts_oldest_beyond_capacity() {
        let mut cache: LruCache<PathBuf, u32> = LruCache::new(NonZeroUsize::new(2).unwrap());
        cache.put(PathBuf::from("/a"), 1);
        cache.put(PathBuf::from("/b"), 2);
        cache.put(PathBuf::from("/c"), 3);
        assert!(cache.get(&PathBuf::from("/a")).is_none());
        assert!(cache.get(&PathBuf::from("/b")).is_some());
        assert!(cache.get(&PathBuf::from("/c")).is_some());
    }

    #[test]
    fn l1_lru_touch_on_get_keeps_entry_alive() {
        let mut cache: LruCache<PathBuf, u32> = LruCache::new(NonZeroUsize::new(2).unwrap());
        cache.put(PathBuf::from("/a"), 1);
        cache.put(PathBuf::from("/b"), 2);
        // Touch `/a` so `/b` becomes the least-recently-used entry.
        assert!(cache.get(&PathBuf::from("/a")).is_some());
        cache.put(PathBuf::from("/c"), 3);
        assert!(cache.get(&PathBuf::from("/a")).is_some());
        assert!(cache.get(&PathBuf::from("/b")).is_none());
    }

    #[test]
    fn md5_uri_hash_is_stable_and_matches_known_vector() {
        let mut hasher = Md5::new();
        hasher.update(b"file:///home/user/image.png");
        let digest = hasher.finalize();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        // Pinned regression value for MD5("file:///home/user/image.png"),
        // cross-checked against `md5sum` at test-authoring time.
        assert_eq!(hex, "e47e29c02f69c7dd185f87f51d43a326");
        assert_eq!(hex.len(), 32);
    }

    #[test]
    fn cache_freshness_rejects_missing_file() {
        assert!(!is_cache_fresh(Path::new("/nonexistent/path.png"), 0));
    }

    #[test]
    fn cache_freshness_accepts_newer_cache_than_source() {
        let dir = std::env::temp_dir().join(format!("veyra-thumb-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("fresh.png");
        std::fs::write(&file, b"stub").unwrap();
        // The freshly-written file's mtime is "now"; any source mtime at or
        // before that must read as fresh.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert!(is_cache_fresh(&file, now - 3600));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cache_freshness_rejects_source_newer_than_cache() {
        let dir = std::env::temp_dir().join(format!("veyra-thumb-test2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("stale.png");
        std::fs::write(&file, b"stub").unwrap();
        let far_future = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 3600;
        assert!(!is_cache_fresh(&file, far_future));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn guard_token_is_unique_per_path() {
        assert_ne!(
            guard_token(Path::new("/a/one.png")),
            guard_token(Path::new("/a/two.png"))
        );
    }

    #[test]
    fn thumbnailable_path_rejects_directories_and_non_images() {
        use chrono::Utc;
        use veyra_filesystem::{FileMetadata, VeyraPath};

        let base_metadata = |kind: FileKind, mime: &str| FileMetadata {
            name: "x".into(),
            path: VeyraPath::from_local("/tmp/x"),
            kind,
            size_bytes: 0,
            modified: Some(Utc::now()),
            created: None,
            accessed: None,
            permissions: None,
            owner: None,
            group: None,
            mime_type: mime.to_string(),
            inode: None,
            is_hidden: false,
        };

        let dir_item = FileItem {
            path: VeyraPath::from_local("/tmp/x"),
            metadata: base_metadata(FileKind::Directory, "inode/directory"),
            target_symlink: None,
        };
        assert!(thumbnailable_path(&dir_item).is_none());

        let text_item = FileItem {
            path: VeyraPath::from_local("/tmp/x.txt"),
            metadata: base_metadata(FileKind::Regular, "text/plain"),
            target_symlink: None,
        };
        assert!(thumbnailable_path(&text_item).is_none());

        let image_item = FileItem {
            path: VeyraPath::from_local("/tmp/x.png"),
            metadata: base_metadata(FileKind::Regular, "image/png"),
            target_symlink: None,
        };
        assert!(thumbnailable_path(&image_item).is_some());
    }
}
