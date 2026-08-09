//! Faz 9 advanced search query syntax: `name:`, `type:`, `size:`, and
//! `modified:` filters freely combined with plain free-text terms, e.g.
//! `type:image size:>10MB modified:last-week`.

use chrono::{DateTime, Duration, Utc};

/// `type:` filter values — the exact set Faz 9 specifies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileTypeFilter {
    Image,
    Video,
    Document,
    Archive,
    Executable,
}

impl FileTypeFilter {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "image" => Some(Self::Image),
            "video" => Some(Self::Video),
            "document" => Some(Self::Document),
            "archive" => Some(Self::Archive),
            "executable" => Some(Self::Executable),
            _ => None,
        }
    }

    /// The `files.kind` column value this filter matches — see
    /// `classify::classify`, which produces exactly these strings.
    pub fn as_kind_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::Document => "document",
            Self::Archive => "archive",
            Self::Executable => "executable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeOp {
    GreaterThan,
    LessThan,
    Equal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeFilter {
    pub op: SizeOp,
    pub bytes: u64,
}

/// `modified:` filter values. Deliberately a named preset rather than a
/// resolved date range at parse time — `resolve` takes `now` as an explicit
/// parameter so both parsing and range math stay independently unit
/// testable (parsing never touches a clock; range math never touches a
/// string).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifiedPreset {
    Today,
    Yesterday,
    LastWeek,
    LastMonth,
}

impl ModifiedPreset {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "today" => Some(Self::Today),
            "yesterday" => Some(Self::Yesterday),
            "last-week" => Some(Self::LastWeek),
            "last-month" => Some(Self::LastMonth),
            _ => None,
        }
    }

    /// Resolves this preset to a half-open `[start, end)` UTC range
    /// relative to `now`.
    pub fn resolve(self, now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
        let today_start = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("00:00:00 is always a valid time")
            .and_utc();
        match self {
            Self::Today => (today_start, today_start + Duration::days(1)),
            Self::Yesterday => (today_start - Duration::days(1), today_start),
            Self::LastWeek => (
                today_start - Duration::days(7),
                today_start + Duration::days(1),
            ),
            Self::LastMonth => (
                today_start - Duration::days(30),
                today_start + Duration::days(1),
            ),
        }
    }
}

/// A parsed Faz 9 search query: structured filters plus whatever free-text
/// terms are left over (searched against the FTS5 filename/path index).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedQuery {
    pub name: Option<String>,
    pub free_text: Vec<String>,
    pub file_type: Option<FileTypeFilter>,
    pub size: Option<SizeFilter>,
    pub modified: Option<ModifiedPreset>,
}

impl ParsedQuery {
    /// True when the query has no filters and no free text at all.
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.free_text.is_empty()
            && self.file_type.is_none()
            && self.size.is_none()
            && self.modified.is_none()
    }

    /// True when the query uses any of the Faz 9 `key:value` filter
    /// prefixes — the signal `veyra-ui` uses to decide whether to run the
    /// indexed search (vs. the plain in-directory name filter Faz 3
    /// already provides for simple text).
    pub fn has_filters(&self) -> bool {
        self.name.is_some()
            || self.file_type.is_some()
            || self.size.is_some()
            || self.modified.is_some()
    }
}

/// Parses a Faz 9 search string into structured filters + free-text terms.
/// Unrecognized `key:value` tokens (unknown key, or a known key with a
/// value that fails to parse) are treated as plain free-text rather than
/// rejected — a typo in a filter should never fail the whole search, it
/// should just search for that literal text instead.
pub fn parse(input: &str) -> ParsedQuery {
    let mut query = ParsedQuery::default();
    for token in input.split_whitespace() {
        if let Some(value) = token.strip_prefix("name:") {
            if !value.is_empty() {
                query.name = Some(value.to_string());
                continue;
            }
        } else if let Some(value) = token.strip_prefix("type:") {
            if let Some(parsed) = FileTypeFilter::parse(value) {
                query.file_type = Some(parsed);
                continue;
            }
        } else if let Some(value) = token.strip_prefix("size:") {
            if let Some(parsed) = parse_size(value) {
                query.size = Some(parsed);
                continue;
            }
        } else if let Some(value) = token.strip_prefix("modified:") {
            if let Some(parsed) = ModifiedPreset::parse(value) {
                query.modified = Some(parsed);
                continue;
            }
        }
        query.free_text.push(token.to_string());
    }
    query
}

