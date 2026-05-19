//! # NOTE and CATEGORIES Tag Parsing
//!
//!   Parses CRM tags from vCard NOTE and CATEGORIES fields: TODOs, Reconnect intervals,
//!   month/year interaction stamps, venue filters, and travel eligibility rules.
//!
//! INPUT:
//!   - NOTE text, category strings, anchor dates (`REV` or `Month YYYY:` notes), `as_of` date.
//!
//! OUTPUT:
//!   - Due dates, reconnect lists, log append strings, and travel-match eligibility booleans.
//!
//! NOTES:
//!   - City triggers (`before … trip`) defer interval math; `Never` and venue cards are excluded.
//!   - `TRAVEL_INTERACTION_WINDOW_MONTHS` (18) gates stale contacts from travel lists.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use chrono::NaiveDate;
use regex::Regex;

/// Separator between user-facing notes and machine CRM log lines.
pub const CRM_LOG_SEPARATOR: &str = "--- CRM Log ---";

/// Prefix for reconnect values in vCard `CATEGORIES`.
pub const RECONNECT_CATEGORY_PREFIX: &str = "Reconnect:";

/// Parse all TODO: tags from the note field (above the CRM Log separator)
pub fn parse_todos(note: &str) -> Vec<String> {
    let content = extract_content_above_log(note);
    let re = Regex::new(r"(?m)^TODO:\s*(.+)$").unwrap();
    
    re.captures_iter(&content)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse the last Reconnect: tag from the note field
/// Returns the full tag text (e.g., "3 months" or "before NY trip")
pub fn parse_reconnect_tag(note: &str) -> Option<String> {
    let re = Regex::new(r"(?m)^Reconnect:\s*(.+)$").unwrap();
    
    // Find all matches and return the last one
    re.captures_iter(note)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().trim().to_string()))
        .filter(|s| !s.is_empty())
        .last()
}

/// Convert a Reconnect tag to a due date
/// Returns None if the tag is a city trigger or invalid format
pub fn tag_to_due_date(tag: &str, from: NaiveDate) -> Option<NaiveDate> {
    if is_city_trigger(tag) {
        return None;
    }
    
    let tag = tag.trim().to_lowercase();
    
    // Try to parse "N days", "N weeks", "N months"
    let re = Regex::new(r"^(\d+)\s+(day|days|week|weeks|month|months|year|years)$").unwrap();
    
    if let Some(cap) = re.captures(&tag) {
        let num: i64 = cap[1].parse().ok()?;
        let unit = &cap[2];
        
        return match unit {
            "day" | "days" => Some(from + chrono::Duration::days(num)),
            "week" | "weeks" => Some(from + chrono::Duration::weeks(num)),
            "month" | "months" => from.checked_add_months(chrono::Months::new(num as u32)),
            "year" | "years" => from.checked_add_months(chrono::Months::new(num as u32 * 12)),
            _ => None,
        };
    }
    
    None
}

/// Check if a tag is a city trigger (e.g., "before NY trip")
pub fn is_city_trigger(tag: &str) -> bool {
    let tag = tag.trim().to_lowercase();
    tag.contains("before") && tag.contains("trip")
}

