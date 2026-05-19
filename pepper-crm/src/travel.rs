//! Build weekly travel match lists from calendar + VCF contacts.

use crate::calendar::{fetch_ics, trips_for_next_week};
use crate::contact_geo::{ensure_contacts_geocoded, ensure_contacts_geocoded_sync, needs_geocoding};
use crate::geo::{
    km_to_miles, miles_to_km, GeoPoint, Geocoder, NominatimGeocoder, DEFAULT_METRO_RADIUS_MI,
};
use crate::models::{
    Contact, MatchReason, TravelMatch, TravelTrip, TravelTripWithMatches, TravelWeekSnapshot,
};
use crate::tags::{
    city_fuzzy_matches_trip, extract_trip_city, is_city_trigger, is_travel_match_eligible,
};
use crate::travel_cache::{save_snapshot, target_week_for_build};
use crate::vcard::{contact_address_query, parse_vcards_from_dir};
use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use std::path::PathBuf;
use tracing::info;

/// Options for building a weekly travel snapshot.
#[derive(Debug, Clone)]
pub struct TravelBuildConfig {
    pub contacts_dir: PathBuf,
    pub cache_root: PathBuf,
    pub ics_url: Option<String>,
    pub ics_content: Option<String>,
    pub metro_radius_km: f64,
    pub as_of: NaiveDate,
    pub force: bool,
    /// When true, missing/stale GEO is written back to vCard files (default on).
    pub write_geo_to_vcf: bool,
}

impl TravelBuildConfig {
    pub fn from_env(as_of: NaiveDate) -> Self {
        let contacts_dir = std::env::var("CONTACTS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./contacts"));
        let cache_root = std::env::var("CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(".cache"));
        let ics_url = std::env::var("GOOGLE_CALENDAR_ICS_URL").ok();
        let metro_radius_km = std::env::var("METRO_RADIUS_KM")
            .ok()
            .and_then(|s| s.parse().ok())
            .or_else(|| {
                std::env::var("METRO_RADIUS_MI")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .map(|mi| miles_to_km(mi))
            })
            .unwrap_or_else(|| miles_to_km(DEFAULT_METRO_RADIUS_MI as f64));
        let write_geo_to_vcf = std::env::var("GEO_WRITE_TO_VCF")
            .map(|s| {
                let lower = s.to_lowercase();
                lower != "0" && lower != "false" && lower != "no"
            })
            .unwrap_or(true);
        Self {
            contacts_dir,
            cache_root,
            ics_url,
            ics_content: None,
            metro_radius_km,
            as_of,
            force: false,
            write_geo_to_vcf,
        }
    }
}

/// Build snapshot using Nominatim geocoder (production, async).
pub async fn build_travel_week_snapshot(config: &TravelBuildConfig) -> Result<TravelWeekSnapshot> {
    let geocoder = NominatimGeocoder::from_env(&config.cache_root)?;
    build_travel_week_snapshot_async(config, &geocoder).await
}

/// Sync entry point (CLI / tests outside a runtime). Prefer [`build_travel_week_snapshot`].
pub fn build_travel_week_snapshot_blocking(config: &TravelBuildConfig) -> Result<TravelWeekSnapshot> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(build_travel_week_snapshot(config)))
    } else {
        tokio::runtime::Runtime::new()
            .context("tokio runtime for travel build")?
            .block_on(build_travel_week_snapshot(config))
    }
}

