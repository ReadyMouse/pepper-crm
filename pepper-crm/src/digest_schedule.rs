//! # Weekly Digest Send Window
//!
//!   Decides when to send the Monday 6:00 digest based on calendar trips (destination timezone)
//!   with fallback to US Eastern. Cron polls hourly via `should_send_weekly_digest`.
//!
//! INPUT: ICS text, `DateTime<Utc>`, cache root (geocode cache + last-sent marker).
//! OUTPUT: `DigestScheduleInfo`, send-window checks, idempotent last-sent tracking.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use crate::calendar::{iso_week_id, trips_on_date, week_range_containing};
use crate::geo::{FileGeocodeCache, GeoPoint};
use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc, Weekday};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::warn;

pub const DIGEST_LOCAL_HOUR: u32 = 6;
pub const DEFAULT_DIGEST_TIMEZONE: &str = "America/New_York";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestLastSent {
    pub iso_week_id: String,
    pub sent_at: DateTime<Utc>,
    pub timezone: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trip_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestScheduleInfo {
    pub monday: NaiveDate,
    pub timezone: String,
    pub tz: Tz,
    pub trip_title: Option<String>,
}

/// Resolve IANA timezone for the digest Monday from calendar trips that day.
pub fn digest_schedule_for_monday(
    ics: &str,
    monday: NaiveDate,
    cache_root: &Path,
) -> Result<DigestScheduleInfo> {
    let trips = trips_on_date(ics, monday).unwrap_or_else(|e| {
        warn!("could not parse calendar for digest timezone: {e}");
        Vec::new()
    });
    let trip_title = trips.first().map(|t| t.title.clone());
    let place = trips.first().map(|t| t.title.as_str());
    let tz = place
        .map(|p| timezone_for_place(p, cache_root))
        .transpose()?
        .unwrap_or_else(default_digest_tz);
    Ok(DigestScheduleInfo {
        monday,
        timezone: tz.name().to_string(),
        tz,
        trip_title,
    })
}

/// Monday of the ISO week containing `now`, using default Eastern to pick the calendar day.
pub fn digest_monday_for_now(now: DateTime<Utc>) -> NaiveDate {
    let local = now.with_timezone(&default_digest_tz());
    week_range_containing(local.date_naive()).0
}

/// Full schedule for the digest Monday that applies to `now`.
pub fn digest_schedule_for_now(ics: &str, now: DateTime<Utc>, cache_root: &Path) -> Result<DigestScheduleInfo> {
    let monday = digest_monday_for_now(now);
    digest_schedule_for_monday(ics, monday, cache_root)
}

/// True during the Monday 6:00 hour in the given timezone (cron runs hourly).
pub fn is_digest_send_window(now: DateTime<Utc>, monday: NaiveDate, tz: Tz) -> bool {
    let local = now.with_timezone(&tz);
    local.date_naive() == monday
        && local.weekday() == Weekday::Mon
        && local.hour() == DIGEST_LOCAL_HOUR
}

/// Returns schedule info when the digest should be sent now; `None` if not due or already sent.
pub fn should_send_weekly_digest(
    ics: &str,
    now: DateTime<Utc>,
    cache_root: &Path,
) -> Result<Option<DigestScheduleInfo>> {
    let info = digest_schedule_for_now(ics, now, cache_root)?;
    if !is_digest_send_window(now, info.monday, info.tz) {
        return Ok(None);
    }
    let week_id = iso_week_id(info.monday);
    if let Some(last) = read_last_sent(cache_root)? {
        if last.iso_week_id == week_id {
            return Ok(None);
        }
    }
    Ok(Some(info))
}

pub fn mark_digest_sent(cache_root: &Path, info: &DigestScheduleInfo) -> Result<()> {
    let path = last_sent_path(cache_root);
    std::fs::create_dir_all(path.parent().unwrap())?;
    let record = DigestLastSent {
        iso_week_id: iso_week_id(info.monday),
        sent_at: Utc::now(),
        timezone: info.timezone.clone(),
        trip_title: info.trip_title.clone(),
    };
    std::fs::write(&path, serde_json::to_string_pretty(&record)?)?;
    Ok(())
}

pub fn read_last_sent(cache_root: &Path) -> Result<Option<DigestLastSent>> {
    let path = last_sent_path(cache_root);
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(&path)?;
    Ok(Some(serde_json::from_str(&data)?))
}

fn last_sent_path(cache_root: &Path) -> PathBuf {
    cache_root.join("digest").join("last_sent.json")
}

pub fn default_digest_tz() -> Tz {
    parse_tz(DEFAULT_DIGEST_TIMEZONE).unwrap_or(chrono_tz::America::New_York)
}

fn parse_tz(name: &str) -> Result<Tz> {
    name.parse::<Tz>()
        .with_context(|| format!("invalid IANA timezone: {name}"))
}

/// Map trip destination text to a timezone (known cities → geocode cache → tzf → Eastern).
pub fn timezone_for_place(place: &str, cache_root: &Path) -> Result<Tz> {
    if let Some(tz) = known_place_timezone(place) {
        return Ok(tz);
    }
    let ttl_days = std::env::var("GEOCODE_CACHE_TTL_DAYS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(7);
    let cache = FileGeocodeCache::new(cache_root, ttl_days);
    if let Some(point) = cache.read_any(place, true)? {
        if let Some(tz) = timezone_at_point(point) {
            return Ok(tz);
        }
    }
    warn!(
        "no timezone for trip destination {:?}, defaulting to {}",
        place, DEFAULT_DIGEST_TIMEZONE
    );
    Ok(default_digest_tz())
}

/// Load calendar ICS for scheduling; returns empty string on missing URL or fetch errors.
pub async fn fetch_schedule_ics() -> String {
    let Ok(url) = std::env::var("GOOGLE_CALENDAR_ICS_URL") else {
        return String::new();
    };
    let url = url.trim();
    if url.is_empty() {
        return String::new();
    }
    match crate::calendar::fetch_ics(url).await {
        Ok(text) => text,
        Err(e) => {
            warn!(
                "calendar fetch failed for digest schedule (defaulting to {}): {e}",
                DEFAULT_DIGEST_TIMEZONE
            );
            String::new()
        }
    }
}

/// Convenience for cron: fetch calendar and evaluate the send window at `Utc::now()`.
pub async fn should_send_weekly_digest_now(cache_root: &Path) -> Result<Option<DigestScheduleInfo>> {
    let ics = fetch_schedule_ics().await;
    should_send_weekly_digest(&ics, Utc::now(), cache_root)
}

fn timezone_at_point(point: GeoPoint) -> Option<Tz> {
    let finder = tzf_rs::DefaultFinder::new();
    let name = finder.get_tz_name(point.lng, point.lat);
    if name.is_empty() {
        return None;
    }
    parse_tz(name).ok()
}

/// Common trip SUMMARY strings → IANA zones (substring match on normalized title).
pub fn known_place_timezone(place: &str) -> Option<Tz> {
    let n = normalize_place_key(place);
    const RULES: &[(&[&str], &str)] = &[
        (&["chicago"], "America/Chicago"),
        (&["denver", "colorado springs"], "America/Denver"),
        (&["phoenix", "scottsdale"], "America/Phoenix"),
        (&["los angeles", "la trip", "san francisco", "san diego", "seattle", "portland"], "America/Los_Angeles"),
        (&["new york", "nyc", "brooklyn", "boston", "miami", "atlanta", "washington dc", "philadelphia"], "America/New_York"),
        (&["london"], "Europe/London"),
        (&["paris"], "Europe/Paris"),
        (&["berlin", "amsterdam"], "Europe/Berlin"),
        (&["tokyo"], "Asia/Tokyo"),
        (&["singapore"], "Asia/Singapore"),
        (&["hong kong"], "Asia/Hong_Kong"),
        (&["sydney", "melbourne"], "Australia/Sydney"),
        (&["toronto", "montreal", "vancouver"], "America/Toronto"),
        (&["honolulu", "hawaii"], "Pacific/Honolulu"),
    ];
    for (needles, tz_name) in RULES {
        if needles.iter().any(|needle| n.contains(needle)) {
            return parse_tz(tz_name).ok();
        }
    }
    None
}

fn normalize_place_key(place: &str) -> String {
    place
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use tempfile::TempDir;

    #[test]
    fn chicago_trip_monday_six_am_is_due() {
        let ics = include_str!("../tests/fixtures/travel_calendar.ics");
        let cache = TempDir::new().unwrap();
        // 2026-05-25 is Monday; trip covers that day
        let monday = NaiveDate::from_ymd_opt(2026, 5, 25).unwrap();
        let info = digest_schedule_for_monday(ics, monday, cache.path()).unwrap();
        assert_eq!(info.timezone, "America/Chicago");
        assert_eq!(info.trip_title.as_deref(), Some("Chicago, IL"));

        let tz: Tz = info.tz;
        let now = tz
            .with_ymd_and_hms(2026, 5, 25, 6, 15, 0)
            .unwrap()
            .with_timezone(&Utc);
        assert!(is_digest_send_window(now, monday, tz));
        assert!(should_send_weekly_digest(ics, now, cache.path())
            .unwrap()
            .is_some());
    }

    #[test]
    fn no_trip_defaults_to_eastern() {
        let ics = include_str!("../tests/fixtures/travel_calendar.ics");
        let cache = TempDir::new().unwrap();
        let monday = NaiveDate::from_ymd_opt(2026, 5, 18).unwrap();
        let info = digest_schedule_for_monday(ics, monday, cache.path()).unwrap();
        assert_eq!(info.timezone, "America/New_York");
        assert!(info.trip_title.is_none());
    }

    #[test]
    fn wrong_hour_not_due() {
        let ics = include_str!("../tests/fixtures/travel_calendar.ics");
        let cache = TempDir::new().unwrap();
        let monday = NaiveDate::from_ymd_opt(2026, 5, 25).unwrap();
        let tz = chrono_tz::America::Chicago;
        let now = tz
            .with_ymd_and_hms(2026, 5, 25, 5, 0, 0)
            .unwrap()
            .with_timezone(&Utc);
        assert!(!is_digest_send_window(now, monday, tz));
        assert!(should_send_weekly_digest(ics, now, cache.path())
            .unwrap()
            .is_none());
    }

    #[test]
    fn already_sent_skips() {
        let ics = include_str!("../tests/fixtures/travel_calendar.ics");
        let cache = TempDir::new().unwrap();
        let monday = NaiveDate::from_ymd_opt(2026, 5, 25).unwrap();
        let info = digest_schedule_for_monday(ics, monday, cache.path()).unwrap();
        mark_digest_sent(cache.path(), &info).unwrap();
        let tz = info.tz;
        let now = tz
            .with_ymd_and_hms(2026, 5, 25, 6, 0, 0)
            .unwrap()
            .with_timezone(&Utc);
        assert!(should_send_weekly_digest(ics, now, cache.path())
            .unwrap()
            .is_none());
    }

    #[test]
    fn known_city_chicago() {
        let cache = TempDir::new().unwrap();
        let tz = timezone_for_place("Chicago, IL", cache.path()).unwrap();
        assert_eq!(tz.name(), "America/Chicago");
    }
}
