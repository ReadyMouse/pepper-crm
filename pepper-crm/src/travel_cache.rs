//! # Travel Snapshot Cache
//!
//!   Persists and loads weekly `TravelWeekSnapshot` JSON files and resolves the target ISO week
//!   for builds (with optional `TRAVEL_WEEK_OVERRIDE`).
//!
//! INPUT:
//!   - Cache root path, `week_id` string, `as_of` date, snapshot JSON on disk.
//!
//! OUTPUT:
//!   - `TravelWeekSnapshot` load/save; snapshot file paths; contact removal from current week cache.
//!
//! NOTES:
//!   - Files live at `.cache/travel/{week_id}.json` (e.g. `2026-W21.json`).
//!   - `load_current_snapshot` drops snapshots whose stored `week_id` does not match the target week.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use crate::calendar::{iso_week_id, next_week_range};
use crate::models::TravelWeekSnapshot;
use anyhow::{Context, Result};
use chrono::NaiveDate;
use std::path::{Path, PathBuf};

const TRAVEL_CACHE_SUBDIR: &str = "travel";

/// Resolve target week from env override or `next_week` relative to today.
pub fn target_week_for_build(as_of: NaiveDate) -> (String, NaiveDate, NaiveDate) {
    if let Ok(override_id) = std::env::var("TRAVEL_WEEK_OVERRIDE") {
        if let Some((start, end)) = parse_week_override(&override_id) {
            return (override_id.clone(), start, end);
        }
    }
    let (start, end) = next_week_range(as_of);
    let id = iso_week_id(start);
    (id, start, end)
}

fn parse_week_override(id: &str) -> Option<(NaiveDate, NaiveDate)> {
    // Format: 2026-W21 — Monday of that ISO week through Sunday
    let (year, week) = id.split_once("-W")?;
    let year: i32 = year.parse().ok()?;
    let week: u32 = week.parse().ok()?;
    let monday = chrono::NaiveDate::from_isoywd_opt(year, week, chrono::Weekday::Mon)?;
    let sunday = monday + chrono::Duration::days(6);
    Some((monday, sunday))
}

pub fn snapshot_path(cache_root: impl AsRef<Path>, week_id: &str) -> PathBuf {
    cache_root
        .as_ref()
        .join(TRAVEL_CACHE_SUBDIR)
        .join(format!("{week_id}.json"))
}

pub fn load_snapshot(cache_root: impl AsRef<Path>, week_id: &str) -> Result<Option<TravelWeekSnapshot>> {
    let path = snapshot_path(&cache_root, week_id);
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(&path)
        .with_context(|| format!("read travel snapshot {}", path.display()))?;
    let snap: TravelWeekSnapshot = serde_json::from_str(&data)?;
    Ok(Some(snap))
}

pub fn save_snapshot(cache_root: impl AsRef<Path>, snapshot: &TravelWeekSnapshot) -> Result<PathBuf> {
    let path = snapshot_path(&cache_root, &snapshot.week_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(snapshot)?)?;
    Ok(path)
}

/// Load snapshot for the target week if `week_id` matches current target.
/// Drop one contact from the current week's cached travel list (after snooze write-back).
pub fn remove_contact_from_current_snapshot(
    cache_root: impl AsRef<Path>,
    as_of: NaiveDate,
    uid: &str,
) -> Result<bool> {
    let (week_id, _, _) = target_week_for_build(as_of);
    let Some(mut snap) = load_snapshot(&cache_root, &week_id)? else {
        return Ok(false);
    };
    let before: usize = snap.trips.iter().map(|t| t.matches.len()).sum();
    for trip in &mut snap.trips {
        trip.matches.retain(|m| m.uid != uid);
    }
    let after: usize = snap.trips.iter().map(|t| t.matches.len()).sum();
    if after == before {
        return Ok(false);
    }
    save_snapshot(cache_root, &snap)?;
    Ok(true)
}

pub fn load_current_snapshot(cache_root: impl AsRef<Path>, as_of: NaiveDate) -> Result<Option<TravelWeekSnapshot>> {
    let (week_id, _, _) = target_week_for_build(as_of);
    let snap = load_snapshot(cache_root, &week_id)?;
    if let Some(ref s) = snap {
        if s.week_id != week_id {
            return Ok(None);
        }
    }
    Ok(snap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MatchReason, TravelMatch, TravelTripWithMatches};
    use chrono::Utc;
    use tempfile::tempdir;

    #[test]
    fn test_save_and_load_snapshot() {
        let dir = tempdir().unwrap();
        let (week_id, start, end) = target_week_for_build(NaiveDate::from_ymd_opt(2026, 5, 18).unwrap());
        let snap = TravelWeekSnapshot {
            week_id: week_id.clone(),
            week_start: start,
            week_end: end,
            built_at: Utc::now(),
            metro_radius_km: 50.0,
            trips: vec![TravelTripWithMatches {
                title: "Chicago, IL".to_string(),
                start,
                end,
                matches: vec![TravelMatch {
                    uid: "u1".to_string(),
                    full_name: "Test".to_string(),
                    city: Some("Chicago".to_string()),
                    distance_km: 0.0,
                    reason: MatchReason::Proximity,
                    reconnect_tag: None,
                }],
            }],
        };
        save_snapshot(dir.path(), &snap).unwrap();
        let loaded = load_snapshot(dir.path(), &week_id).unwrap().unwrap();
        assert_eq!(loaded.match_count(), 1);
    }
}
