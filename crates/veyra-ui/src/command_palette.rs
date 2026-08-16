//! Faz 24: the Command Palette's data model and scored fuzzy-search engine
//! (`Ctrl+K` / `Ctrl+Shift+P`, see `dialogs::command_palette_dialog`).
//!
//! Kept free of any GTK widget construction so the matching/ranking logic is
//! plain, 100% unit-testable Rust — the dialog module owns everything about
//! presenting `CommandItem`s on screen.

use gtk4::glib;
use gtk4::prelude::*;

/// One entry in the palette: a single `win.*` action a user can search for
/// and trigger, exactly as if they'd clicked its menu item or pressed its
/// shortcut.
#[derive(Clone)]
pub(crate) struct CommandItem {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub icon_name: &'static str,
    pub action_name: &'static str,
    /// The action's parameter, for the handful of commands backed by a
    /// parameterized action (`win.set-view-mode`, `win.sort-by`) rather than
    /// a dedicated no-argument action per choice.
    pub action_target: Option<glib::Variant>,
}

/// Every command the palette can list, grouped by `category` in the order
/// they should appear when the search query is empty.
pub(crate) fn all_commands() -> Vec<CommandItem> {
    vec![
        // --- File Operations ---
        CommandItem {
            id: "new-folder",
            title: "New Folder",
            category: "File Operations",
            icon_name: "folder-new-symbolic",
            action_name: "win.create-folder",
            action_target: None,
        },
        CommandItem {
            id: "new-document",
            title: "New Document",
            category: "File Operations",
            icon_name: "document-new-symbolic",
            action_name: "win.create-document",
            action_target: None,
        },
        CommandItem {
            id: "compress-selection",
            title: "Compress Selection…",
            category: "File Operations",
            icon_name: "package-x-generic-symbolic",
            action_name: "win.compress-selected",
            action_target: None,
        },
        CommandItem {
            id: "extract-archive",
            title: "Extract Archive…",
            category: "File Operations",
            icon_name: "archive-extract-symbolic",
            action_name: "win.extract-here-selected",
            action_target: None,
        },
        CommandItem {
            id: "empty-trash",
            title: "Empty Trash",
            category: "File Operations",
            icon_name: "user-trash-symbolic",
            action_name: "win.empty-trash",
            action_target: None,
        },
        CommandItem {
            id: "copy-path",
            title: "Copy Path",
            category: "File Operations",
            icon_name: "edit-copy-symbolic",
            action_name: "win.copy-path-selected",
            action_target: None,
        },
        CommandItem {
            id: "copy-location",
            title: "Copy Location",
            category: "File Operations",
            icon_name: "edit-copy-symbolic",
            action_name: "win.copy-location-selected",
            action_target: None,
        },
        CommandItem {
            id: "undo",
            title: "Undo",
            category: "File Operations",
            icon_name: "edit-undo-symbolic",
            action_name: "win.undo",
            action_target: None,
        },
        CommandItem {
            id: "redo",
            title: "Redo",
            category: "File Operations",
            icon_name: "edit-redo-symbolic",
            action_name: "win.redo",
            action_target: None,
        },
        // --- Navigation & Tabs ---
        CommandItem {
            id: "new-tab",
            title: "New Tab",
            category: "Navigation",
            icon_name: "tab-new-symbolic",
            action_name: "win.new-tab",
            action_target: None,
        },
        CommandItem {
            id: "close-tab",
            title: "Close Tab",
            category: "Navigation",
            icon_name: "window-close-symbolic",
            action_name: "win.close-tab",
            action_target: None,
        },
        CommandItem {
            id: "open-new-window",
            title: "Open in New Window",
            category: "Navigation",
            icon_name: "window-new-symbolic",
            action_name: "win.open-in-new-window-selected",
            action_target: None,
        },
        CommandItem {
            id: "toggle-split-view",
            title: "Toggle Dual Pane / Split View",
            category: "Navigation",
            icon_name: "sidebar-show-right-symbolic",
            action_name: "win.toggle-split-view",
            action_target: None,
        },
        CommandItem {
            id: "open-terminal",
            title: "Open Terminal Here",
            category: "Navigation",
            icon_name: "utilities-terminal-symbolic",
            action_name: "win.open-terminal-here-current",
            action_target: None,
        },
        CommandItem {
            id: "go-to-location",
            title: "Go to Location",
            category: "Navigation",
            icon_name: "folder-symbolic",
            action_name: "win.focus-address",
            action_target: None,
        },
        CommandItem {
            id: "select-all",
            title: "Select All",
            category: "Navigation",
            icon_name: "edit-select-all-symbolic",
            action_name: "win.select-all",
            action_target: None,
        },
        // --- View & Toggle ---
        CommandItem {
            id: "toggle-hidden",
            title: "Toggle Hidden Files",
            category: "View",
            icon_name: "view-reveal-symbolic",
            action_name: "win.toggle-hidden-files",
            action_target: None,
        },
        CommandItem {
            id: "toggle-preview",
            title: "Toggle File Preview",
            category: "View",
            icon_name: "view-preview-symbolic",
            action_name: "win.toggle-preview",
            action_target: None,
        },
        CommandItem {
            id: "view-icon",
            title: "Icon View",
            category: "View",
            icon_name: "view-grid-symbolic",
            action_name: "win.set-view-mode",
            action_target: Some("icon".to_variant()),
        },
        CommandItem {
            id: "view-compact",
            title: "Compact View",
            category: "View",
            icon_name: "view-continuous-symbolic",
            action_name: "win.set-view-mode",
            action_target: Some("compact".to_variant()),
        },
        CommandItem {
            id: "view-details",
            title: "Details View",
            category: "View",
            icon_name: "view-list-symbolic",
            action_name: "win.set-view-mode",
            action_target: Some("details".to_variant()),
        },
        // --- Tools ---
        CommandItem {
            id: "analyze-disk",
            title: "Analyze Disk Usage…",
            category: "Tools",
            icon_name: "drive-harddisk-symbolic",
            action_name: "win.analyze-disk-current",
            action_target: None,
        },
        CommandItem {
            id: "connect-server",
            title: "Connect to Server…",
            category: "Tools",
            icon_name: "network-server-symbolic",
            action_name: "win.connect-to-server",
            action_target: None,
        },
        CommandItem {
            id: "properties",
            title: "Open Properties",
            category: "Tools",
            icon_name: "document-properties-symbolic",
            action_name: "win.properties-selected",
            action_target: None,
        },
        CommandItem {
            id: "search-files",
            title: "Search Files",
            category: "Tools",
            icon_name: "system-search-symbolic",
            action_name: "win.toggle-search",
            action_target: None,
        },
        CommandItem {
            id: "manage-file-associations",
            title: "Manage File Associations…",
            category: "Tools",
            icon_name: "preferences-desktop-default-applications-symbolic",
            action_name: "win.manage-file-associations",
            action_target: None,
        },
        CommandItem {
            id: "preferences",
            title: "Preferences",
            category: "Tools",
            icon_name: "preferences-system-symbolic",
            action_name: "win.show-preferences",
            action_target: None,
        },
        CommandItem {
            id: "keyboard-shortcuts",
            title: "Keyboard Shortcuts",
            category: "Tools",
            icon_name: "input-keyboard-symbolic",
            action_name: "win.show-shortcuts-help",
            action_target: None,
        },
        CommandItem {
            id: "reset-shortcuts",
            title: "Reset Shortcuts to Default",
            category: "Tools",
            icon_name: "edit-undo-symbolic",
            action_name: "win.reset-shortcuts",
            action_target: None,
        },
        // --- Sort & Filter ---
        CommandItem {
            id: "sort-name",
            title: "Sort by Name",
            category: "Sort & Filter",
            icon_name: "view-sort-ascending-symbolic",
            action_name: "win.sort-by",
            action_target: Some("name".to_variant()),
        },
        CommandItem {
            id: "sort-size",
            title: "Sort by Size",
            category: "Sort & Filter",
            icon_name: "view-sort-ascending-symbolic",
            action_name: "win.sort-by",
            action_target: Some("size".to_variant()),
        },
        CommandItem {
            id: "sort-modified",
            title: "Sort by Modified",
            category: "Sort & Filter",
            icon_name: "view-sort-ascending-symbolic",
            action_name: "win.sort-by",
            action_target: Some("modified".to_variant()),
        },
        CommandItem {
            id: "sort-type",
            title: "Sort by Type",
            category: "Sort & Filter",
            icon_name: "view-sort-ascending-symbolic",
            action_name: "win.sort-by",
            action_target: Some("type".to_variant()),
        },
    ]
}