/// Parse vCard category lines into tokens (comma-separated).
pub fn parse_categories_value(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// Last `Reconnect: …` category body (e.g. `3 months`, `before Chicago trip`, `Never`).
pub fn parse_reconnect_category(categories: &[String]) -> Option<String> {
    categories
        .iter()
        .filter_map(|cat| {
            let cat = cat.trim();
            cat.strip_prefix(RECONNECT_CATEGORY_PREFIX)
                .map(|body| body.trim().to_string())
        })
        .filter(|s| !s.is_empty())
        .last()
}

/// Resolve reconnect from categories first, then legacy `NOTE` lines.
pub fn resolve_reconnect_tag(categories: &[String], note: &str) -> Option<String> {
    parse_reconnect_category(categories).or_else(|| parse_reconnect_tag(note))
}

/// True when reconnect body is `Never` or categories contain `Reconnect: Never`.
pub fn is_reconnect_never(categories: &[String], reconnect_tag: Option<&str>) -> bool {
    if reconnect_tag
        .map(|t| t.trim().eq_ignore_ascii_case("never"))
        .unwrap_or(false)
    {
        return true;
    }
    if parse_reconnect_category(categories)
        .map(|t| t.trim().eq_ignore_ascii_case("never"))
        .unwrap_or(false)
    {
        return true;
    }
    categories.iter().any(|c| {
        let c = c.trim();
        c.eq_ignore_ascii_case(&format!("{RECONNECT_CATEGORY_PREFIX} Never"))
            || c.eq_ignore_ascii_case(&format!("{RECONNECT_CATEGORY_PREFIX}Never"))
    })
}

/// Month/year interaction lines in notes, e.g. `July 2026:` or `Mar 2025:` (CRM style).
pub fn month_year_dates_in_note(note: &str) -> Vec<NaiveDate> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        // Long month names before abbreviations (e.g. September before Sep).
        Regex::new(
            r"(?i)(January|February|March|April|May|June|July|August|September|October|November|December|Jan|Feb|Mar|Apr|Jun|Jul|Aug|Sept|Sep|Oct|Nov|Dec)\s+(\d{4})\s*:",
        )
        .expect("month-year note regex")
    });

    re.captures_iter(note)
        .filter_map(|cap| {
            let month = cap.get(1)?.as_str();
            let year: i32 = cap.get(2)?.as_str().parse().ok()?;
            let month_num = month_name_to_number(month)?;
            NaiveDate::from_ymd_opt(year, month_num, 1)
        })
        .collect()
}

/// How far back travel lists look for a `Month YYYY:` interaction in notes.
pub const TRAVEL_INTERACTION_WINDOW_MONTHS: u32 = 18;

/// Most recent `Month YYYY:` in the note that is on or before `as_of`.
pub fn most_recent_past_month_year_in_note(note: &str, as_of: NaiveDate) -> Option<NaiveDate> {
    month_year_dates_in_note(note)
        .into_iter()
        .filter(|d| *d <= as_of)
        .max()
}

/// True when the latest past `Month YYYY:` entry is within `window_months` of `as_of`.
pub fn has_recent_month_year_interaction_note(
    note: &str,
    as_of: NaiveDate,
    window_months: u32,
) -> bool {
    let Some(latest) = most_recent_past_month_year_in_note(note, as_of) else {
        return false;
    };
    let Some(cutoff) = as_of.checked_sub_months(chrono::Months::new(window_months)) else {
        return false;
    };
    latest >= cutoff
}

/// True for venue/business category labels (e.g. `Venue/Business`).
pub fn is_venue_category(categories: &[String]) -> bool {
    categories.iter().any(|c| is_venue_label(c))
}

/// Venue or business place — not a person to reconnect with while traveling.
pub fn is_venue_label(label: &str) -> bool {
    let lower = label.trim().to_lowercase();
    lower.contains("venue/business")
        || lower.starts_with("venue:")
        || lower.starts_with("venue/business:")
        || lower == "venue"
        || lower == "venue/business"
}

/// Venue contacts — categories, or `Venue: …` / `Venue/Business: …` names (Google export).
pub fn is_venue_contact(contact: &crate::models::Contact) -> bool {
    if is_venue_category(&contact.categories) {
        return true;
    }
    is_venue_label(&contact.full_name)
}

/// `Month YYYY` prefix for a note stamp (e.g. `May 2026`).
pub fn format_month_year_note_prefix(date: NaiveDate) -> String {
    date.format("%B %Y").to_string()
}