/// Parses a `size:` filter value: an optional `>`/`<` comparison prefix
/// (defaulting to exact `=` when absent), a decimal number, and a unit
/// suffix (`B`/`KB`/`MB`/`GB`/`TB`, case-insensitive, 1024-based —
/// matching `veyra_filesystem::format_size`). A bare number with no suffix
/// is bytes.
fn parse_size(value: &str) -> Option<SizeFilter> {
    let (op, rest) = match value.as_bytes().first() {
        Some(b'>') => (SizeOp::GreaterThan, &value[1..]),
        Some(b'<') => (SizeOp::LessThan, &value[1..]),
        _ => (SizeOp::Equal, value),
    };
    if rest.is_empty() {
        return None;
    }

    let (number_str, unit) = match rest.find(|c: char| !c.is_ascii_digit() && c != '.') {
        Some(split_at) => rest.split_at(split_at),
        None => (rest, ""),
    };
    let number: f64 = number_str.parse().ok()?;
    if number < 0.0 {
        return None;
    }

    let multiplier: u64 = match unit.to_ascii_uppercase().as_str() {
        "" | "B" => 1,
        "KB" => 1024,
        "MB" => 1024 * 1024,
        "GB" => 1024 * 1024 * 1024,
        "TB" => 1024 * 1024 * 1024 * 1024,
        _ => return None,
    };

    Some(SizeFilter {
        op,
        bytes: (number * multiplier as f64) as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn parses_plain_free_text() {
        let parsed = parse("vacation photo");
        assert_eq!(parsed.free_text, vec!["vacation", "photo"]);
        assert!(!parsed.has_filters());
    }

    #[test]
    fn parses_name_filter() {
        let parsed = parse("name:report");
        assert_eq!(parsed.name.as_deref(), Some("report"));
        assert!(parsed.free_text.is_empty());
    }

    #[test]
    fn parses_type_filter() {
        assert_eq!(parse("type:image").file_type, Some(FileTypeFilter::Image));
        assert_eq!(parse("type:VIDEO").file_type, Some(FileTypeFilter::Video));
        assert_eq!(
            parse("type:executable").file_type,
            Some(FileTypeFilter::Executable)
        );
    }

    #[test]
    fn unknown_type_value_falls_back_to_free_text() {
        let parsed = parse("type:spreadsheet");
        assert_eq!(parsed.file_type, None);
        assert_eq!(parsed.free_text, vec!["type:spreadsheet"]);
    }

    #[test]
    fn parses_size_greater_than_megabytes() {
        let parsed = parse("size:>100MB");
        assert_eq!(
            parsed.size,
            Some(SizeFilter {
                op: SizeOp::GreaterThan,
                bytes: 100 * 1024 * 1024,
            })
        );
    }

    #[test]
    fn parses_size_less_than_kilobytes() {
        let parsed = parse("size:<10KB");
        assert_eq!(
            parsed.size,
            Some(SizeFilter {
                op: SizeOp::LessThan,
                bytes: 10 * 1024,
            })
        );
    }

    #[test]
    fn parses_size_without_comparison_as_equal() {
        let parsed = parse("size:1GB");
        assert_eq!(
            parsed.size,
            Some(SizeFilter {
                op: SizeOp::Equal,
                bytes: 1024 * 1024 * 1024,
            })
        );
    }

    #[test]
    fn parses_bare_byte_size_with_no_unit() {
        let parsed = parse("size:>512");
        assert_eq!(
            parsed.size,
            Some(SizeFilter {
                op: SizeOp::GreaterThan,
                bytes: 512,
            })
        );
    }

    #[test]
    fn parses_fractional_size() {
        let parsed = parse("size:>1.5MB");
        assert_eq!(parsed.size.unwrap().bytes, (1.5 * 1024.0 * 1024.0) as u64);
    }

    #[test]
    fn invalid_size_falls_back_to_free_text() {
        let parsed = parse("size:>abc");
        assert_eq!(parsed.size, None);
        assert_eq!(parsed.free_text, vec!["size:>abc"]);
    }

    #[test]
    fn parses_modified_presets() {
        assert_eq!(
            parse("modified:today").modified,
            Some(ModifiedPreset::Today)
        );
        assert_eq!(
            parse("modified:yesterday").modified,
            Some(ModifiedPreset::Yesterday)
        );
        assert_eq!(
            parse("modified:last-week").modified,
            Some(ModifiedPreset::LastWeek)
        );
        assert_eq!(
            parse("modified:last-month").modified,
            Some(ModifiedPreset::LastMonth)
        );
    }

    #[test]
    fn unknown_modified_value_falls_back_to_free_text() {
        let parsed = parse("modified:tomorrow");
        assert_eq!(parsed.modified, None);
        assert_eq!(parsed.free_text, vec!["modified:tomorrow"]);
    }

    #[test]
    fn parses_combined_query() {
        let parsed = parse("type:image size:>10MB modified:last-week vacation");
        assert_eq!(parsed.file_type, Some(FileTypeFilter::Image));
        assert_eq!(parsed.size.unwrap().op, SizeOp::GreaterThan);
        assert_eq!(parsed.modified, Some(ModifiedPreset::LastWeek));
        assert_eq!(parsed.free_text, vec!["vacation"]);
        assert!(parsed.has_filters());
    }

    #[test]
    fn empty_query_parses_to_empty() {
        assert!(parse("").is_empty());
        assert!(parse("   ").is_empty());
    }

    #[test]
    fn modified_today_resolves_to_midnight_to_midnight_range() {
        let now = Utc.with_ymd_and_hms(2026, 8, 9, 15, 30, 0).unwrap();
        let (start, end) = ModifiedPreset::Today.resolve(now);
        assert_eq!(start, Utc.with_ymd_and_hms(2026, 8, 9, 0, 0, 0).unwrap());
        assert_eq!(end, Utc.with_ymd_and_hms(2026, 8, 10, 0, 0, 0).unwrap());
    }

    #[test]
    fn modified_yesterday_resolves_to_previous_day() {
        let now = Utc.with_ymd_and_hms(2026, 8, 9, 15, 30, 0).unwrap();
        let (start, end) = ModifiedPreset::Yesterday.resolve(now);
        assert_eq!(start, Utc.with_ymd_and_hms(2026, 8, 8, 0, 0, 0).unwrap());
        assert_eq!(end, Utc.with_ymd_and_hms(2026, 8, 9, 0, 0, 0).unwrap());
    }

    #[test]
    fn modified_last_week_spans_seven_days_plus_today() {
        let now = Utc.with_ymd_and_hms(2026, 8, 9, 15, 30, 0).unwrap();
        let (start, end) = ModifiedPreset::LastWeek.resolve(now);
        assert_eq!(start, Utc.with_ymd_and_hms(2026, 8, 2, 0, 0, 0).unwrap());
        assert_eq!(end, Utc.with_ymd_and_hms(2026, 8, 10, 0, 0, 0).unwrap());
    }
}
