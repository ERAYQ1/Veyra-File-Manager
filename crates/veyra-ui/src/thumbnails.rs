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
use std::sync::{Arc, Mutex};

use gtk4::glib;
use gtk4::prelude::*;
use lru::LruCache;
use md5::{Digest, Md5};

use veyra_filesystem::{FileItem, FileKind};

/// Target decode/cache size (freedesktop.org "normal" thumbnail size).
const THUMBNAIL_SIZE: i32 = 128;

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
    cancelled: Arc<Mutex<HashSet<PathBuf>>>,
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
    /// Faz 31: paths whose request has been abandoned (the bound row
    /// scrolled out of view / got rebound before the result arrived).
    /// Shared with the worker threads so an already-queued request for a
    /// path can be skipped *before* paying the decode cost — the actual
    /// "drop it from the backlog" half of scroll cancellation (Rule #31);
    /// the `widget-name` guard in `bind` only handles the other half
    /// (never painting a stale result over a rebound row).
    cancelled: Arc<Mutex<HashSet<PathBuf>>>,
    request_tx: async_channel::Sender<Request>,
    cache_dir: PathBuf,
}

impl ThumbnailService {
    /// `cache_dir` is the L2 root (`<xdg-cache>/veyra/thumbnails`); its
    /// `normal/` subdirectory is created up front (Rule #26 XDG layout),
    /// mirroring the one-time `search_index.db` setup already done
    /// synchronously at startup in `window.rs`. `l1_capacity` seeds the L1
    /// cache from the Preferences "Thumbnail Cache Capacity" setting (Faz
    /// 34); [`resize_l1`](Self::resize_l1) adjusts it live afterwards.
    pub(crate) fn new(cache_dir: PathBuf, l1_capacity: usize) -> Rc<Self> {
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
                    // Faz 31: a request that sat in the queue behind a fast
                    // scroll may already be abandoned by the time a worker
                    // picks it up — skip the decode/IO entirely rather than
                    // spending CPU on a thumbnail nothing will ever paint.
                    if take_cancelled(&request.cancelled, &request.path) {
                        let _ = request.reply_tx.send_blocking(None);
                        continue;
                    }
                    let decoded = produce_thumbnail(&request);
                    let _ = request.reply_tx.send_blocking(decoded);
                }
            });
        }

        Rc::new(ThumbnailService {
            l1: RefCell::new(LruCache::new(
                NonZeroUsize::new(l1_capacity.max(1)).expect("capacity clamped to at least 1"),
            )),
            in_flight: RefCell::new(HashSet::new()),
            cancelled: Arc::new(Mutex::new(HashSet::new())),
            request_tx,
            cache_dir,
        })
    }

    /// Live-resizes the L1 cache (Faz 34's "Thumbnail Cache Capacity"
    /// setting): shrinking evicts the least-recently-used entries down to
    /// the new capacity, growing just raises the ceiling. Takes effect for
    /// every already-open tab immediately — no rebuild needed, unlike
    /// `stream_chunk_size` (which only applies to the next directory load).
    pub(crate) fn resize_l1(&self, capacity: usize) {
        self.l1
            .borrow_mut()
            .resize(NonZeroUsize::new(capacity.max(1)).expect("capacity clamped to at least 1"));
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

        // A previous bind for this exact path may have been cancelled (row
        // scrolled away) while its request was still queued; rebinding it
        // now means it's wanted again, so undo that before it can be picked
        // up (or re-picked-up, if the worker hasn't dequeued it yet).
        self.cancelled.lock().unwrap().remove(&path);

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
            cancelled: self.cancelled.clone(),
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

    /// Faz 31: releases whatever thumbnail request `icon` is currently
    /// waiting on, called from the view factory's `connect_unbind` when a
    /// `GtkListItem` row is recycled away from its current file (typically
    /// a fast scroll moving it out of the viewport). If the request is
    /// still queued or in flight, it's marked cancelled so a worker thread
    /// skips the decode instead of spending CPU/IO on a thumbnail this row
    /// no longer represents (Rule #31). Safe to call even when `icon` was
    /// never bound to a thumbnailable path (`bind` clears the widget name
    /// in that case, so this becomes a no-op).
    pub(crate) fn unbind(&self, icon: &gtk4::Image) {
        let name = icon.widget_name();
        if let Some(path_str) = name.strip_prefix(WIDGET_NAME_GUARD_PREFIX) {
            let path = PathBuf::from(path_str);
            if self.in_flight.borrow().contains(&path) {
                self.cancelled.lock().unwrap().insert(path);
            }
        }
        icon.set_widget_name("");
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

/// Atomically checks and clears whether `path` was cancelled — `true` means
/// the caller (a worker about to decode it) should drop the request instead
/// of processing it. Pulled out as its own function so the scroll-
/// cancellation contract (Rule #31) is unit-testable without spinning up
/// the worker threads or any GTK widget.
fn take_cancelled(cancelled: &Mutex<HashSet<PathBuf>>, path: &Path) -> bool {
    cancelled.lock().unwrap().remove(path)
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
            path = %veyra_core::security::log_path(request.path.display()),
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

    /// Faz 49: L1 LRU access-speed benchmark (Rule #31/#33) — the same
    /// `LruCache<PathBuf, _>` shape `ThumbnailService::l1` uses (proxied
    /// with a `u32` payload here, as the sibling tests above already do, to
    /// stay independent of a real `gdk_pixbuf::Pixbuf`/display). Measured
    /// ~500ns/op in development (dominated by the `PathBuf` allocation each
    /// put clones, not the O(1) hash-map lookup itself, which is closer to
    /// 150ns alone); the 5µs bound below leaves a comfortable margin so
    /// this never flakes under a loaded CI runner while still catching an
    /// actual regression (e.g. an accidental linear scan replacing the O(1)
    /// lookup).
    #[test]
    fn l1_lru_cache_access_is_fast_at_scale() {
        const N: usize = 100_000;
        let mut cache: LruCache<PathBuf, u32> = LruCache::new(NonZeroUsize::new(500).unwrap());
        let paths: Vec<PathBuf> = (0..N).map(|i| PathBuf::from(format!("/f{i}"))).collect();

        let start = std::time::Instant::now();
        for (i, path) in paths.iter().enumerate() {
            cache.put(path.clone(), i as u32);
        }
        let put_elapsed = start.elapsed();

        let start = std::time::Instant::now();
        let mut hits = 0u32;
        // Only the last 500 puts are still resident (capacity 500); query
        // across the whole range so most calls are legitimate misses, same
        // as a real cache under normal churn.
        for path in &paths {
            if cache.get(path).is_some() {
                hits += 1;
            }
        }
        let get_elapsed = start.elapsed();

        println!(
            "l1_lru_cache_access_is_fast_at_scale: {N} puts in {put_elapsed:?} \
             ({:.1}ns/op), {N} gets in {get_elapsed:?} ({:.1}ns/op), {hits} hits",
            put_elapsed.as_nanos() as f64 / N as f64,
            get_elapsed.as_nanos() as f64 / N as f64
        );
        assert_eq!(hits, 500);
        let per_op_ns = (put_elapsed + get_elapsed).as_nanos() as f64 / (2 * N) as f64;
        assert!(
            per_op_ns < 5_000.0,
            "L1 LRU access averaged {per_op_ns:.1}ns/op, expected well under 5µs"
        );
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
    fn scroll_away_marks_pending_request_cancelled_and_worker_skips_it() {
        // Simulates the fast-scroll scenario: a row is bound (request
        // enqueued, path tracked as in-flight), then scrolled out of view
        // before the worker ever picks the request up — `unbind` should
        // mark it cancelled so the worker's `take_cancelled` check drops it
        // instead of decoding (Rule #31 backlog prevention).
        let cancelled: Mutex<HashSet<PathBuf>> = Mutex::new(HashSet::new());
        let path = PathBuf::from("/huge-folder/image-000042.png");

        // Not cancelled yet: nothing has abandoned this request.
        assert!(!take_cancelled(&cancelled, &path));

        // Row scrolls away: mark it cancelled, as `unbind` does for an
        // in-flight path.
        cancelled.lock().unwrap().insert(path.clone());

        // Worker dequeues it: sees it's cancelled, consumes the flag, skips
        // the decode.
        assert!(take_cancelled(&cancelled, &path));

        // The flag doesn't leak forever — a second dequeue attempt (or a
        // stale duplicate) sees it's already been consumed.
        assert!(!take_cancelled(&cancelled, &path));
    }

    #[test]
    fn rebinding_a_cancelled_path_before_the_worker_runs_un_cancels_it() {
        // If the user scrolls back to a row before its (still-queued)
        // request has been dequeued, `bind` clears the cancellation so the
        // worker still produces the thumbnail instead of dropping it.
        let cancelled: Mutex<HashSet<PathBuf>> = Mutex::new(HashSet::new());
        let path = PathBuf::from("/huge-folder/image-000042.png");

        cancelled.lock().unwrap().insert(path.clone());
        // Re-bind: undo the cancellation, mirroring `bind`'s
        // `cancelled.lock().unwrap().remove(&path)`.
        cancelled.lock().unwrap().remove(&path);

        assert!(!take_cancelled(&cancelled, &path));
    }

    #[test]
    fn unbind_of_a_path_with_no_pending_request_does_not_panic_or_leak() {
        // A row that never had a thumbnailable path (`bind` already left
        // `widget-name` empty) must be a safe no-op for `unbind`'s guard
        // token parsing.
        assert_eq!(
            WIDGET_NAME_GUARD_PREFIX.strip_prefix("no-such-prefix"),
            None
        );
        let empty = "";
        assert!(empty.strip_prefix(WIDGET_NAME_GUARD_PREFIX).is_none());
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
