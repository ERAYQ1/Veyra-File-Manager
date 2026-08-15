//! Faz 15: Recent Files & Privacy.
//!
//! `recent:///` is not backed by a real GVfs mount on most distros, so
//! unlike every other location `window.rs` navigates to, it can't just be
//! `veyra_filesystem::read_dir`'d — this module builds its listing directly
//! from the XDG recently-used registry (`gtk4::RecentManager`, which reads
//! `~/.local/share/recently-used.xbel`) instead.
//!
//! `RecentManager` itself is main-thread-only (asserted by the bindings), so
//! listing happens in two steps: [`snapshot_entries`] grabs the (already
//! in-memory, no I/O) URI + last-visited list on the GTK main thread, then
//! [`list_recent_items`] does the actual per-entry `stat` off-thread, via
//! `fs_async::run_blocking` in `window.rs` — same non-blocking-UI contract
//! every other listing follows (Rule #11/#12).

use std::cell::RefCell;
use std::rc::Rc;

use chrono::{DateTime, Duration, Utc};
use gtk4::prelude::*;

use veyra_filesystem::{FileItem, VeyraPath};

/// The virtual location the sidebar's "Recent" entry navigates to.
pub(crate) const RECENT_URI: &str = "recent:///";

/// Whether `path` is Veyra's virtual Recent Files location.
pub(crate) fn is_recent_location(path: &VeyraPath) -> bool {
    matches!(path, VeyraPath::Uri(uri) if uri == RECENT_URI)
}

/// The recency bucket a recent entry falls into, relative to "now". Used to
/// group `recent:///`'s listing (Today / Yesterday / This Week / Older).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TimeGroup {
    Today,
    Yesterday,
    ThisWeek,
    Older,
}

impl TimeGroup {
    pub(crate) fn label(self) -> &'static str {
        match self {
            TimeGroup::Today => "Today",
            TimeGroup::Yesterday => "Yesterday",
            TimeGroup::ThisWeek => "This Week",
            TimeGroup::Older => "Older",
        }
    }

    /// Classifies `accessed` against `now` by calendar day (Today/
    /// Yesterday) and by a rolling 7-day window (This Week), falling back to
    /// `Older` beyond that.
    pub(crate) fn classify(accessed: DateTime<Utc>, now: DateTime<Utc>) -> TimeGroup {
        let today = now.date_naive();
        let accessed_day = accessed.date_naive();

        if accessed_day == today {
            TimeGroup::Today
        } else if accessed_day == today - Duration::days(1) {
            TimeGroup::Yesterday
        } else if now.signed_duration_since(accessed) <= Duration::days(7) {
            TimeGroup::ThisWeek
        } else {
            TimeGroup::Older
        }
    }
}

/// Snapshots every URI `gtk4::RecentManager::default()` currently knows
/// about, paired with its last-visited timestamp (Unix seconds). Cheap and
/// GTK-main-thread-only: `RecentManager::items()` just reads the registry's
/// already-parsed in-memory bookmark list, no disk or `stat` I/O — the
/// existence checks that actually touch the filesystem happen later, off
/// the main thread, in [`list_recent_items`].
pub(crate) fn snapshot_entries() -> Vec<(String, i64)> {
    gtk4::RecentManager::default()
        .items()
        .into_iter()
        .map(|info| (info.uri().to_string(), info.visited().to_unix()))
        .collect()
}