/// Last interaction date for reconnect math: vCard `REV`, else latest past `Month YYYY:` note.
pub fn reconnect_anchor_date(
    rev: Option<NaiveDate>,
    note: &str,
    as_of: NaiveDate,
) -> Option<NaiveDate> {
    if let Some(d) = rev.filter(|&d| d <= as_of) {
        return Some(d);
    }
    most_recent_past_month_year_in_note(note, as_of)
}

/// When the current `Reconnect: …` interval becomes due (from `REV` or note anchor).
pub fn reconnect_due_date(
    categories: &[String],
    note: &str,
    reconnect_tag: Option<&str>,
    rev: Option<NaiveDate>,
    as_of: NaiveDate,
) -> Option<NaiveDate> {
    let tag = reconnect_tag
        .map(ToString::to_string)
        .or_else(|| resolve_reconnect_tag(categories, note))?;
    if is_city_trigger(&tag) {
        return None;
    }
    let last = reconnect_anchor_date(rev, note, as_of)?;
    tag_to_due_date(&tag, last)
}

/// Include in travel lists only when no interval tag, or the reconnect date has arrived.
pub fn is_reconnect_due_for_travel(
    categories: &[String],
    note: &str,
    reconnect_tag: Option<&str>,
    rev: Option<NaiveDate>,
    as_of: NaiveDate,
) -> bool {
    let Some(tag) = reconnect_tag
        .map(ToString::to_string)
        .or_else(|| resolve_reconnect_tag(categories, note))
    else {
        return true;
    };
    if is_city_trigger(&tag) {
        return true;
    }
    match reconnect_due_date(categories, note, Some(&tag), rev, as_of) {
        None => true,
        Some(due) => as_of >= due,
    }
}

/// Preset bodies for `CATEGORIES: Reconnect: …` (travel snooze dropdown).
pub const RECONNECT_SNOOZE_OPTIONS: &[&str] = &[
    "1 week",
    "1 month",
    "2 months",
    "6 months",
    "Never",
];

/// Contacts with a timed `Reconnect:` interval due on or before `as_of + window_days`.
/// Uses vCard `REV` (or latest past `Month YYYY:` note) as the anchor. Excludes Never,
/// city triggers, and venue/business cards.
pub fn due_reconnects_from_contacts(
    contacts: &[crate::models::Contact],
    as_of: NaiveDate,
    window_days: u32,
) -> Vec<crate::models::DueReconnectInfo> {
    let window_end = as_of + chrono::Duration::days(window_days as i64);
    let mut out = Vec::new();

    for contact in contacts {
        if is_reconnect_never(&contact.categories, contact.reconnect_tag.as_deref()) {
            continue;
        }
        if is_venue_contact(contact) {
            continue;
        }
        let Some(anchor) = reconnect_anchor_date(contact.rev, &contact.note_raw, as_of) else {
            continue;
        };
        let Some(cutoff) =
            as_of.checked_sub_months(chrono::Months::new(TRAVEL_INTERACTION_WINDOW_MONTHS))
        else {
            continue;
        };
        if anchor < cutoff {
            continue;
        }
        let Some(tag) = resolve_reconnect_tag(&contact.categories, &contact.note_raw) else {
            continue;
        };
        if is_city_trigger(&tag) {
            continue;
        }
        let Some(due) = reconnect_due_date(
            &contact.categories,
            &contact.note_raw,
            Some(&tag),
            contact.rev,
            as_of,
        ) else {
            continue;
        };
        if due > window_end {
            continue;
        }
        out.push(crate::models::DueReconnectInfo {
            uid: contact.uid.clone(),
            full_name: contact.full_name.clone(),
            due_date: due,
            tag,
        });
    }

    out.sort_by_key(|r| r.due_date);
    out
}