/// Scores how well `query` fuzzy-matches `target` (case-insensitive,
/// subsequence match — every query character must appear in `target`, in
/// order, though not necessarily contiguously). Returns `None` when `query`
/// doesn't match at all.
///
/// Higher is better. Bonuses stack: a query character landing right at a
/// word boundary (start of string, or just after a non-alphanumeric
/// character) scores extra, and runs of consecutive matching characters
/// score progressively more per run — both reward matches that read like the
/// query was typed as a recognizable fragment of the target rather than
/// scattered across it. An exact (case-insensitive) match or a match that's
/// a prefix of the target is boosted further still, so "New Tab" beats
/// "Toggle... Tab-ish thing" for the query "tab" is not guaranteed by
/// subsequence order alone.
pub(crate) fn fuzzy_score(query: &str, target: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    let query_lower = query.to_lowercase();
    let target_lower = target.to_lowercase();
    let query_chars: Vec<char> = query_lower.chars().collect();
    let target_chars: Vec<char> = target_lower.chars().collect();

    let mut score = 0i32;
    let mut query_index = 0usize;
    let mut prev_match_index: Option<usize> = None;
    let mut consecutive_run = 0i32;

    for (target_index, &target_char) in target_chars.iter().enumerate() {
        if query_index >= query_chars.len() {
            break;
        }
        if target_char != query_chars[query_index] {
            continue;
        }

        score += 1;

        let at_word_boundary =
            target_index == 0 || !target_chars[target_index - 1].is_alphanumeric();
        if at_word_boundary {
            score += 8;
        }

        if prev_match_index == Some(target_index.wrapping_sub(1)) {
            consecutive_run += 1;
            score += 5 * consecutive_run;
        } else {
            consecutive_run = 0;
        }

        prev_match_index = Some(target_index);
        query_index += 1;
    }

    if query_index < query_chars.len() {
        return None;
    }

    if target_lower == query_lower {
        score += 100;
    } else if target_lower.starts_with(query_lower.as_str()) {
        score += 50;
    }

    Some(score)
}