async fn build_travel_week_snapshot_async(
    config: &TravelBuildConfig,
    geocoder: &NominatimGeocoder,
) -> Result<TravelWeekSnapshot> {
    let (week_id, week_start, week_end) = target_week_for_build(config.as_of);

    if !config.force {
        if let Some(existing) =
            crate::travel_cache::load_snapshot(&config.cache_root, &week_id)?
        {
            if existing.week_id == week_id {
                return Ok(existing);
            }
        }
    }

    let ics = resolve_ics_content_async(config).await?;
    let trips = trips_for_next_week(&ics, config.as_of)?;
    let mut contacts = parse_vcards_from_dir(&config.contacts_dir)?;
    ensure_contacts_geocoded(&mut contacts, geocoder, config.write_geo_to_vcf).await?;

    let mut trip_results = Vec::new();
    for trip in trips {
        let matches = match_contacts_for_trip_async(
            &trip,
            &contacts,
            geocoder,
            config.metro_radius_km,
            config.as_of,
        )
        .await?;
        trip_results.push(TravelTripWithMatches {
            title: trip.title.clone(),
            start: trip.start,
            end: trip.end,
            matches,
        });
    }

    let snapshot = TravelWeekSnapshot {
        week_id,
        week_start,
        week_end,
        built_at: Utc::now(),
        metro_radius_km: config.metro_radius_km,
        trips: trip_results,
    };

    save_snapshot(&config.cache_root, &snapshot)?;
    info!(
        "Travel snapshot built: {} trips, {} total matches (radius {:.0} mi)",
        snapshot.trips.len(),
        snapshot
            .trips
            .iter()
            .map(|t| t.matches.len())
            .sum::<usize>(),
        km_to_miles(config.metro_radius_km)
    );
    Ok(snapshot)
}

/// Build snapshot with an injected geocoder (unit tests, sync only).
pub fn build_travel_week_snapshot_with_geocoder<G: Geocoder>(
    config: &TravelBuildConfig,
    geocoder: &G,
) -> Result<TravelWeekSnapshot> {
    let (week_id, week_start, week_end) = target_week_for_build(config.as_of);

    if !config.force {
        if let Some(existing) =
            crate::travel_cache::load_snapshot(&config.cache_root, &week_id)?
        {
            if existing.week_id == week_id {
                return Ok(existing);
            }
        }
    }

    let ics = match &config.ics_content {
        Some(c) => c.clone(),
        None => anyhow::bail!("ics_content required for sync geocoder test builds"),
    };
    let trips = trips_for_next_week(&ics, config.as_of)?;
    let mut contacts = parse_vcards_from_dir(&config.contacts_dir)?;
    ensure_contacts_geocoded_sync(&mut contacts, geocoder, config.write_geo_to_vcf)?;

    let mut trip_results = Vec::new();
    for trip in trips {
        let matches =
            match_contacts_for_trip(&trip, &contacts, geocoder, config.metro_radius_km, config.as_of)?;
        trip_results.push(TravelTripWithMatches {
            title: trip.title.clone(),
            start: trip.start,
            end: trip.end,
            matches,
        });
    }

    let snapshot = TravelWeekSnapshot {
        week_id,
        week_start,
        week_end,
        built_at: Utc::now(),
        metro_radius_km: config.metro_radius_km,
        trips: trip_results,
    };

    save_snapshot(&config.cache_root, &snapshot)?;
    Ok(snapshot)
}

async fn resolve_ics_content_async(config: &TravelBuildConfig) -> Result<String> {
    if let Some(ref content) = config.ics_content {
        return Ok(content.clone());
    }
    let url = config
        .ics_url
        .as_deref()
        .context("GOOGLE_CALENDAR_ICS_URL is not set")?;
    fetch_ics(url).await
}

async fn match_contacts_for_trip_async(
    trip: &TravelTrip,
    contacts: &[Contact],
    geocoder: &NominatimGeocoder,
    radius_km: f64,
    as_of: NaiveDate,
) -> Result<Vec<TravelMatch>> {
    let trip_point = geocoder.geocode_async(&trip.title).await?;
    let mut candidates: Vec<TravelMatch> = Vec::new();

    for contact in contacts {
        if !is_travel_match_eligible(contact, as_of) {
            continue;
        }
        if contact_address_query(contact).is_none() {
            continue;
        }
        let contact_point = match resolve_contact_point(contact, geocoder) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let distance_km = crate::geo::haversine_km(trip_point, contact_point);
        if distance_km > radius_km {
            continue;
        }

        push_travel_match(&mut candidates, contact, &trip.title, distance_km);
    }

    sort_travel_matches(&mut candidates);
    Ok(candidates)
}