/// Whether a contact should appear in weekly travel match lists.
pub fn is_travel_match_eligible(contact: &crate::models::Contact, as_of: NaiveDate) -> bool {
    if is_reconnect_never(&contact.categories, contact.reconnect_tag.as_deref()) {
        return false;
    }
    if is_venue_contact(contact) {
        return false;
    }
    let Some(anchor) = reconnect_anchor_date(contact.rev, &contact.note_raw, as_of) else {
        return false;
    };
    let Some(cutoff) = as_of.checked_sub_months(chrono::Months::new(TRAVEL_INTERACTION_WINDOW_MONTHS))
    else {
        return false;
    };
    if anchor < cutoff {
        return false;
    }
    is_reconnect_due_for_travel(
        &contact.categories,
        &contact.note_raw,
        contact.reconnect_tag.as_deref(),
        contact.rev,
        as_of,
    )
}

fn month_name_to_number(name: &str) -> Option<u32> {
    match name.to_lowercase().as_str() {
        "january" | "jan" => Some(1),
        "february" | "feb" => Some(2),
        "march" | "mar" => Some(3),
        "april" | "apr" => Some(4),
        "may" => Some(5),
        "june" | "jun" => Some(6),
        "july" | "jul" => Some(7),
        "august" | "aug" => Some(8),
        "september" | "sept" | "sep" => Some(9),
        "october" | "oct" => Some(10),
        "november" | "nov" => Some(11),
        "december" | "dec" => Some(12),
        _ => None,
    }
}

/// City name from a `before [city] trip` tag body.
pub fn extract_trip_city(tag: &str) -> Option<String> {
    if !is_city_trigger(tag) {
        return None;
    }
    let lower = tag.trim().to_lowercase();
    let rest = lower.strip_prefix("before")?.trim();
    let city = rest.strip_suffix("trip")?.trim();
    if city.is_empty() {
        None
    } else {
        Some(city.to_string())
    }
}

/// Loose match between calendar trip title and a city hint from a trip tag.
pub fn city_fuzzy_matches_trip(trip_title: &str, city_hint: &str) -> bool {
    let title = normalize_place_name(trip_title);
    let hint = normalize_place_name(city_hint);
    if hint.is_empty() || title.is_empty() {
        return false;
    }
    title.contains(&hint) || hint.contains(&title)
}

