//! # Reconnect ICS Generation
//!
//!   Builds iCalendar (.ics) attachments for pending reconnect reminders so they can be
//!   imported into a calendar client with a one-day advance alarm.
//!
//! INPUT:
//!   - `DueReconnectInfo` (contact name, due date, tag, vCard UID).
//!
//! OUTPUT:
//!   - `IcsFile` with filename and RFC 5545 calendar content (`VALARM` included).
//!
//! NOTES:
//!   - Events are all-day on the due date; batch builder skips individual failures with a warning.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use crate::models::{DueReconnectInfo, IcsFile};
use anyhow::Result;
use icalendar::{Alarm, Calendar, Component, Event, EventLike};
use chrono::{Duration, NaiveTime};

pub fn build_ics(reconnect: &DueReconnectInfo) -> Result<IcsFile> {
    let summary = format!("Follow up: {}", reconnect.full_name);
    let description = format!(
        "Time to reconnect with {}.\nOriginal reminder: {}",
        reconnect.full_name, reconnect.tag
    );

    let start_datetime = reconnect
        .due_date
        .and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap());

    let alarm = Alarm::display(&summary, Duration::days(-1));

    let event = Event::new()
        .summary(&summary)
        .description(&description)
        .starts(start_datetime)
        .ends(start_datetime + Duration::days(1))
        .alarm(alarm)
        .done();

    let mut calendar = Calendar::new();
    calendar.push(event);

    let filename = format!("reconnect-{}.ics", reconnect.uid);
    let content = calendar.to_string();

    Ok(IcsFile { filename, content })
}

/// Build ICS files for multiple reconnects.
pub fn build_ics_batch(reconnects: &[DueReconnectInfo]) -> Result<Vec<IcsFile>> {
    let mut ics_files = Vec::new();

    for reconnect in reconnects {
        match build_ics(reconnect) {
            Ok(ics) => ics_files.push(ics),
            Err(e) => {
                tracing::warn!(
                    "Failed to build ICS for reconnect {}: {}",
                    reconnect.uid,
                    e
                );
            }
        }
    }

    Ok(ics_files)
}

/// Build an ICS attachment from a due reconnect (for weekly digest).
pub fn build_ics_for_due(reconnect: &DueReconnectInfo, _email: Option<&str>) -> Result<IcsFile> {
    build_ics(reconnect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn test_build_ics() {
        let reconnect = DueReconnectInfo {
            uid: "test-uid-123".to_string(),
            full_name: "Alice Smith".to_string(),
            due_date: NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
            tag: "3 months".to_string(),
        };

        let ics = build_ics(&reconnect).unwrap();

        assert!(ics.filename.contains("test-uid-123"));
        assert!(ics.content.contains("Follow up: Alice Smith"));
        assert!(ics.content.contains("VALARM"));
    }
}
