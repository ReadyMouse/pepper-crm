//! # Weekly Random Contact Picks
//!
//!   Selects a small set of contacts at random for dashboard spotlight and enrichment prompts.
//!   Default picks are stable for the ISO week containing `as_of`; manual shuffle uses fresh randomness.
//!
//! INPUT:
//!   - Parsed `Contact` list, `as_of` date, optional cache root for shuffle overrides.
//!
//! OUTPUT:
//!   - `RandomPickWeek` with up to three eligible contacts and metadata for the UI.
//!
//! NOTES:
//!   - Excludes `Do Not Engage` and venue/business cards; `Reconnect: Never` is eligible.
//!   - Shuffle overrides persist at `.cache/random_pick/{week_id}-shuffle.json` until next ISO week.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use crate::calendar::{iso_week_id, week_range_containing};
use crate::models::{Contact, RandomPickInfo, RandomPickWeek};
use crate::tags::is_random_pick_eligible;
use anyhow::{Context, Result};
use chrono::NaiveDate;
use rand::seq::SliceRandom;
use rand::{rngs::StdRng, SeedableRng, thread_rng};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

pub const RANDOM_PICK_COUNT: usize = 3;

const RANDOM_PICK_CACHE_SUBDIR: &str = "random_pick";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ShuffleCache {
    week_id: String,
    uids: Vec<String>,
}

fn week_seed(week_id: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    week_id.hash(&mut hasher);
    "pepper-random-pick".hash(&mut hasher);
    hasher.finish()
}

fn format_week_label(start: NaiveDate, end: NaiveDate) -> String {
    format!(
        "{} – {}",
        start.format("%b %-d"),
        end.format("%b %-d, %Y")
    )
}

fn week_meta(as_of: NaiveDate) -> (String, String) {
    let (week_start, week_end) = week_range_containing(as_of);
    let week_id = iso_week_id(week_start);
    let week_label = format_week_label(week_start, week_end);
    (week_id, week_label)
}

/// True when the URL looks like a LinkedIn profile or company page.
pub fn is_linkedin_url(url: &str) -> bool {
    let lower = url.trim().to_lowercase();
    lower.contains("linkedin.com")
}

/// First LinkedIn URL from vCard `URL:` fields (not NOTE).
pub fn contact_linkedin_url(contact: &Contact) -> Option<&str> {
    contact.urls.iter().find(|u| is_linkedin_url(u)).map(|s| s.as_str())
}

fn contact_to_pick_info(contact: &Contact) -> RandomPickInfo {
    use crate::tags::resolve_reconnect_tag;
    let reconnect_tag =
        resolve_reconnect_tag(&contact.categories, &contact.note_raw).or_else(|| {
            contact
                .reconnect_tag
                .clone()
                .filter(|t| !t.trim().is_empty())
        });
    RandomPickInfo {
        uid: contact.uid.clone(),
        full_name: contact.full_name.clone(),
        org: contact.org.clone(),
        email: contact.email.clone(),
        phone: contact.phone.clone(),
        linkedin_url: contact_linkedin_url(contact).map(|s| s.to_string()),
        city: contact.city.clone(),
        state: contact.state.clone(),
        reconnect_tag,
        note: contact.note_raw.clone(),
        categories: contact.categories.clone(),
    }
}

fn eligible_contacts<'a>(contacts: &'a [Contact]) -> Vec<&'a Contact> {
    let mut eligible: Vec<&Contact> = contacts
        .iter()
        .filter(|c| is_random_pick_eligible(c))
        .collect();
    eligible.sort_by(|a, b| a.uid.cmp(&b.uid));
    eligible
}

fn picks_from_contacts(contacts: &[&Contact], count: usize) -> Vec<RandomPickInfo> {
    contacts
        .iter()
        .take(count)
        .map(|c| contact_to_pick_info(c))
        .collect()
}

fn shuffle_cache_path(cache_root: impl AsRef<Path>, week_id: &str) -> PathBuf {
    cache_root
        .as_ref()
        .join(RANDOM_PICK_CACHE_SUBDIR)
        .join(format!("{week_id}-shuffle.json"))
}

