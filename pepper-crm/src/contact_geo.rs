//! # Contact GEO Ensure Pass
//!
//!   Geocodes contacts whose vCards lack GEO or have stale `X-PEPPER-GEO-SOURCE` relative to ADR,
//!   optionally writing coordinates back to VCF files before travel matching.
//!
//! INPUT:
//!   - Mutable `Contact` slice; `Geocoder` (Nominatim async or sync test geocoder); `write_back` flag.
//!
//! OUTPUT:
//!   - `GeocodeEnsureStats` (skipped, already_ok, geocoded, failed); updated in-memory `Contact.geo`.
//!
//! NOTES:
//!   - Legacy GEO without source line triggers a one-time refresh to stamp `X-PEPPER-GEO-SOURCE`.
//!   - Write-back failures are logged but do not abort the batch.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use crate::geo::{normalize_geocode_query, GeoPoint, Geocoder, NominatimGeocoder};
use crate::models::Contact;
use crate::vcard::{contact_address_query, write_contact_geo};
use anyhow::Result;
use tracing::info;

/// Stats from a geocode-ensure pass (before travel matching).
#[derive(Debug, Default, Clone, Copy)]
pub struct GeocodeEnsureStats {
    pub skipped_no_address: usize,
    pub already_ok: usize,
    pub geocoded: usize,
    pub failed: usize,
}

/// True when the contact has an address but no valid GEO for the current address.
pub fn needs_geocoding(contact: &Contact) -> bool {
    contact_address_query(contact).is_some() && (contact.geo.is_none() || is_geo_stale(contact))
}

/// GEO is stale when the normalized address query differs from `geo_source`.
pub fn is_geo_stale(contact: &Contact) -> bool {
    let Some(current) = contact_address_query(contact) else {
        return false;
    };
    let current = normalize_geocode_query(&current);
    match &contact.geo_source {
        Some(src) => normalize_geocode_query(src) != current,
        // GEO present but no source line (legacy) — refresh once to stamp source.
        None => contact.geo.is_some(),
    }
}

/// Geocode contacts that need it and optionally write `GEO` + `X-PEPPER-GEO-SOURCE` to vCard files.
pub async fn ensure_contacts_geocoded(
    contacts: &mut [Contact],
    geocoder: &NominatimGeocoder,
    write_back: bool,
) -> Result<GeocodeEnsureStats> {
    let mut stats = GeocodeEnsureStats::default();
    for contact in contacts.iter_mut() {
        if contact_address_query(contact).is_none() {
            stats.skipped_no_address += 1;
            continue;
        }
        if !needs_geocoding(contact) {
            stats.already_ok += 1;
            continue;
        }
        let query = contact_address_query(contact).expect("checked above");
        match geocoder.geocode_async(&query).await {
            Ok(point) => {
                let source = normalize_geocode_query(&query);
                apply_geocode_to_contact(contact, point, &source, write_back);
                stats.geocoded += 1;
            }
            Err(e) => {
                tracing::debug!(uid = %contact.uid, error = %e, "contact geocode failed");
                stats.failed += 1;
            }
        }
    }
    if stats.geocoded > 0 || stats.failed > 0 {
        info!(
            geocoded = stats.geocoded,
            already_ok = stats.already_ok,
            failed = stats.failed,
            skipped = stats.skipped_no_address,
            "Contact GEO ensure pass"
        );
    }
    Ok(stats)
}

/// Sync variant for unit tests and `FixedGeocoder` builds.
pub fn ensure_contacts_geocoded_sync<G: Geocoder>(
    contacts: &mut [Contact],
    geocoder: &G,
    write_back: bool,
) -> Result<GeocodeEnsureStats> {
    let mut stats = GeocodeEnsureStats::default();
    for contact in contacts.iter_mut() {
        if contact_address_query(contact).is_none() {
            stats.skipped_no_address += 1;
            continue;
        }
        if !needs_geocoding(contact) {
            stats.already_ok += 1;
            continue;
        }
        let query = contact_address_query(contact).expect("checked above");
        match geocoder.geocode(&query) {
            Ok(point) => {
                let source = normalize_geocode_query(&query);
                apply_geocode_to_contact(contact, point, &source, write_back);
                stats.geocoded += 1;
            }
            Err(e) => {
                tracing::debug!(uid = %contact.uid, error = %e, "contact geocode failed");
                stats.failed += 1;
            }
        }
    }
    Ok(stats)
}

fn apply_geocode_to_contact(
    contact: &mut Contact,
    point: GeoPoint,
    source: &str,
    write_back: bool,
) {
    if write_back {
        if let Err(e) = write_contact_geo(contact, point, source) {
            tracing::warn!(
                uid = %contact.uid,
                path = %contact.vcf_path.display(),
                error = %e,
                "GEO write-back to vCard failed"
            );
        }
    }
    contact.geo = Some(point);
    contact.geo_source = Some(source.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::GeoPoint;
    use std::path::PathBuf;

    fn contact_with_geo(city: &str, state: &str, geo: Option<GeoPoint>, source: Option<&str>) -> Contact {
        Contact {
            uid: "u1".to_string(),
            full_name: "Test".to_string(),
            email: None,
            phone: None,
            org: None,
            city: Some(city.to_string()),
            state: Some(state.to_string()),
            country: Some("USA".to_string()),
            geo,
            geo_source: source.map(str::to_string),
            categories: vec![],
            note_raw: String::new(),
            todos: vec![],
            reconnect_tag: None,
            rev: None,
            log_entries: vec![],
            vcf_path: PathBuf::from("/tmp/u1.vcf"),
        }
    }

    #[test]
    fn needs_geocode_when_geo_missing() {
        let c = contact_with_geo("Boston", "MA", None, None);
        assert!(needs_geocoding(&c));
    }

    #[test]
    fn ok_when_geo_matches_address() {
        let p = GeoPoint {
            lat: 42.36,
            lng: -71.06,
        };
        let c = contact_with_geo("Boston", "MA", Some(p), Some("boston, ma"));
        assert!(!needs_geocoding(&c));
        assert!(!is_geo_stale(&c));
    }

    #[test]
    fn stale_when_city_changes() {
        let p = GeoPoint {
            lat: 42.36,
            lng: -71.06,
        };
        let c = contact_with_geo("Cambridge", "MA", Some(p), Some("boston, ma"));
        assert!(is_geo_stale(&c));
        assert!(needs_geocoding(&c));
    }
}
