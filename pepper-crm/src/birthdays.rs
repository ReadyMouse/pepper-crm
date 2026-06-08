//! # Upcoming Birthdays
//!
//!   Parses vCard `BDAY` and lists contacts with a birthday in a forward-looking window.
//!
//! INPUT:
//!   - `Contact` slice with optional `birthday`, `as_of` date, window length in days.
//!
//! OUTPUT:
//!   - Sorted `UpcomingBirthdayInfo` rows for dashboard display.
//!
//! NOTES:
//!   - Excludes `Do Not Engage`; includes `Reconnect: Never`.
//!   - Recurring dates use month/day; year optional for age display.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use chrono::{Datelike, Duration, NaiveDate};
use crate::models::{Birthday, Contact, UpcomingBirthdayInfo};
use crate::tags::is_do_not_engage;

/// Default dashboard window: birthdays in the next two weeks.
pub const BIRTHDAY_WINDOW_DAYS: u32 = 14;

/// Parse vCard `BDAY` (`YYYY-MM-DD`, `YYYYMMDD`, or `--MMDD`).
pub fn parse_bday_value(value: &str) -> Option<Birthday> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if let Some(rest) = value.strip_prefix("--") {
        let (month, day) = parse_month_day(rest)?;
        return Some(Birthday {
            month,
            day,
            year: None,
        });
    }

    if value.len() >= 10 && value.as_bytes().get(4) == Some(&b'-') {
        let year: i32 = value.get(0..4)?.parse().ok()?;
        let month: u32 = value.get(5..7)?.parse().ok()?;
        let day: u32 = value.get(8..10)?.parse().ok()?;
        return Some(Birthday {
            month,
            day,
            year: Some(year),
        });
    }

    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 8 {
        let year: i32 = digits.get(0..4)?.parse().ok()?;
        let month: u32 = digits.get(4..6)?.parse().ok()?;
        let day: u32 = digits.get(6..8)?.parse().ok()?;
        return Some(Birthday {
            month,
            day,
            year: Some(year),
        });
    }

    None
}

fn parse_month_day(s: &str) -> Option<(u32, u32)> {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() != 4 {
        return None;
    }
    let month: u32 = digits.get(0..2)?.parse().ok()?;
    let day: u32 = digits.get(2..4)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((month, day))
}

/// Calendar date for `month`/`day` in `year`, with Feb 29 → Feb 28 on non-leap years.
pub fn birthday_on_date(month: u32, day: u32, year: i32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(year, month, day).or_else(|| {
        if month == 2 && day == 29 {
            NaiveDate::from_ymd_opt(year, 2, 28)
        } else {
            None
        }
    })
}

/// Next birthday on or after `as_of` (annual recurrence).
pub fn next_birthday_occurrence(month: u32, day: u32, as_of: NaiveDate) -> Option<NaiveDate> {
    let year = as_of.year();
    let this_year = birthday_on_date(month, day, year)?;
    if this_year >= as_of {
        return Some(this_year);
    }
    birthday_on_date(month, day, year + 1)
}

/// Contacts with `BDAY` falling between `as_of` and `as_of + window_days` (inclusive).
pub fn upcoming_birthdays_from_contacts(
    contacts: &[Contact],
    as_of: NaiveDate,
    window_days: u32,
) -> Vec<UpcomingBirthdayInfo> {
    let window_end = as_of + Duration::days(window_days as i64);
    let mut out = Vec::new();

    for contact in contacts {
        if is_do_not_engage(&contact.categories) {
            continue;
        }
        let Some(bday) = contact.birthday else {
            continue;
        };
        let Some(occurrence) = next_birthday_occurrence(bday.month, bday.day, as_of) else {
            continue;
        };
        if occurrence > window_end {
            continue;
        }
        let days_until = (occurrence - as_of).num_days().max(0) as u32;
        let turning_age = bday.year.and_then(|birth_year| {
            let age = occurrence.year() - birth_year;
            (age >= 0).then_some(age as u32)
        });
        out.push(UpcomingBirthdayInfo {
            uid: contact.uid.clone(),
            full_name: contact.full_name.clone(),
            occurrence,
            turning_age,
            days_until,
        });
    }

    out.sort_by(|a, b| {
        a.occurrence
            .cmp(&b.occurrence)
            .then_with(|| a.full_name.cmp(&b.full_name))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_bday_formats() {
        assert_eq!(
            parse_bday_value("1996-04-15"),
            Some(Birthday {
                month: 4,
                day: 15,
                year: Some(1996)
            })
        );
        assert_eq!(
            parse_bday_value("19960415"),
            Some(Birthday {
                month: 4,
                day: 15,
                year: Some(1996)
            })
        );
        assert_eq!(
            parse_bday_value("--0415"),
            Some(Birthday {
                month: 4,
                day: 15,
                year: None
            })
        );
    }

    #[test]
    fn test_upcoming_birthdays_window() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 19).unwrap();
        let soon = Contact {
            uid: "b1".into(),
            full_name: "Alex".into(),
            email: None,
            phone: None,
            urls: vec![],
            org: None,
            street: None,
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
            vcf_path: PathBuf::from("x.vcf"),
            carddav_href: None,
            birthday: Some(Birthday {
                month: 5,
                day: 25,
                year: Some(1990),
            }),
        };
        let mut later = soon.clone();
        later.uid = "b2".into();
        later.full_name = "Blair".into();
        later.birthday = Some(Birthday {
            month: 7,
            day: 1,
            year: None,
        });

        let mut blocked = soon.clone();
        blocked.uid = "b3".into();
        blocked.categories = vec!["Do Not Engage".into()];

        let list = upcoming_birthdays_from_contacts(&[soon, later, blocked], as_of, 14);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].full_name, "Alex");
        assert_eq!(list[0].days_until, 6);
        assert_eq!(list[0].turning_age, Some(36));
    }

    #[test]
    fn test_never_category_still_listed() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 19).unwrap();
        let mom = Contact {
            uid: "m1".into(),
            full_name: "Mom".into(),
            email: None,
            phone: None,
            urls: vec![],
            org: None,
            street: None,
            city: None,
            state: None,
            country: None,
            geo: None,
            geo_source: None,
            categories: vec!["Reconnect: Never".into()],
            note_raw: String::new(),
            todos: vec![],
            reconnect_tag: Some("Never".into()),
            rev: None,
            log_entries: vec![],
            vcf_path: PathBuf::from("x.vcf"),
            carddav_href: None,
            birthday: Some(Birthday {
                month: 5,
                day: 20,
                year: None,
            }),
        };
        let list = upcoming_birthdays_from_contacts(&[mom], as_of, 14);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].days_until, 1);
    }
}