fn match_contacts_for_trip<G: Geocoder>(
    trip: &TravelTrip,
    contacts: &[Contact],
    geocoder: &G,
    radius_km: f64,
    as_of: NaiveDate,
) -> Result<Vec<TravelMatch>> {
    let trip_point = geocoder.geocode(&trip.title)?;
    let mut candidates: Vec<TravelMatch> = Vec::new();

    for contact in contacts {
        if !is_travel_match_eligible(contact, as_of) {
            continue;
        }
        if contact_address_query(contact).is_none() {
            continue;
        }
        let contact_point = match resolve_contact_point(contact, geocoder) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let distance_km = crate::geo::haversine_km(trip_point, contact_point);
        if distance_km > radius_km {
            continue;
        }

        push_travel_match(&mut candidates, contact, &trip.title, distance_km);
    }

    sort_travel_matches(&mut candidates);
    Ok(candidates)
}

fn push_travel_match(
    candidates: &mut Vec<TravelMatch>,
    contact: &Contact,
    trip_title: &str,
    distance_km: f64,
) {
    let tagged = contact
        .reconnect_tag
        .as_deref()
        .filter(|t| is_city_trigger(t))
        .and_then(|t| extract_trip_city(t))
        .map(|city| city_fuzzy_matches_trip(trip_title, &city))
        .unwrap_or(false);

    let reason = if tagged {
        MatchReason::TaggedBeforeTrip
    } else {
        MatchReason::Proximity
    };

    candidates.push(TravelMatch {
        uid: contact.uid.clone(),
        full_name: contact.full_name.clone(),
        city: contact.city.clone(),
        distance_km,
        reason,
        reconnect_tag: contact.reconnect_tag.clone(),
    });
}

fn sort_travel_matches(candidates: &mut [TravelMatch]) {
    candidates.sort_by(|a, b| {
        let rank = |m: &TravelMatch| {
            (
                !matches!(m.reason, MatchReason::TaggedBeforeTrip),
                (m.distance_km * 100.0) as i64,
                m.full_name.clone(),
            )
        };
        rank(a).cmp(&rank(b))
    });
}