/// Builds `recent:///`'s listing from a [`snapshot_entries`] result: `stat`s
/// each URI (skipping entries whose target no longer exists — Rule #18/#20,
/// no panics on stale recent-file records) and stamps the resulting
/// `FileItem`'s `accessed` metadata with the registry's last-visited time
/// (more meaningful for "recent" than on-disk atime, which many filesystems
/// don't even track). Sorted by that timestamp, descending, matching
/// `recent:///`'s default `SortKey::Accessed` sort in `window.rs`.
///
/// Blocking (does one `stat` per entry) — always call off the GTK main
/// thread.
pub(crate) fn list_recent_items(entries: Vec<(String, i64)>) -> Vec<FileItem> {
    let mut items: Vec<FileItem> = entries
        .into_iter()
        .filter_map(|(uri, visited_unix)| {
            let mut item = veyra_filesystem::stat(&VeyraPath::from_uri(uri)).ok()?;
            item.metadata.accessed = DateTime::<Utc>::from_timestamp(visited_unix, 0);
            Some(item)
        })
        .collect();

    items.sort_by_key(|item| std::cmp::Reverse(item.metadata.accessed));
    items
}

/// Records `uri` as opened in the XDG recent-files registry, unless privacy
/// mode is enabled. Must run on the GTK main thread (`RecentManager`
/// requirement) — called from `window.rs::open_item` before it spawns the
/// background thread that actually launches the file.
pub(crate) fn record_opened(uri: &str, privacy_mode: &Rc<RefCell<bool>>) {
    if *privacy_mode.borrow() {
        return;
    }
    gtk4::RecentManager::default().add_item(uri);
}

/// Purges every entry from the XDG recent-files registry.
pub(crate) fn clear_history() {
    if let Err(err) = gtk4::RecentManager::default().purge_items() {
        tracing::warn!(error = %err, "failed to clear recent file history");
    }
}

/// The `recent:///`-only info row a panel reveals above its tab strip:
/// "Clear History" and a "Privacy Mode" switch (Faz 15-B3). Built once per
/// panel in `split_view::build_panel`; `window.rs::update_chrome` reveals it
/// (and syncs the switch) whenever that panel's active tab is showing
/// `recent:///`.
#[derive(Clone)]
pub(crate) struct RecentBannerHandles {
    pub revealer: gtk4::Revealer,
    /// Reflects the current listing's Today/Yesterday/This Week/Older
    /// breakdown (`window.rs::on_directory_loaded`, via
    /// [`format_group_summary`]) whenever `recent:///` is showing.
    pub title_label: gtk4::Label,
    pub clear_button: gtk4::Button,
    pub privacy_switch: gtk4::Switch,
}

/// Renders `items`' Today/Yesterday/This Week/Older breakdown (Faz 15-A2) as
/// the Recent Files banner's title, e.g. `"Recent Files — 3 Today, 12 This
/// Week"`. Empty groups are omitted; an empty listing falls back to the bare
/// title.
pub(crate) fn format_group_summary(items: &[FileItem], now: DateTime<Utc>) -> String {
    let mut counts = [0u32; 4];
    for item in items {
        let Some(accessed) = item.metadata.accessed else {
            continue;
        };
        let group = TimeGroup::classify(accessed, now);
        counts[group as usize] += 1;
    }

    let groups = [
        TimeGroup::Today,
        TimeGroup::Yesterday,
        TimeGroup::ThisWeek,
        TimeGroup::Older,
    ];
    let parts: Vec<String> = groups
        .into_iter()
        .zip(counts)
        .filter(|(_, count)| *count > 0)
        .map(|(group, count)| format!("{count} {}", group.label()))
        .collect();

    if parts.is_empty() {
        "Recent Files".to_string()
    } else {
        format!("Recent Files — {}", parts.join(", "))
    }
}

