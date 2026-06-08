//! # Calendar ICS Parsing
//!
//!   Fetches and parses iCalendar feeds to find travel trips overlapping the week after `as_of`.
//!   Event SUMMARY is treated as the destination name.
//!
//! INPUT:
//!   - ICS URL or raw ICS text; `as_of` calendar date for next-week window math.
//!
//! OUTPUT:
//!   - `TravelTrip` list, `IcsEvent` structs, and `(week_start, week_end)` range helpers.
//!
//! NOTES:
//!   - Google all-day `DTEND` is exclusive; parser normalizes to inclusive end dates.
//!   - `fetch_ics` requires network access to the calendar secret URL.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use crate::models::TravelTrip;
use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use std::collections::HashMap;

/// Parsed calendar event (minimal fields for travel).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcsEvent {
    pub summary: String,
    pub start: NaiveDate,
    /// Inclusive end date for all-day events (Google DTEND is exclusive; we normalize).
    pub end: NaiveDate,
}

/// Monday–Sunday range for the ISO week containing `as_of`.
pub fn week_range_containing(as_of: NaiveDate) -> (NaiveDate, NaiveDate) {
    let weekday = as_of.weekday();
    let days_from_monday = weekday.num_days_from_monday();
    let monday = as_of - chrono::Duration::days(days_from_monday as i64);
    let sunday = monday + chrono::Duration::days(6);
    (monday, sunday)
}

/// Monday–Sunday range for the calendar week after the week containing `as_of`.
pub fn next_week_range(as_of: NaiveDate) -> (NaiveDate, NaiveDate) {
    let weekday = as_of.weekday();
    let days_from_monday = weekday.num_days_from_monday();
    let this_monday = as_of - chrono::Duration::days(days_from_monday as i64);
    let next_monday = this_monday + chrono::Duration::days(7);
    let next_sunday = next_monday + chrono::Duration::days(6);
    (next_monday, next_sunday)
}

/// ISO week id e.g. `2026-W21` for the Monday of that week.
pub fn iso_week_id(monday: NaiveDate) -> String {
    let iso = monday.iso_week();
    format!("{}-W{:02}", iso.year(), iso.week())
}

/// Fetch ICS content from a URL (Google Calendar secret link).
pub async fn fetch_ics(url: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .send()
        .await
        .context("fetch calendar ICS")?;
    let text = resp.error_for_status()?.text().await?;
    Ok(text)
}

/// Parse VEVENT blocks from ICS text.
pub fn parse_ics_events(ics: &str) -> Result<Vec<IcsEvent>> {
    let unfolded = unfold_ics_lines(ics);
    let mut events = Vec::new();
    let mut in_event = false;
    let mut props: HashMap<String, String> = HashMap::new();

    for line in unfolded.lines() {
        if line == "BEGIN:VEVENT" {
            in_event = true;
            props.clear();
            continue;
        }
        if line == "END:VEVENT" {
            if in_event {
                if let Some(event) = props_to_event(&props) {
                    events.push(event);
                }
            }
            in_event = false;
            props.clear();
            continue;
        }
        if !in_event {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.split(';').next().unwrap_or(key).to_string();
            props.insert(key, value.to_string());
        }
    }

    Ok(events)
}

/// Trips overlapping `next_week` where SUMMARY is the destination.
pub fn trips_for_next_week(ics: &str, as_of: NaiveDate) -> Result<Vec<TravelTrip>> {
    let (week_start, week_end) = next_week_range(as_of);
    let events = parse_ics_events(ics)?;
    let mut trips = Vec::new();

    for ev in events {
        if !ranges_overlap(ev.start, ev.end, week_start, week_end) {
            continue;
        }
        let title = ev.summary.trim();
        if title.is_empty() {
            continue;
        }
        trips.push(TravelTrip {
            title: title.to_string(),
            start: ev.start.max(week_start),
            end: ev.end.min(week_end),
        });
    }

    trips.sort_by(|a, b| a.start.cmp(&b.start).then(a.title.cmp(&b.title)));
    Ok(trips)
}

/// Trips whose date range includes `day` (calendar all-day events; SUMMARY = destination).
pub fn trips_on_date(ics: &str, day: NaiveDate) -> Result<Vec<TravelTrip>> {
    let events = parse_ics_events(ics)?;
    let mut trips = Vec::new();

    for ev in events {
        if !ranges_overlap(ev.start, ev.end, day, day) {
            continue;
        }
        let title = ev.summary.trim();
        if title.is_empty() {
            continue;
        }
        trips.push(TravelTrip {
            title: title.to_string(),
            start: ev.start,
            end: ev.end,
        });
    }

    trips.sort_by(|a, b| a.start.cmp(&b.start).then(a.title.cmp(&b.title)));
    Ok(trips)
}

fn ranges_overlap(a_start: NaiveDate, a_end: NaiveDate, b_start: NaiveDate, b_end: NaiveDate) -> bool {
    a_start <= b_end && a_end >= b_start
}

fn props_to_event(props: &HashMap<String, String>) -> Option<IcsEvent> {
    let summary = props.get("SUMMARY")?.clone();
    let start = parse_ics_date(props.get("DTSTART")?).ok()?;
    let end = props
        .get("DTEND")
        .and_then(|s| parse_ics_date(s).ok())
        .unwrap_or(start);
    // Google all-day DTEND is exclusive
    let end_inclusive = if end > start {
        end - chrono::Duration::days(1)
    } else {
        end
    };
    Some(IcsEvent {
        summary,
        start,
        end: end_inclusive,
    })
}

fn parse_ics_date(raw: &str) -> Result<NaiveDate> {
    let raw = raw.trim();
    if raw.len() >= 8 && raw.chars().all(|c| c.is_ascii_digit() || c == 'T' || c == 'Z') {
        let date_part = &raw[..8];
        let y: i32 = date_part[0..4].parse()?;
        let m: u32 = date_part[4..6].parse()?;
        let d: u32 = date_part[6..8].parse()?;
        return NaiveDate::from_ymd_opt(y, m, d).context("invalid ICS date");
    }
    anyhow::bail!("unsupported ICS date format: {raw}");
}

fn unfold_ics_lines(ics: &str) -> String {
    let mut result = String::new();
    let mut current = String::new();
    for line in ics.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            current.push_str(line.trim_start());
        } else {
            if !current.is_empty() {
                result.push_str(&current);
                result.push('\n');
            }
            current = line.to_string();
        }
    }
    if !current.is_empty() {
        result.push_str(&current);
        result.push('\n');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_week_range() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 18).unwrap(); // Monday
        let (start, end) = next_week_range(as_of);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 5, 25).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 5, 31).unwrap());
    }

    #[test]
    fn test_parse_fixture_trips() {
        let ics = include_str!("../tests/fixtures/travel_calendar.ics");
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 18).unwrap();
        let trips = trips_for_next_week(ics, as_of).unwrap();
        assert_eq!(trips.len(), 1);
        assert_eq!(trips[0].title, "Chicago, IL");
    }

    #[test]
    fn test_trips_on_date() {
        let ics = include_str!("../tests/fixtures/travel_calendar.ics");
        let monday = NaiveDate::from_ymd_opt(2026, 5, 25).unwrap();
        let trips = trips_on_date(ics, monday).unwrap();
        assert_eq!(trips.len(), 1);
        assert_eq!(trips[0].title, "Chicago, IL");
        let home = NaiveDate::from_ymd_opt(2026, 5, 19).unwrap();
        assert!(trips_on_date(ics, home).unwrap().is_empty());
    }
}