fn resolve_contact_point<G: Geocoder>(contact: &Contact, geocoder: &G) -> Result<GeoPoint> {
    if let Some(p) = contact.geo {
        if !needs_geocoding(contact) {
            return Ok(p);
        }
    }
    let query = contact_address_query(contact).context("no address for contact")?;
    geocoder.geocode(&query)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::FixedGeocoder;
    use crate::tags::RECONNECT_CATEGORY_PREFIX;
    use std::path::PathBuf;

    fn contact(
        uid: &str,
        name: &str,
        city: &str,
        state: &str,
        reconnect: Option<&str>,
        categories: Vec<String>,
    ) -> Contact {
        Contact {
            uid: uid.to_string(),
            full_name: name.to_string(),
            email: None,
            phone: None,
            org: None,
            city: Some(city.to_string()),
            state: Some(state.to_string()),
            country: Some("USA".to_string()),
            categories,
            note_raw: "April 2025: Met for testing.".to_string(),
            todos: vec![],
            reconnect_tag: reconnect.map(str::to_string),
            rev: None,
            log_entries: vec![],
            vcf_path: PathBuf::from(format!("/tmp/{uid}.vcf")),
            geo: None,
            geo_source: None,
        }
    }

    #[test]
    fn test_excludes_reconnect_never() {
        let trip = TravelTrip {
            title: "Chicago, IL".to_string(),
            start: NaiveDate::from_ymd_opt(2026, 5, 26).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 5, 29).unwrap(),
        };
        let geocoder = FixedGeocoder::us_metro_fixture();
        let contacts = vec![contact(
            "n1",
            "Never Person",
            "Chicago",
            "IL",
            Some("Never"),
            vec![format!("{RECONNECT_CATEGORY_PREFIX} Never")],
        )];
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 18).unwrap();
        let matches = match_contacts_for_trip(&trip, &contacts, &geocoder, 50.0, as_of).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn test_excludes_contact_without_dated_note() {
        let trip = TravelTrip {
            title: "Chicago, IL".to_string(),
            start: NaiveDate::from_ymd_opt(2026, 5, 26).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 5, 29).unwrap(),
        };
        let geocoder = FixedGeocoder::us_metro_fixture();
        let mut c = contact("c1", "Evanston", "Evanston", "IL", None, vec![]);
        c.note_raw = "Met at conference, no month stamp.".to_string();
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 18).unwrap();
        let matches = match_contacts_for_trip(&trip, &[c], &geocoder, 50.0, as_of).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn test_chicago_includes_evanston_excludes_littleton() {
        let trip = TravelTrip {
            title: "Chicago, IL".to_string(),
            start: NaiveDate::from_ymd_opt(2026, 5, 26).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 5, 29).unwrap(),
        };
        let geocoder = FixedGeocoder::us_metro_fixture();
        let contacts = vec![
            contact("c1", "Evanston", "Evanston", "IL", None, vec![]),
            contact("c2", "Littleton", "Littleton", "CO", None, vec![]),
        ];
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 18).unwrap();
        let matches = match_contacts_for_trip(&trip, &contacts, &geocoder, 50.0, as_of).unwrap();
        let names: Vec<_> = matches.iter().map(|m| m.full_name.as_str()).collect();
        assert!(names.contains(&"Evanston"));
        assert!(!names.contains(&"Littleton"));
    }

    #[test]
    fn test_trip_tag_ranks_first() {
        let trip = TravelTrip {
            title: "Chicago, IL".to_string(),
            start: NaiveDate::from_ymd_opt(2026, 5, 26).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 5, 29).unwrap(),
        };
        let geocoder = FixedGeocoder::us_metro_fixture();
        let contacts = vec![
            contact(
                "far",
                "Far Tagged",
                "Chicago",
                "IL",
                Some("before Chicago trip"),
                vec![format!("{RECONNECT_CATEGORY_PREFIX} before Chicago trip")],
            ),
            contact("near", "Near Plain", "Chicago", "IL", None, vec![]),
        ];
        let as_of = NaiveDate::from_ymd_opt(2026, 5, 18).unwrap();
        let matches = match_contacts_for_trip(&trip, &contacts, &geocoder, 50.0, as_of).unwrap();
        assert!(!matches.is_empty());
        assert_eq!(matches[0].full_name, "Far Tagged");
        assert_eq!(matches[0].reason, MatchReason::TaggedBeforeTrip);
    }

    #[test]
    fn test_build_from_fixture_ics() {
        let dir = tempfile::tempdir().unwrap();
        let contacts_dir = dir.path().join("contacts");
        std::fs::create_dir_all(&contacts_dir).unwrap();
        let vcf = r#"BEGIN:VCARD
VERSION:3.0
UID:chicago1
FN:Sofia Chen
ADR;TYPE=HOME:;;123 Main St;Chicago;IL;60601;USA
NOTE:May 2025: Met at conference.
CATEGORIES:Reconnect: 3 months
END:VCARD"#;
        std::fs::write(contacts_dir.join("sofia.vcf"), vcf).unwrap();

        let config = TravelBuildConfig {
            contacts_dir: contacts_dir.clone(),
            cache_root: dir.path().to_path_buf(),
            ics_url: None,
            ics_content: Some(include_str!("../tests/fixtures/travel_calendar.ics").to_string()),
            metro_radius_km: 50.0,
            as_of: NaiveDate::from_ymd_opt(2026, 5, 18).unwrap(),
            force: true,
            write_geo_to_vcf: true,
        };
        let geocoder = FixedGeocoder::us_metro_fixture();
        let snap = build_travel_week_snapshot_with_geocoder(&config, &geocoder).unwrap();
        assert_eq!(snap.trips.len(), 1);
        assert!(!snap.trips[0].matches.is_empty());
    }
}