/// Builds the (initially collapsed) Recent Files banner.
pub(crate) fn build_banner(privacy_mode: Rc<RefCell<bool>>) -> RecentBannerHandles {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    row.set_margin_start(12);
    row.set_margin_end(12);
    row.set_margin_top(6);
    row.set_margin_bottom(6);
    row.add_css_class("card");

    let title_label = gtk4::Label::new(Some("Recent Files"));
    title_label.set_hexpand(true);
    title_label.set_xalign(0.0);
    row.append(&title_label);

    let switch_label = gtk4::Label::new(Some("Privacy Mode"));
    row.append(&switch_label);

    let privacy_switch = gtk4::Switch::new();
    privacy_switch.set_valign(gtk4::Align::Center);
    privacy_switch.set_tooltip_text(Some(
        "When on, opening files here won't be added to Recent Files",
    ));
    privacy_switch.update_property(&[gtk4::accessible::Property::Label("Privacy Mode")]);
    {
        let privacy_mode = privacy_mode.clone();
        privacy_switch.connect_state_set(move |switch, state| {
            *privacy_mode.borrow_mut() = state;
            switch.set_state(state);
            gtk4::glib::Propagation::Stop
        });
    }
    row.append(&privacy_switch);

    let clear_button = gtk4::Button::with_label("Clear History");
    clear_button.add_css_class("destructive-action");
    row.append(&clear_button);

    let revealer = gtk4::Revealer::new();
    revealer.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
    revealer.set_child(Some(&row));
    revealer.set_reveal_child(false);

    RecentBannerHandles {
        revealer,
        title_label,
        clear_button,
        privacy_switch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(secs: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(secs, 0).unwrap()
    }

    #[test]
    fn is_recent_location_matches_exact_uri() {
        assert!(is_recent_location(&VeyraPath::from_uri("recent:///")));
        assert!(!is_recent_location(&VeyraPath::from_uri("trash:///")));
        assert!(!is_recent_location(&VeyraPath::from_local("/home/user")));
    }

    #[test]
    fn classify_same_calendar_day_is_today() {
        let now = dt(1_000_000);
        assert_eq!(TimeGroup::classify(now, now), TimeGroup::Today);
    }

    #[test]
    fn classify_previous_calendar_day_is_yesterday() {
        let now = dt(1_700_000_000); // 2023-11-14T22:13:20Z
        let yesterday = now - Duration::days(1);
        assert_eq!(TimeGroup::classify(yesterday, now), TimeGroup::Yesterday);
    }

    #[test]
    fn classify_within_a_week_but_not_yesterday_is_this_week() {
        let now = dt(1_700_000_000);
        let five_days_ago = now - Duration::days(5);
        assert_eq!(TimeGroup::classify(five_days_ago, now), TimeGroup::ThisWeek);
    }

    #[test]
    fn classify_older_than_a_week_is_older() {
        let now = dt(1_700_000_000);
        let three_weeks_ago = now - Duration::weeks(3);
        assert_eq!(TimeGroup::classify(three_weeks_ago, now), TimeGroup::Older);
    }

    #[test]
    fn classify_boundary_at_exactly_seven_days_is_this_week() {
        let now = dt(1_700_000_000);
        let exactly_a_week_ago = now - Duration::days(7);
        assert_eq!(
            TimeGroup::classify(exactly_a_week_ago, now),
            TimeGroup::ThisWeek
        );
    }

    #[test]
    fn list_recent_items_skips_missing_files_and_sorts_by_visited_desc() {
        let dir = std::env::temp_dir().join(format!(
            "veyra-recent-test-{}-{}",
            std::process::id(),
            "list_recent_items_skips_missing_files_and_sorts_by_visited_desc"
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let older = dir.join("older.txt");
        let newer = dir.join("newer.txt");
        std::fs::write(&older, b"a").unwrap();
        std::fs::write(&newer, b"b").unwrap();

        let missing_uri = gio::File::for_path(dir.join("gone.txt")).uri().to_string();
        let older_uri = gio::File::for_path(&older).uri().to_string();
        let newer_uri = gio::File::for_path(&newer).uri().to_string();

        let items = list_recent_items(vec![
            (missing_uri, 500),
            (older_uri, 1_000),
            (newer_uri, 2_000),
        ]);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name(), "newer.txt");
        assert_eq!(items[1].name(), "older.txt");
        assert_eq!(items[0].metadata.accessed, Some(dt(2_000)));
        assert_eq!(items[1].metadata.accessed, Some(dt(1_000)));

        std::fs::remove_dir_all(&dir).ok();
    }
}