/// Filters+ranks `commands` against `query`. An empty (or whitespace-only)
/// query returns every command in its original, category-grouped order
/// (requirement A.3); otherwise, only commands whose title fuzzy-matches are
/// returned, highest score first, ties broken by original order (`sort_by`
/// is stable).
pub(crate) fn filter_commands<'a>(
    query: &str,
    commands: &'a [CommandItem],
) -> Vec<&'a CommandItem> {
    if query.trim().is_empty() {
        return commands.iter().collect();
    }

    let mut scored: Vec<(i32, &CommandItem)> = commands
        .iter()
        .filter_map(|command| fuzzy_score(query, command.title).map(|score| (score, command)))
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().map(|(_, command)| command).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_matches_everything_with_zero_score() {
        assert_eq!(fuzzy_score("", "Toggle Hidden Files"), Some(0));
    }

    #[test]
    fn subsequence_out_of_order_does_not_match() {
        assert_eq!(fuzzy_score("ba", "abc"), None);
    }

    #[test]
    fn non_matching_character_rejects() {
        assert_eq!(fuzzy_score("xyz", "New Tab"), None);
    }

    #[test]
    fn case_insensitive_match() {
        assert!(fuzzy_score("NEW", "new tab").is_some());
        assert!(fuzzy_score("new", "NEW TAB").is_some());
    }

    #[test]
    fn exact_match_scores_highest() {
        let exact = fuzzy_score("new tab", "New Tab").unwrap();
        let scattered = fuzzy_score("nwtb", "New Tab").unwrap();
        assert!(exact > scattered);
    }

    #[test]
    fn prefix_match_beats_non_prefix_subsequence() {
        let prefix = fuzzy_score("new", "New Tab").unwrap();
        let non_prefix = fuzzy_score("new", "Open in New Window").unwrap();
        assert!(prefix > non_prefix);
    }

    #[test]
    fn word_boundary_match_beats_mid_word_match() {
        let boundary = fuzzy_score("t", "Toggle Preview").unwrap();
        let mid_word = fuzzy_score("g", "Toggle Preview").unwrap();
        assert!(boundary > mid_word);
    }

    #[test]
    fn consecutive_run_beats_scattered_hits_of_equal_length() {
        let consecutive = fuzzy_score("tog", "Toggle Hidden Files").unwrap();
        let scattered = fuzzy_score("tgh", "Toggle Hidden Files").unwrap();
        assert!(consecutive > scattered);
    }

    #[test]
    fn filter_empty_query_preserves_category_order() {
        let commands = all_commands();
        let filtered = filter_commands("", &commands);
        assert_eq!(filtered.len(), commands.len());
        assert_eq!(filtered[0].id, commands[0].id);
        assert_eq!(filtered.last().unwrap().id, commands.last().unwrap().id);
    }

    #[test]
    fn filter_whitespace_only_query_behaves_like_empty() {
        let commands = all_commands();
        assert_eq!(
            filter_commands("   ", &commands).len(),
            filter_commands("", &commands).len()
        );
    }

    #[test]
    fn filter_ranks_best_match_first() {
        let commands = all_commands();
        let filtered = filter_commands("hidden", &commands);
        assert!(!filtered.is_empty());
        assert_eq!(filtered[0].id, "toggle-hidden");
    }

    #[test]
    fn filter_excludes_non_matching_commands() {
        let commands = all_commands();
        let filtered = filter_commands("zzzzzzzzzz", &commands);
        assert!(filtered.is_empty());
    }

    #[test]
    fn all_commands_have_unique_ids() {
        let commands = all_commands();
        let mut ids: Vec<&str> = commands.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), commands.len());
    }

    #[test]
    fn all_commands_target_a_win_action() {
        for command in all_commands() {
            assert!(
                command.action_name.starts_with("win."),
                "{} does not target a win.* action",
                command.id
            );
        }
    }
}
