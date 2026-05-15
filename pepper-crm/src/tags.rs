use chrono::NaiveDate;
use regex::Regex;

const CRM_LOG_SEPARATOR: &str = "--- CRM Log ---";

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
    let re = Regex::new(r"^(\d+)\s+(day|days|week|weeks|month|months)$").unwrap();
    
    if let Some(cap) = re.captures(&tag) {
        let num: i64 = cap[1].parse().ok()?;
        let unit = &cap[2];
        
        return match unit {
            "day" | "days" => Some(from + chrono::Duration::days(num)),
            "week" | "weeks" => Some(from + chrono::Duration::weeks(num)),
            "month" | "months" => {
                // Use month arithmetic from chrono
                from.checked_add_months(chrono::Months::new(num as u32))
            }
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
    fn test_append_log_entry() {
        let note = "Met at conference.\nTODO: send intro";
        let updated = append_log_entry(note, "Sent follow-up email", Some("6 months"));
        
        assert!(updated.contains(CRM_LOG_SEPARATOR));
        assert!(updated.contains("Sent follow-up email"));
        assert!(updated.contains("Reconnect: 6 months"));
    }
}
