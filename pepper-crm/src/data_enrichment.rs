//! # Data Enrichment Picks
//!
//!   Surfaces contacts who need address fixes you can make in the dashboard — not “GEO pending” backlog.
//!
//! INPUT:
//!   - Parsed `Contact` list, cache root (geocode failure cache), and `as_of` date.
//!
//! OUTPUT:
//!   - Up to three stable weekly picks with issue labels for the dashboard.
//!
//! NOTES:
//!   - Excludes contacts that merely lack GEO but have a geocodable address (handled on save / batch geocode).
//!   - Includes ill-formed ADR and addresses whose geocode queries all failed in cache.
//!   - Saving location dismisses the contact for the week once they no longer match any issue.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use crate::calendar::{iso_week_id, week_range_containing};
use crate::contact_geo::{contact_geocode_queries_all_failed, contact_has_unusable_geo};
use crate::models::{Contact, DataEnrichmentInfo, DataEnrichmentIssue, DataEnrichmentWeek};
use crate::tags::is_random_pick_eligible;
use crate::vcard::geocode_queries_for_contact;
use anyhow::{Context, Result};
use chrono::NaiveDate;
use rand::seq::SliceRandom;
use rand::{rngs::StdRng, thread_rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

pub const DATA_ENRICHMENT_COUNT: usize = 3;

const DATA_ENRICHMENT_CACHE_SUBDIR: &str = "data_enrichment";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DismissedCache {
    week_id: String,
    uids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ShuffleCache {
    week_id: String,
    uids: Vec<String>,
}

fn week_seed(week_id: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    week_id.hash(&mut hasher);
    "pepper-data-enrichment".hash(&mut hasher);
    hasher.finish()
}

fn week_meta(as_of: NaiveDate) -> (String, NaiveDate) {
    let (week_start, _) = week_range_containing(as_of);
    (iso_week_id(week_start), week_start)
}

fn dismissed_cache_path(cache_root: &Path, week_id: &str) -> PathBuf {
    cache_root
        .join(DATA_ENRICHMENT_CACHE_SUBDIR)
        .join(format!("{week_id}-dismissed.json"))
}

fn shuffle_cache_path(cache_root: &Path, week_id: &str) -> PathBuf {
    cache_root
        .join(DATA_ENRICHMENT_CACHE_SUBDIR)
        .join(format!("{week_id}-shuffle.json"))
}

fn load_shuffle_override(cache_root: &Path, week_id: &str) -> Result<Option<Vec<String>>> {
    let path = shuffle_cache_path(cache_root, week_id);
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(&path)
        .with_context(|| format!("read enrichment shuffle {}", path.display()))?;
    let cache: ShuffleCache = serde_json::from_str(&data)?;
    if cache.week_id != week_id {
        return Ok(None);
    }
    Ok(Some(cache.uids))
}

fn save_shuffle_override(cache_root: &Path, week_id: &str, uids: &[String]) -> Result<()> {
    let path = shuffle_cache_path(cache_root, week_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let cache = ShuffleCache {
        week_id: week_id.to_string(),
        uids: uids.to_vec(),
    };
    std::fs::write(&path, serde_json::to_string_pretty(&cache)?)?;
    Ok(())
}

fn load_dismissed_uids(cache_root: &Path, week_id: &str) -> Result<HashSet<String>> {
    let path = dismissed_cache_path(cache_root, week_id);
    if !path.exists() {
        return Ok(HashSet::new());
    }
    let data = std::fs::read_to_string(&path)
        .with_context(|| format!("read enrichment dismiss {}", path.display()))?;
    let cache: DismissedCache = serde_json::from_str(&data)?;
    if cache.week_id != week_id {
        return Ok(HashSet::new());
    }
    Ok(cache.uids.into_iter().collect())
}

/// Hide a contact from enrichment picks for the rest of the ISO week (after address + GEO are OK).
pub fn dismiss_enrichment_pick(
    cache_root: impl AsRef<Path>,
    as_of: NaiveDate,
    uid: &str,
) -> Result<()> {
    let (week_id, _) = week_meta(as_of);
    let path = dismissed_cache_path(cache_root.as_ref(), &week_id);
    let mut uids: Vec<String> = if path.exists() {
        let data = std::fs::read_to_string(&path)?;
        let cache: DismissedCache = serde_json::from_str(&data).unwrap_or(DismissedCache {
            week_id: week_id.clone(),
            uids: vec![],
        });
        if cache.week_id == week_id {
            cache.uids
        } else {
            vec![]
        }
    } else {
        vec![]
    };
    if uids.iter().any(|u| u == uid) {
        return Ok(());
    }
    uids.push(uid.to_string());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let cache = DismissedCache { week_id, uids };
    std::fs::write(&path, serde_json::to_string_pretty(&cache)?)?;
    Ok(())
}

/// True when any ADR component is present on the parsed contact.
pub fn has_location_hints(contact: &Contact) -> bool {
    [&contact.street, &contact.city, &contact.state, &contact.country]
        .iter()
        .any(|f| f.as_ref().is_some_and(|s| !s.trim().is_empty()))
}

/// True when the contact is a person card that still needs address work you can do in the UI.
pub fn is_data_enrichment_eligible(contact: &Contact, cache_root: &Path) -> bool {
    is_random_pick_eligible(contact)
        && enrichment_issue(contact, cache_root)
            .unwrap_or(None)
            .is_some()
}

/// Why this contact appears in the enrichment queue (None when complete or not user-fixable).
pub fn enrichment_issue(contact: &Contact, cache_root: &Path) -> Result<Option<DataEnrichmentIssue>> {
    let queries = geocode_queries_for_contact(contact);
    if queries.is_empty() {
        return Ok(if has_location_hints(contact) {
            Some(DataEnrichmentIssue::IllFormedAddress)
        } else {
            Some(DataEnrichmentIssue::MissingAddress)
        });
    }

    if contact_has_unusable_geo(contact) {
        return Ok(Some(DataEnrichmentIssue::GeocodeFailed));
    }

    if contact.geo.is_none()
        && contact_geocode_queries_all_failed(cache_root, contact)?
    {
        return Ok(Some(DataEnrichmentIssue::GeocodeFailed));
    }

    Ok(None)
}

fn contact_to_info(contact: &Contact, issue: DataEnrichmentIssue) -> DataEnrichmentInfo {
    DataEnrichmentInfo {
        uid: contact.uid.clone(),
        full_name: contact.full_name.clone(),
        org: contact.org.clone(),
        street: contact.street.clone(),
        city: contact.city.clone(),
        state: contact.state.clone(),
        issue,
    }
}

fn issue_sort_key(issue: DataEnrichmentIssue) -> u8 {
    match issue {
        DataEnrichmentIssue::MissingAddress => 0,
        DataEnrichmentIssue::IllFormedAddress => 1,
        DataEnrichmentIssue::GeocodeFailed => 2,
    }
}

/// Eligible contacts with their enrichment issue, sorted deterministically (issue, name, uid).
fn eligible_with_issues<'a>(
    contacts: &'a [Contact],
    cache_root: &Path,
    dismissed: &HashSet<String>,
) -> Vec<(&'a Contact, DataEnrichmentIssue)> {
    let mut eligible: Vec<(&Contact, DataEnrichmentIssue)> = contacts
        .iter()
        .filter(|c| is_random_pick_eligible(c) && !dismissed.contains(&c.uid))
        .filter_map(|c| {
            enrichment_issue(c, cache_root)
                .ok()
                .flatten()
                .map(|issue| (c, issue))
        })
        .collect();

    eligible.sort_by(|(a, issue_a), (b, issue_b)| {
        issue_sort_key(*issue_a)
            .cmp(&issue_sort_key(*issue_b))
            .then_with(|| a.full_name.cmp(&b.full_name))
            .then_with(|| a.uid.cmp(&b.uid))
    });
    eligible
}

/// Pick up to `count` contacts for the ISO week containing `as_of` (stable weekly seed).
pub fn data_enrichment_picks(
    contacts: &[Contact],
    cache_root: impl AsRef<Path>,
    as_of: NaiveDate,
    count: usize,
) -> Result<DataEnrichmentWeek> {
    let cache_root = cache_root.as_ref();
    let (week_id, _) = week_meta(as_of);
    let dismissed = load_dismissed_uids(cache_root, &week_id)?;

    let mut eligible = eligible_with_issues(contacts, cache_root, &dismissed);
    let eligible_count = eligible.len();
    let mut rng = StdRng::seed_from_u64(week_seed(&week_id));
    eligible.shuffle(&mut rng);

    let picks = eligible
        .into_iter()
        .take(count)
        .map(|(c, issue)| contact_to_info(c, issue))
        .collect();

    Ok(DataEnrichmentWeek {
        picks,
        eligible_count,
        shuffled: false,
    })
}

/// Non-deterministic shuffle that avoids `exclude_uids` while enough other contacts exist.
fn data_enrichment_picks_shuffled(
    contacts: &[Contact],
    cache_root: &Path,
    count: usize,
    exclude_uids: &[String],
    dismissed: &HashSet<String>,
) -> DataEnrichmentWeek {
    let eligible = eligible_with_issues(contacts, cache_root, dismissed);
    let eligible_count = eligible.len();

    if eligible.is_empty() {
        return DataEnrichmentWeek {
            picks: Vec::new(),
            eligible_count: 0,
            shuffled: true,
        };
    }

    let exclude: HashSet<&str> = exclude_uids.iter().map(|s| s.as_str()).collect();
    let mut pool: Vec<(&Contact, DataEnrichmentIssue)> = eligible
        .iter()
        .copied()
        .filter(|(c, _)| !exclude.contains(c.uid.as_str()))
        .collect();
    if pool.len() < count {
        pool = eligible;
    }

    pool.shuffle(&mut thread_rng());

    let take = count.min(pool.len());
    let picks = pool[..take]
        .iter()
        .map(|(c, issue)| contact_to_info(c, *issue))
        .collect();

    DataEnrichmentWeek {
        picks,
        eligible_count,
        shuffled: true,
    }
}

fn build_week_from_uids(
    contacts: &[Contact],
    cache_root: &Path,
    uids: &[String],
    dismissed: &HashSet<String>,
) -> Result<DataEnrichmentWeek> {
    let eligible_count = eligible_with_issues(contacts, cache_root, dismissed).len();
    let by_uid: HashMap<&str, &Contact> = contacts.iter().map(|c| (c.uid.as_str(), c)).collect();
    let mut picks = Vec::new();
    for uid in uids {
        let Some(contact) = by_uid.get(uid.as_str()) else {
            continue;
        };
        if dismissed.contains(uid) || !is_random_pick_eligible(contact) {
            continue;
        }
        if let Some(issue) = enrichment_issue(contact, cache_root)? {
            picks.push(contact_to_info(contact, issue));
        }
    }
    Ok(DataEnrichmentWeek {
        picks,
        eligible_count,
        shuffled: true,
    })
}

/// Resolve picks for the dashboard: shuffle override if present, else weekly default.
pub fn resolve_data_enrichment_picks(
    contacts: &[Contact],
    cache_root: impl AsRef<Path>,
    as_of: NaiveDate,
    count: usize,
) -> Result<DataEnrichmentWeek> {
    let cache_root = cache_root.as_ref();
    let (week_id, _) = week_meta(as_of);
    let dismissed = load_dismissed_uids(cache_root, &week_id)?;
    if let Some(uids) = load_shuffle_override(cache_root, &week_id)? {
        if !uids.is_empty() {
            let week = build_week_from_uids(contacts, cache_root, &uids, &dismissed)?;
            // Fall back to the weekly draw once every shuffled pick has been resolved.
            if !week.picks.is_empty() {
                return Ok(week);
            }
        }
    }
    data_enrichment_picks(contacts, cache_root, as_of, count)
}

/// Shuffle, persist override for this ISO week, and return fresh picks.
pub fn shuffle_and_save_enrichment(
    contacts: &[Contact],
    cache_root: impl AsRef<Path>,
    as_of: NaiveDate,
    count: usize,
    current_uids: &[String],
) -> Result<DataEnrichmentWeek> {
    let cache_root = cache_root.as_ref();
    let (week_id, _) = week_meta(as_of);
    let dismissed = load_dismissed_uids(cache_root, &week_id)?;
    let week =
        data_enrichment_picks_shuffled(contacts, cache_root, count, current_uids, &dismissed);
    let uids: Vec<String> = week.picks.iter().map(|p| p.uid.clone()).collect();
    save_shuffle_override(cache_root, &week_id, &uids)?;
    Ok(week)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::{is_plausible_geo_point, FileGeocodeCache, GeoPoint};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn sample_contact(uid: &str, name: &str) -> Contact {
        Contact {
            uid: uid.to_string(),
            full_name: name.to_string(),
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
            birthday: None,
            rev: None,
            log_entries: vec![],
            vcf_path: PathBuf::from(format!("/tmp/{uid}.vcf")),
            carddav_href: None,
        }
    }

    #[test]
    fn missing_address_is_eligible() {
        let dir = TempDir::new().unwrap();
        let c = sample_contact("u1", "Alex");
        assert_eq!(
            enrichment_issue(&c, dir.path()).unwrap(),
            Some(DataEnrichmentIssue::MissingAddress)
        );
    }

    #[test]
    fn address_without_geo_is_not_eligible_until_geocode_fails() {
        let dir = TempDir::new().unwrap();
        let mut c = sample_contact("u2", "Blair");
        c.city = Some("Boston".into());
        c.state = Some("MA".into());
        assert!(enrichment_issue(&c, dir.path()).unwrap().is_none());

        let cache = FileGeocodeCache::new(dir.path(), 7);
        for q in geocode_queries_for_contact(&c) {
            cache.write_failure(&q).unwrap();
        }
        assert_eq!(
            enrichment_issue(&c, dir.path()).unwrap(),
            Some(DataEnrichmentIssue::GeocodeFailed)
        );
    }

    #[test]
    fn ill_formed_address_with_only_state() {
        let dir = TempDir::new().unwrap();
        let mut c = sample_contact("u4", "Dana");
        c.state = Some("MA".into());
        assert_eq!(
            enrichment_issue(&c, dir.path()).unwrap(),
            Some(DataEnrichmentIssue::IllFormedAddress)
        );
    }

    #[test]
    fn invalid_geo_is_eligible() {
        let dir = TempDir::new().unwrap();
        let mut c = sample_contact("u5", "Evan");
        c.city = Some("Boston".into());
        c.state = Some("MA".into());
        c.geo = Some(GeoPoint { lat: 0.0, lng: 0.0 });
        assert!(!is_plausible_geo_point(c.geo.unwrap()));
        assert_eq!(
            enrichment_issue(&c, dir.path()).unwrap(),
            Some(DataEnrichmentIssue::GeocodeFailed)
        );
    }

    #[test]
    fn complete_geo_is_not_eligible() {
        let dir = TempDir::new().unwrap();
        let mut c = sample_contact("u3", "Casey");
        c.city = Some("Boston".into());
        c.state = Some("MA".into());
        c.geo = Some(GeoPoint {
            lat: 42.36,
            lng: -71.06,
        });
        c.geo_source = Some("boston, ma".into());
        assert!(enrichment_issue(&c, dir.path()).unwrap().is_none());
    }

    #[test]
    fn venue_and_do_not_engage_excluded() {
        let dir = TempDir::new().unwrap();
        let mut c = sample_contact("v1", "Venue: Cafe");
        assert!(!is_data_enrichment_eligible(&c, dir.path()));

        c.full_name = "Friend".into();
        c.categories = vec!["Do Not Engage".into()];
        assert!(!is_data_enrichment_eligible(&c, dir.path()));
    }

    #[test]
    fn picks_are_stable_for_the_week() {
        let dir = TempDir::new().unwrap();
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 28).unwrap();
        let contacts: Vec<Contact> = (0..8)
            .map(|i| sample_contact(&format!("u{i}"), &format!("Person {i}")))
            .collect();
        let a = data_enrichment_picks(&contacts, dir.path(), as_of, DATA_ENRICHMENT_COUNT).unwrap();
        let b = data_enrichment_picks(&contacts, dir.path(), as_of, DATA_ENRICHMENT_COUNT).unwrap();
        assert_eq!(a.picks.len(), DATA_ENRICHMENT_COUNT);
        assert_eq!(
            a.picks.iter().map(|p| p.uid.as_str()).collect::<Vec<_>>(),
            b.picks.iter().map(|p| p.uid.as_str()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn shuffle_override_round_trips_and_persists() {
        let dir = TempDir::new().unwrap();
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 28).unwrap();
        let contacts: Vec<Contact> = (0..8)
            .map(|i| sample_contact(&format!("u{i}"), &format!("Person {i}")))
            .collect();

        let shuffled = shuffle_and_save_enrichment(
            &contacts,
            dir.path(),
            as_of,
            DATA_ENRICHMENT_COUNT,
            &[],
        )
        .unwrap();
        assert!(shuffled.shuffled);
        assert_eq!(shuffled.picks.len(), DATA_ENRICHMENT_COUNT);

        let resolved =
            resolve_data_enrichment_picks(&contacts, dir.path(), as_of, DATA_ENRICHMENT_COUNT)
                .unwrap();
        assert!(resolved.shuffled);
        assert_eq!(
            resolved.picks.iter().map(|p| &p.uid).collect::<Vec<_>>(),
            shuffled.picks.iter().map(|p| &p.uid).collect::<Vec<_>>()
        );
    }

    #[test]
    fn resolve_falls_back_to_weekly_without_override() {
        let dir = TempDir::new().unwrap();
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 28).unwrap();
        let contacts: Vec<Contact> = (0..8)
            .map(|i| sample_contact(&format!("u{i}"), &format!("Person {i}")))
            .collect();
        let weekly =
            data_enrichment_picks(&contacts, dir.path(), as_of, DATA_ENRICHMENT_COUNT).unwrap();
        let resolved =
            resolve_data_enrichment_picks(&contacts, dir.path(), as_of, DATA_ENRICHMENT_COUNT)
                .unwrap();
        assert!(!resolved.shuffled);
        assert_eq!(
            resolved.picks.iter().map(|p| &p.uid).collect::<Vec<_>>(),
            weekly.picks.iter().map(|p| &p.uid).collect::<Vec<_>>()
        );
    }

    #[test]
    fn dismiss_hides_contact_for_the_week() {
        let dir = TempDir::new().unwrap();
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 28).unwrap();
        let nancy = sample_contact("nancy", "Nancy");
        let before =
            data_enrichment_picks(&[nancy.clone()], dir.path(), as_of, DATA_ENRICHMENT_COUNT)
                .unwrap();
        assert_eq!(before.picks.len(), 1);
        assert_eq!(before.picks[0].uid, "nancy");

        dismiss_enrichment_pick(dir.path(), as_of, "nancy").unwrap();

        let after =
            data_enrichment_picks(&[nancy], dir.path(), as_of, DATA_ENRICHMENT_COUNT).unwrap();
        assert!(after.picks.is_empty());
    }
}