pub fn load_shuffle_override(
    cache_root: impl AsRef<Path>,
    as_of: NaiveDate,
) -> Result<Option<Vec<String>>> {
    let (week_id, _) = week_meta(as_of);
    let path = shuffle_cache_path(&cache_root, &week_id);
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(&path)
        .with_context(|| format!("read random pick shuffle {}", path.display()))?;
    let cache: ShuffleCache = serde_json::from_str(&data)?;
    if cache.week_id != week_id {
        return Ok(None);
    }
    Ok(Some(cache.uids))
}

pub fn save_shuffle_override(
    cache_root: impl AsRef<Path>,
    as_of: NaiveDate,
    uids: &[String],
) -> Result<()> {
    let (week_id, _) = week_meta(as_of);
    let path = shuffle_cache_path(&cache_root, &week_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let cache = ShuffleCache {
        week_id,
        uids: uids.to_vec(),
    };
    std::fs::write(&path, serde_json::to_string_pretty(&cache)?)?;
    Ok(())
}

fn build_week_from_uids(
    contacts: &[Contact],
    as_of: NaiveDate,
    uids: &[String],
    shuffled: bool,
) -> RandomPickWeek {
    let (week_id, week_label) = week_meta(as_of);
    let eligible_count = eligible_contacts(contacts).len();
    let by_uid: HashMap<&str, &Contact> = contacts.iter().map(|c| (c.uid.as_str(), c)).collect();
    let picks: Vec<RandomPickInfo> = uids
        .iter()
        .filter_map(|uid| by_uid.get(uid.as_str()).map(|c| contact_to_pick_info(c)))
        .collect();
    RandomPickWeek {
        week_id,
        week_label,
        picks,
        eligible_count,
        shuffled,
    }
}

/// Pick up to `count` random eligible contacts for the ISO week containing `as_of` (stable seed).
pub fn random_picks_for_week(
    contacts: &[Contact],
    as_of: NaiveDate,
    count: usize,
) -> RandomPickWeek {
    let (week_id, week_label) = week_meta(as_of);
    let eligible = eligible_contacts(contacts);
    let eligible_count = eligible.len();

    if eligible.is_empty() {
        return RandomPickWeek {
            week_id,
            week_label,
            picks: Vec::new(),
            eligible_count: 0,
            shuffled: false,
        };
    }

    let mut rng = StdRng::seed_from_u64(week_seed(&week_id));
    let mut pool = eligible;
    pool.shuffle(&mut rng);

    let take = count.min(pool.len());
    RandomPickWeek {
        week_id,
        week_label,
        picks: picks_from_contacts(&pool[..take], take),
        eligible_count,
        shuffled: false,
    }
}

/// Pick up to `count` random eligible contacts using non-deterministic randomness.
/// Tries to avoid `exclude_uids` when enough other contacts exist.
pub fn random_picks_shuffled(
    contacts: &[Contact],
    as_of: NaiveDate,
    count: usize,
    exclude_uids: &[String],
) -> RandomPickWeek {
    let (week_id, week_label) = week_meta(as_of);
    let eligible = eligible_contacts(contacts);
    let eligible_count = eligible.len();

    if eligible.is_empty() {
        return RandomPickWeek {
            week_id,
            week_label,
            picks: Vec::new(),
            eligible_count: 0,
            shuffled: true,
        };
    }

    let exclude: HashSet<&str> = exclude_uids.iter().map(|s| s.as_str()).collect();
    let mut pool: Vec<&Contact> = eligible
        .iter()
        .copied()
        .filter(|c| !exclude.contains(c.uid.as_str()))
        .collect();
    if pool.len() < count {
        pool = eligible;
    }

    pool.shuffle(&mut thread_rng());

    let take = count.min(pool.len());
    RandomPickWeek {
        week_id,
        week_label,
        picks: picks_from_contacts(&pool[..take], take),
        eligible_count,
        shuffled: true,
    }
}

/// Resolve picks for the dashboard: shuffle override if present, else weekly default.
pub fn resolve_random_picks(
    contacts: &[Contact],
    cache_root: impl AsRef<Path>,
    as_of: NaiveDate,
    count: usize,
) -> Result<RandomPickWeek> {
    if let Some(uids) = load_shuffle_override(&cache_root, as_of)? {
        if !uids.is_empty() {
            return Ok(build_week_from_uids(contacts, as_of, &uids, true));
        }
    }
    Ok(random_picks_for_week(contacts, as_of, count))
}

/// Shuffle, persist override for this ISO week, and return fresh picks.
pub fn shuffle_and_save(
    contacts: &[Contact],
    cache_root: impl AsRef<Path>,
    as_of: NaiveDate,
    count: usize,
    current_uids: &[String],
) -> Result<RandomPickWeek> {
    let week = random_picks_shuffled(contacts, as_of, count, current_uids);
    let uids: Vec<String> = week.picks.iter().map(|p| p.uid.clone()).collect();
    save_shuffle_override(cache_root, as_of, &uids)?;
    Ok(week)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Contact;
    use chrono::NaiveDate;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn sample(uid: &str, name: &str) -> Contact {
        Contact {
            uid: uid.into(),
            full_name: name.into(),
            email: None,
            phone: None,
            urls: vec![],
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
            birthday: None,
            rev: None,
            log_entries: vec![],
            vcf_path: PathBuf::from("x.vcf"),
        }
    }

    #[test]
    fn picks_are_stable_within_week() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 19).unwrap();
        let contacts: Vec<Contact> = (0..12)
            .map(|i| sample(&format!("uid-{i:02}"), &format!("Person {i}")))
            .collect();
        let a = random_picks_for_week(&contacts, as_of, 3);
        let b = random_picks_for_week(&contacts, as_of, 3);
        assert!(!a.shuffled);
        assert_eq!(
            a.picks.iter().map(|p| &p.uid).collect::<Vec<_>>(),
            b.picks.iter().map(|p| &p.uid).collect::<Vec<_>>()
        );
    }

    #[test]
    fn shuffle_differs_from_weekly_when_pool_is_large() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 19).unwrap();
        let contacts: Vec<Contact> = (0..30)
            .map(|i| sample(&format!("uid-{i:02}"), &format!("Person {i}")))
            .collect();
        let weekly = random_picks_for_week(&contacts, as_of, 3);
        let weekly_uids: Vec<_> = weekly.picks.iter().map(|p| p.uid.clone()).collect();

        let mut saw_different = false;
        for _ in 0..8 {
            let shuffled = random_picks_shuffled(&contacts, as_of, 3, &weekly_uids);
            assert!(shuffled.shuffled);
            if shuffled
                .picks
                .iter()
                .any(|p| !weekly_uids.contains(&p.uid))
            {
                saw_different = true;
                break;
            }
        }
        assert!(saw_different);
    }

    #[test]
    fn shuffle_cache_round_trip() {
        let dir = TempDir::new().unwrap();
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 19).unwrap();
        let contacts: Vec<Contact> = (0..10)
            .map(|i| sample(&format!("uid-{i:02}"), &format!("Person {i}")))
            .collect();
        let shuffled = random_picks_shuffled(&contacts, as_of, 3, &[]);
        let uids: Vec<String> = shuffled.picks.iter().map(|p| p.uid.clone()).collect();
        save_shuffle_override(dir.path(), as_of, &uids).unwrap();

        let resolved = resolve_random_picks(&contacts, dir.path(), as_of, 3).unwrap();
        assert!(resolved.shuffled);
        assert_eq!(
            resolved.picks.iter().map(|p| &p.uid).collect::<Vec<_>>(),
            shuffled.picks.iter().map(|p| &p.uid).collect::<Vec<_>>()
        );
    }

    #[test]
    fn excludes_do_not_engage_and_venue() {
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 19).unwrap();
        let mut mom = sample("mom", "Mom");
        mom.categories = vec!["Reconnect: Never".into()];

        let mut blocked = sample("blocked", "Blocked Person");
        blocked.categories = vec!["Do Not Engage".into()];

        let mut venue = sample("venue", "Venue/Business: Cafe");
        venue.categories = vec!["Venue/Business".into()];

        let ok = sample("ok", "Regular Friend");
        let result = random_picks_for_week(&[mom, blocked, venue, ok], as_of, 3);
        assert_eq!(result.eligible_count, 2);
        let names: Vec<_> = result.picks.iter().map(|p| p.full_name.as_str()).collect();
        assert!(names.contains(&"Mom") || names.contains(&"Regular Friend"));
        assert!(!names.iter().any(|n| *n == "Blocked Person"));
    }
}