fn normalize_place_name(s: &str) -> String {
    s.to_lowercase()
        .replace(',', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Append a log entry to the note field
/// Creates the CRM Log separator if it doesn't exist
/// Optionally appends a new Reconnect: tag after the log entry
pub fn append_log_entry(note: &str, entry: &str, new_tag: Option<&str>) -> String {
    let today = chrono::Local::now().format("%Y-%m-%d");
    let log_line = format!("{}: {}", today, entry);
    
    if note.contains(CRM_LOG_SEPARATOR) {
        // Log section exists, append to it
        let mut result = note.to_string();
        result.push('\n');
        result.push_str(&log_line);
        if let Some(tag) = new_tag {
            result.push('\n');
            result.push_str(&format!("Reconnect: {}", tag));
        }
        result
    } else {
        // Create log section
        let mut result = note.to_string();
        if !result.is_empty() && !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(CRM_LOG_SEPARATOR);
        result.push('\n');
        result.push_str(&log_line);
        if let Some(tag) = new_tag {
            result.push('\n');
            result.push_str(&format!("Reconnect: {}", tag));
        }
        result
    }
}

/// Extract content above the CRM Log separator
fn extract_content_above_log(note: &str) -> String {
    if let Some(pos) = note.find(CRM_LOG_SEPARATOR) {
        note[..pos].to_string()
    } else {
        note.to_string()
    }
}

/// Extract log entries from the CRM Log section
pub fn extract_log_entries(note: &str) -> Vec<String> {
    if let Some(pos) = note.find(CRM_LOG_SEPARATOR) {
        let log_section = &note[pos + CRM_LOG_SEPARATOR.len()..];
        log_section
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.starts_with("Reconnect:"))
            .map(|line| line.to_string())
            .collect()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_todos() {
        let note = "Met at conference.\nTODO: send intro email\nTODO: share grant template\nReconnect: 3 months";
        let todos = parse_todos(note);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0], "send intro email");
        assert_eq!(todos[1], "share grant template");
    }

    #[test]
    fn test_parse_reconnect_tag() {
        let note = "Some notes.\nReconnect: 2 weeks\nMore notes.\nReconnect: 3 months";
        let tag = parse_reconnect_tag(note);
        assert_eq!(tag, Some("3 months".to_string())); // Last one wins
    }

    #[test]
    fn test_tag_to_due_date() {
        let from = NaiveDate::from_ymd_opt(2026, 5, 14).unwrap();
        
        let due = tag_to_due_date("2 weeks", from);
        assert_eq!(due, Some(NaiveDate::from_ymd_opt(2026, 5, 28).unwrap()));
        
        let due = tag_to_due_date("3 months", from);
        assert_eq!(due, Some(NaiveDate::from_ymd_opt(2026, 8, 14).unwrap()));
        
        let due = tag_to_due_date("5 days", from);
        assert_eq!(due, Some(NaiveDate::from_ymd_opt(2026, 5, 19).unwrap()));
    }

    #[test]
    fn test_is_city_trigger() {
        assert!(is_city_trigger("before NY trip"));
        assert!(is_city_trigger("before Berlin trip"));
        assert!(!is_city_trigger("3 months"));
        assert!(!is_city_trigger("2 weeks"));
    }

    #[test]
    fn test_parse_reconnect_category() {
        let cats = vec![
            "Work".to_string(),
            "Reconnect: 2 weeks".to_string(),
            "Reconnect: before Chicago trip".to_string(),
        ];
        assert_eq!(
            parse_reconnect_category(&cats),
            Some("before Chicago trip".to_string())
        );
    }

    #[test]
    fn test_is_reconnect_never() {
        let never_cats = vec!["Reconnect: Never".to_string()];
        assert!(is_reconnect_never(&never_cats, Some("Never")));
        assert!(!is_reconnect_never(&[], Some("3 months")));
    }

    #[test]
    fn test_extract_trip_city() {
        assert_eq!(
            extract_trip_city("before Chicago trip"),
            Some("chicago".to_string())
        );
    }

    #[test]
    fn test_city_fuzzy_matches_trip() {
        assert!(city_fuzzy_matches_trip("Chicago, IL", "chicago"));
        assert!(!city_fuzzy_matches_trip("Chicago, IL", "denver"));
    }

    #[test]
    fn test_month_year_dates_in_note() {
        let note = "April 2024: Met at archer.\nJuly 2026: Planning trip.";
        let dates = month_year_dates_in_note(note);
        assert_eq!(dates.len(), 2);
        assert_eq!(dates[0], NaiveDate::from_ymd_opt(2024, 4, 1).unwrap());

        let abbrev = "Mar 2025: Quick drink.\nSep 2024: Fall event.";
        let dates = month_year_dates_in_note(abbrev);
        assert_eq!(dates.len(), 2);
        assert_eq!(dates[0], NaiveDate::from_ymd_opt(2025, 3, 1).unwrap());
        assert_eq!(dates[1], NaiveDate::from_ymd_opt(2024, 9, 1).unwrap());
    }

    #[test]
    fn test_has_recent_month_year_interaction_note() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 19).unwrap();
        assert!(!has_recent_month_year_interaction_note(
            "July 2026: Future only.",
            as_of,
            18
        ));
        assert!(!has_recent_month_year_interaction_note(
            "March 2024: Met at party.",
            as_of,
            18
        ));
        assert!(has_recent_month_year_interaction_note(
            "March 2025: Met at party.",
            as_of,
            18
        ));
    }

    #[test]
    fn test_is_reconnect_due_for_travel() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 19).unwrap();
        let rev = NaiveDate::from_ymd_opt(2025, 3, 1).unwrap();
        let note = "March 2025: Met at party.";
        let cats = vec!["Reconnect: 6 months".to_string()];
        assert!(is_reconnect_due_for_travel(
            &cats, note, Some("6 months"), Some(rev), as_of
        ));
        let cats = vec!["Reconnect: 2 months".to_string()];
        assert!(!is_reconnect_due_for_travel(
            &cats,
            note,
            Some("2 months"),
            Some(NaiveDate::from_ymd_opt(2026, 4, 1).unwrap()),
            as_of
        ));
        let cats = vec!["Reconnect: 1 week".to_string()];
        assert!(!is_reconnect_due_for_travel(
            &cats,
            note,
            Some("1 week"),
            Some(NaiveDate::from_ymd_opt(2026, 5, 15).unwrap()),
            as_of
        ));
    }

    fn sample_contact(uid: &str, name: &str) -> crate::models::Contact {
        crate::models::Contact {
            uid: uid.into(),
            full_name: name.into(),
            email: None,
            phone: None,
            org: None,
            city: None,
            state: None,
            country: None,
            geo: None,
            geo_source: None,
            categories: vec![],
            note_raw: String::new(),
            todos: vec![],
            reconnect_tag: None,
            rev: None,
            log_entries: vec![],
            vcf_path: "x.vcf".into(),
        }
    }

    #[test]
    fn test_due_reconnects_from_contacts() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 19).unwrap();
        let mut due_soon = sample_contact("u1", "Alex");
        due_soon.categories = vec!["Reconnect: 1 week".into()];
        due_soon.reconnect_tag = Some("1 week".into());
        due_soon.rev = Some(NaiveDate::from_ymd_opt(2026, 5, 15).unwrap());

        let mut due_later = sample_contact("u2", "Blair");
        due_later.categories = vec!["Reconnect: 6 months".into()];
        due_later.reconnect_tag = Some("6 months".into());
        due_later.rev = Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());

        let list = due_reconnects_from_contacts(&[due_soon, due_later], as_of, 7);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].full_name, "Alex");
        assert_eq!(
            list[0].due_date,
            NaiveDate::from_ymd_opt(2026, 5, 22).unwrap()
        );
    }

    #[test]
    fn test_reconnect_anchor_prefers_rev() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 19).unwrap();
        let rev = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let note = "March 2020: Old note.";
        let anchor = reconnect_anchor_date(Some(rev), note, as_of).unwrap();
        assert_eq!(anchor, rev);
    }

    #[test]
    fn test_has_recent_month_year_empty_note() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 19).unwrap();
        assert!(!has_recent_month_year_interaction_note("", as_of, 18));
        assert!(!has_recent_month_year_interaction_note(
            "Met at conference, no date.",
            as_of,
            18
        ));
    }

    #[test]
    fn test_is_venue_contact() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 19).unwrap();
        let mut c = crate::models::Contact {
            uid: "v1".into(),
            full_name: "Venue: Metro".into(),
            email: None,
            phone: None,
            org: None,
            city: Some("Boston".into()),
            state: Some("MA".into()),
            country: None,
            geo: None,
            geo_source: None,
            categories: vec![],
            note_raw: "May 2025: Regular spot.".into(),
            todos: vec![],
            reconnect_tag: None,
            rev: None,
            log_entries: vec![],
            vcf_path: "x.vcf".into(),
        };
        assert!(is_venue_contact(&c));
        assert!(!is_travel_match_eligible(&c, as_of));

        c.full_name = "Metro Bar".into();
        c.categories = vec!["Venue/Business".to_string()];
        assert!(is_venue_contact(&c));

        c.full_name = "Venue/Business: Archer Hotel".into();
        c.categories = vec![];
        assert!(is_venue_contact(&c));
    }

    #[test]
    fn test_append_log_entry() {
        let note = "Met at conference.\nTODO: send intro";
        let updated = append_log_entry(note, "Sent follow-up email", Some("6 months"));
        
        assert!(updated.contains(CRM_LOG_SEPARATOR));
        assert!(updated.contains("Sent follow-up email"));
        assert!(updated.contains("Reconnect: 6 months"));
    }
}
