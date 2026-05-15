use crate::models::{IcsFile, ReconnectRow};
use anyhow::Result;
use icalendar::{Alarm, Calendar, Component, Event, EventLike};
use chrono::{Duration, NaiveTime};

/// Build an ICS file for a single reconnect reminder
pub fn build_ics(reconnect: &ReconnectRow) -> Result<IcsFile> {
    let summary = format!("Follow up: {}", reconnect.full_name);
    let description = format!(
        "Time to reconnect with {}.\nOriginal reminder: {}",
        reconnect.full_name, reconnect.original_tag
    );
    
    // Create an all-day event on the due date
    let start_datetime = reconnect.due_date
        .and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    
    // Add a reminder alarm 1 day before
    let alarm = Alarm::display(&summary, Duration::days(-1));
    
    let event = Event::new()
        .summary(&summary)
        .description(&description)
        .starts(start_datetime)
        .ends(start_datetime + Duration::days(1))
        .alarm(alarm)
        .done();
    
    // Build the calendar
    let mut calendar = Calendar::new();
    calendar.push(event);
    
    let filename = format!("reconnect-{}.ics", reconnect.vcard_uid);
    let content = calendar.to_string();
    
    Ok(IcsFile { filename, content })
}

/// Build ICS files for multiple reconnects
pub fn build_ics_batch(reconnects: &[ReconnectRow]) -> Result<Vec<IcsFile>> {
    let mut ics_files = Vec::new();
    
    for reconnect in reconnects {
        match build_ics(reconnect) {
            Ok(ics) => ics_files.push(ics),
            Err(e) => {
                tracing::warn!(
                    "Failed to build ICS for reconnect {}: {}",
                    reconnect.reconnect_id,
                    e
                );
            }
        }
    }
    
    Ok(ics_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use crate::models::ReconnectStatus;
    use chrono::NaiveDate;

    #[test]
    fn test_build_ics() {
        let reconnect = ReconnectRow {
            reconnect_id: Uuid::new_v4(),
            contact_id: Uuid::new_v4(),
            vcard_uid: "test-uid-123".to_string(),
            full_name: "Alice Smith".to_string(),
            email: Some("alice@example.com".to_string()),
            due_date: NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
            original_tag: "3 months".to_string(),
            status: ReconnectStatus::Pending,
        };
        
        let ics = build_ics(&reconnect).unwrap();
        
        println!("Generated ICS content:\n{}", ics.content);
        
        assert!(ics.filename.contains("test-uid-123"));
        assert!(ics.content.contains("Follow up: Alice Smith"), "ICS content: {}", ics.content);
        assert!(ics.content.contains("VALARM"));
    }
}
