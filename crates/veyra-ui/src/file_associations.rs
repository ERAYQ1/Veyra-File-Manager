//! Faz 27: File Associations engine — MIME type discovery and default
//! application management. Enumerates all registered content types across
//! the system (via gio::AppInfo), builds a searchable list of file types with
//! descriptions and default handlers, and wraps GIO's `set_as_default_for_type`
//! for persistence to XDG mimeapps.list.

use gtk4::gio;
use gtk4::prelude::*;

use crate::open_with;

/// One entry in the file types list: a registered MIME type with its
/// human description, icon, and current default application (if set).
#[derive(Clone)]
pub(crate) struct FileTypeEntry {
    pub content_type: String,
    pub description: String,
    pub icon: Option<gio::Icon>,
    pub default_app: Option<gio::AppInfo>,
}

/// Every registered MIME type on the system, collected from the union of
/// `app.supported_content_types()` across all applications. Deduplicated and
/// sorted by description (case-insensitive) then by content_type as a
/// stable tiebreak. Computed once when the dialog opens (not on every window
/// build, since it's the union across potentially hundreds of apps).
pub(crate) fn list_system_file_types() -> Vec<FileTypeEntry> {
    let mut types_set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    // Collect union of supported_content_types from all applications
    for app in gio::AppInfo::all() {
        for content_type in app.supported_types() {
            types_set.insert(content_type.to_string());
        }
    }

    // Build a FileTypeEntry for each unique content type, then sort by
    // description then content_type
    let mut entries: Vec<FileTypeEntry> = types_set
        .into_iter()
        .map(|content_type| {
            let description = gio::content_type_get_description(&content_type);
            let description = if description.is_empty() {
                content_type.clone()
            } else {
                description.to_string()
            };

            let icon = Some(gio::content_type_get_icon(&content_type));

            let default_app = open_with::default_app(&content_type);

            FileTypeEntry {
                content_type,
                description,
                icon,
                default_app,
            }
        })
        .collect();

    entries.sort_by(|a, b| {
        let cmp = a
            .description
            .to_lowercase()
            .cmp(&b.description.to_lowercase());
        if cmp == std::cmp::Ordering::Equal {
            a.content_type.cmp(&b.content_type)
        } else {
            cmp
        }
    });

    entries
}

/// Case-insensitive substring match against a file type entry's content_type,
/// description, or default application name, for the File Associations
/// dialog's live search filter. Mirrors `open_with::matches_query`'s behavior:
/// empty query matches everything; trimmed, case-insensitive substring search.
pub(crate) fn matches_query(
    entry: &FileTypeEntry,
    default_app_name: Option<&str>,
    query: &str,
) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();

    entry.content_type.to_lowercase().contains(&query)
        || entry.description.to_lowercase().contains(&query)
        || default_app_name
            .map(|name| name.to_lowercase().contains(&query))
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_matches_everything() {
        let entry = FileTypeEntry {
            content_type: "image/png".to_string(),
            description: "PNG image".to_string(),
            icon: None,
            default_app: None,
        };
        assert!(matches_query(&entry, None, ""));
        assert!(matches_query(&entry, None, "   "));
    }

    #[test]
    fn matches_content_type_case_insensitively() {
        let entry = FileTypeEntry {
            content_type: "image/PNG".to_string(),
            description: "PNG image".to_string(),
            icon: None,
            default_app: None,
        };
        assert!(matches_query(&entry, None, "image"));
        assert!(matches_query(&entry, None, "IMAGE"));
        assert!(matches_query(&entry, None, "png"));
    }

    #[test]
    fn matches_description_case_insensitively() {
        let entry = FileTypeEntry {
            content_type: "image/png".to_string(),
            description: "Portable Network Graphics".to_string(),
            icon: None,
            default_app: None,
        };
        assert!(matches_query(&entry, None, "portable"));
        assert!(matches_query(&entry, None, "NETWORK"));
    }

    #[test]
    fn matches_app_name_case_insensitively() {
        let entry = FileTypeEntry {
            content_type: "image/png".to_string(),
            description: "PNG image".to_string(),
            icon: None,
            default_app: None,
        };
        assert!(matches_query(&entry, Some("GIMP Image Editor"), "gimp"));
        assert!(matches_query(&entry, Some("GIMP Image Editor"), "EDITOR"));
    }

    #[test]
    fn rejects_non_matching_query() {
        let entry = FileTypeEntry {
            content_type: "image/png".to_string(),
            description: "PNG image".to_string(),
            icon: None,
            default_app: None,
        };
        assert!(!matches_query(&entry, Some("GIMP"), "video"));
    }
}
