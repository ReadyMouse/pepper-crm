//! # Geocoding and Distance
//!
//!   Resolves place names to coordinates and computes Haversine distances for travel matching.
//!   Provides Nominatim geocoder, file cache, and test fixtures.
//!
//! INPUT:
//!   - Place query strings; `GeoPoint` pairs; env vars (`NOMINATIM_USER_AGENT`, cache TTL).
//!
//! OUTPUT:
//!   - `GeoPoint`, `haversine_km`, and `Geocoder` trait implementations.
//!
//! NOTES:
//!   - Nominatim requests are rate-limited to ~1 req/s; cache lives under `.cache/geocode/`.
//!   - `DEFAULT_METRO_RADIUS_MI` (30) is the default proximity threshold for travel matches.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

/// Default metro radius when not set in the UI or env.
pub const DEFAULT_METRO_RADIUS_MI: u32 = 30;
pub const KM_PER_MILE: f64 = 1.609_344;

/// Convert miles to kilometers (for Haversine / Nominatim matching).
pub fn miles_to_km(mi: f64) -> f64 {
    mi * KM_PER_MILE
}

/// Convert kilometers to miles (for display).
pub fn km_to_miles(km: f64) -> f64 {
    km / KM_PER_MILE
}

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration as StdDuration;

/// A resolved latitude/longitude for a place query.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoPoint {
    pub lat: f64,
    pub lng: f64,
}

/// Geocode a place name (with optional file-backed cache).
pub trait Geocoder: Send + Sync {
    fn geocode(&self, query: &str) -> Result<GeoPoint>;

    /// True when a recent miss is cached for this query (default: never).
    fn is_failure_cached(&self, _query: &str) -> Result<bool> {
        Ok(false)
    }
}

/// True when coordinates look like a real place (not null island / out of range).
pub fn is_plausible_geo_point(p: GeoPoint) -> bool {
    p.lat.is_finite()
        && p.lng.is_finite()
        && p.lat.abs() <= 90.0
        && p.lng.abs() <= 180.0
        && !(p.lat.abs() < 1e-6 && p.lng.abs() < 1e-6)
}

/// Great-circle distance in kilometers (Haversine).
pub fn haversine_km(a: GeoPoint, b: GeoPoint) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;
    let lat1 = a.lat.to_radians();
    let lat2 = b.lat.to_radians();
    let d_lat = (b.lat - a.lat).to_radians();
    let d_lng = (b.lng - a.lng).to_radians();

    let h = (d_lat / 2.0).sin().powi(2)
        + lat1.cos() * lat2.cos() * (d_lng / 2.0).sin().powi(2);
    let c = 2.0 * h.sqrt().asin();
    EARTH_RADIUS_KM * c
}

/// Normalize a cache key for place queries.
pub fn normalize_geocode_query(query: &str) -> String {
    query
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedGeoEntry {
    lat: f64,
    lng: f64,
    fetched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedGeoFailure {
    fetched_at: DateTime<Utc>,
}

/// File-backed geocode cache under `.cache/geocode/` (successes and failures).
pub struct FileGeocodeCache {
    cache_dir: PathBuf,
    fail_dir: PathBuf,
    ttl_days: i64,
}

impl FileGeocodeCache {
    pub fn new(cache_root: impl AsRef<Path>, ttl_days: i64) -> Self {
        let cache_dir = cache_root.as_ref().join("geocode");
        Self {
            fail_dir: cache_dir.join("fail"),
            cache_dir,
            ttl_days,
        }
    }

    fn cache_key(query: &str) -> String {
        let key = normalize_geocode_query(query);
        key.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect()
    }

    fn cache_path(&self, query: &str) -> PathBuf {
        self.cache_dir.join(format!("{}.json", Self::cache_key(query)))
    }

    fn failure_path(&self, query: &str) -> PathBuf {
        self.fail_dir.join(format!("{}.json", Self::cache_key(query)))
    }

    fn entry_fresh(fetched_at: DateTime<Utc>, ttl_days: i64) -> bool {
        Utc::now() - fetched_at <= Duration::days(ttl_days)
    }

    pub fn is_failure_cached(&self, query: &str) -> Result<bool> {
        let path = self.failure_path(query);
        if !path.exists() {
            return Ok(false);
        }
        let data = std::fs::read_to_string(&path)
            .with_context(|| format!("read geocode failure cache {}", path.display()))?;
        let entry: CachedGeoFailure = serde_json::from_str(&data)?;
        Ok(Self::entry_fresh(entry.fetched_at, self.ttl_days))
    }

    /// Record a geocode miss (used by Nominatim and tests).
    pub fn write_failure(&self, query: &str) -> Result<()> {
        std::fs::create_dir_all(&self.fail_dir)?;
        let entry = CachedGeoFailure {
            fetched_at: Utc::now(),
        };
        std::fs::write(
            self.failure_path(query),
            serde_json::to_string_pretty(&entry)?,
        )?;
        Ok(())
    }

    fn read(&self, query: &str) -> Result<Option<GeoPoint>> {
        self.read_entry(query, false)
    }

    /// Read cache entry ignoring TTL (fallback when Nominatim is rate-limited).
    fn read_stale(&self, query: &str) -> Result<Option<GeoPoint>> {
        self.read_entry(query, true)
    }

    fn read_entry(&self, query: &str, allow_stale: bool) -> Result<Option<GeoPoint>> {
        let path = self.cache_path(query);
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read_to_string(&path)
            .with_context(|| format!("read geocode cache {}", path.display()))?;
        let entry: CachedGeoEntry = serde_json::from_str(&data)?;
        if !allow_stale && !Self::entry_fresh(entry.fetched_at, self.ttl_days) {
            return Ok(None);
        }
        Ok(Some(GeoPoint {
            lat: entry.lat,
            lng: entry.lng,
        }))
    }

    /// Try the query and common place-name suffix variants (e.g. `Denver, CO` → `Denver, CO, USA`).
    pub fn read_any(&self, query: &str, allow_stale: bool) -> Result<Option<GeoPoint>> {
        for variant in place_query_variants(query) {
            let read = if allow_stale {
                self.read_stale(&variant)?
            } else {
                self.read(&variant)?
            };
            if read.is_some() {
                return Ok(read);
            }
        }
        Ok(None)
    }

    fn write(&self, query: &str, point: GeoPoint) -> Result<()> {
        std::fs::create_dir_all(&self.cache_dir)?;
        let entry = CachedGeoEntry {
            lat: point.lat,
            lng: point.lng,
            fetched_at: Utc::now(),
        };
        let path = self.cache_path(query);
        std::fs::write(&path, serde_json::to_string_pretty(&entry)?)?;
        Ok(())
    }
}

/// Ordered query variants for cache lookup and Nominatim (broader suffixes after the raw query).
pub fn place_query_variants(query: &str) -> Vec<String> {
    let q = query.trim();
    let mut variants = vec![q.to_string()];
    let lower = q.to_ascii_lowercase();
    if !lower.contains("usa")
        && !lower.contains("united states")
        && !lower.ends_with(", us")
    {
        variants.push(format!("{q}, USA"));
        variants.push(format!("{q}, US"));
    }
    variants.sort();
    variants.dedup();
    variants
}

fn is_rate_limited(err: &anyhow::Error) -> bool {
    err.to_string().contains("429")
}

fn is_permanent_geocode_miss(err: &anyhow::Error) -> bool {
    let msg = err.to_string();
    msg.contains("no geocode results") && !is_rate_limited(err)
}

/// Nominatim (OpenStreetMap) geocoder with file cache and rate limiting.
pub struct NominatimGeocoder {
    cache: FileGeocodeCache,
    user_agent: String,
    last_request: Mutex<Option<std::time::Instant>>,
}

impl NominatimGeocoder {
    pub fn from_env(cache_root: impl AsRef<Path>) -> Result<Self> {
        let user_agent = std::env::var("NOMINATIM_USER_AGENT")
            .unwrap_or_else(|_| "pepper-crm/1.0".to_string());
        let ttl_days = std::env::var("GEOCODE_CACHE_TTL_DAYS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(7);
        Ok(Self {
            cache: FileGeocodeCache::new(cache_root, ttl_days),
            user_agent,
            last_request: Mutex::new(None),
        })
    }

    fn throttle(&self) {
        let mut guard = self.last_request.lock().expect("geocoder lock");
        if let Some(last) = *guard {
            let elapsed = last.elapsed();
            if elapsed < StdDuration::from_secs(1) {
                std::thread::sleep(StdDuration::from_secs(1) - elapsed);
            }
        }
        *guard = Some(std::time::Instant::now());
    }

    async fn fetch_nominatim(&self, query: &str) -> Result<GeoPoint> {
        self.fetch_nominatim_with_retry(query, 4).await
    }

    async fn fetch_nominatim_with_retry(&self, query: &str, max_attempts: u32) -> Result<GeoPoint> {
        let mut last_err = None;
        for attempt in 0..max_attempts {
            match self.fetch_nominatim_once(query).await {
                Ok(point) => return Ok(point),
                Err(e) if is_rate_limited(&e) => {
                    last_err = Some(e);
                    if attempt + 1 < max_attempts {
                        let wait = StdDuration::from_secs(2u64.pow(attempt + 1));
                        tracing::warn!(
                            query,
                            wait_secs = wait.as_secs(),
                            "Nominatim rate limit (429); retrying"
                        );
                        tokio::time::sleep(wait).await;
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("nominatim request failed for {query}")))
    }

    async fn fetch_nominatim_once(&self, query: &str) -> Result<GeoPoint> {
        self.throttle();
        let client = reqwest::Client::new();
        let resp = client
            .get("https://nominatim.openstreetmap.org/search")
            .query(&[("q", query), ("format", "json"), ("limit", "1")])
            .header("User-Agent", &self.user_agent)
            .send()
            .await
            .context("nominatim request")?;
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            anyhow::bail!("HTTP status client error (429 Too Many Requests) for nominatim query {query}");
        }
        let body = resp.error_for_status()?.text().await?;
        let results: Vec<serde_json::Value> = serde_json::from_str(&body)?;
        let first = results
            .first()
            .context(format!("no geocode results for {query}"))?;
        let lat: f64 = first["lat"]
            .as_str()
            .context("lat")?
            .parse()
            .context("parse lat")?;
        let lng: f64 = first["lon"]
            .as_str()
            .context("lon")?
            .parse()
            .context("parse lon")?;
        Ok(GeoPoint { lat, lng })
    }
}

impl NominatimGeocoder {
    /// True when a recent Nominatim miss is cached for this query (skip network retry).
    pub fn is_failure_cached(&self, query: &str) -> Result<bool> {
        self.cache.is_failure_cached(query)
    }

    /// Geocode with cache and Nominatim HTTP (async).
    pub async fn geocode_async(&self, query: &str) -> Result<GeoPoint> {
        if let Some(cached) = self.cache.read_any(query, false)? {
            return Ok(cached);
        }
        let variants = place_query_variants(query);
        if variants
            .iter()
            .all(|q| self.cache.is_failure_cached(q).unwrap_or(false))
        {
            anyhow::bail!("geocode failure cached for {query}");
        }
        match self.fetch_nominatim(query).await {
            Ok(point) => {
                self.cache.write(query, point)?;
                Ok(point)
            }
            Err(e) if is_rate_limited(&e) => {
                if let Some(stale) = self.cache.read_any(query, true)? {
                    tracing::warn!(
                        query,
                        "Using stale geocode cache after Nominatim rate limit"
                    );
                    return Ok(stale);
                }
                Err(e)
            }
            Err(e) => {
                if is_permanent_geocode_miss(&e) {
                    let _ = self.cache.write_failure(query);
                }
                Err(e)
            }
        }
    }

    /// Try queries in order; returns the first hit and the query that matched.
    pub async fn geocode_queries_async(&self, queries: &[String]) -> Result<(GeoPoint, String)> {
        let mut last_err: Option<anyhow::Error> = None;
        for query in queries {
            if query.trim().is_empty() {
                continue;
            }
            match self.geocode_async(query).await {
                Ok(point) => return Ok((point, query.clone())),
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no geocode queries provided")))
    }

    /// Sync multi-query geocode for matching loops.
    pub fn geocode_queries(&self, queries: &[String]) -> Result<(GeoPoint, String)> {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(self.geocode_queries_async(queries)))
        } else {
            tokio::runtime::Runtime::new()
                .context("tokio runtime for geocode")?
                .block_on(self.geocode_queries_async(queries))
        }
    }
}

/// In-memory geocoder for tests (fixed coordinates).
pub struct FixedGeocoder {
    places: HashMap<String, GeoPoint>,
}

impl FixedGeocoder {
    pub fn new(places: HashMap<String, GeoPoint>) -> Self {
        Self { places }
    }

    pub fn us_metro_fixture() -> Self {
        let mut m = HashMap::new();
        m.insert(
            normalize_geocode_query("Chicago, IL"),
            GeoPoint {
                lat: 41.8781,
                lng: -87.6298,
            },
        );
        m.insert(
            normalize_geocode_query("Evanston, IL"),
            GeoPoint {
                lat: 42.0451,
                lng: -87.6877,
            },
        );
        m.insert(
            normalize_geocode_query("Littleton, CO"),
            GeoPoint {
                lat: 39.6133,
                lng: -105.0166,
            },
        );
        m.insert(
            normalize_geocode_query("Denver, CO"),
            GeoPoint {
                lat: 39.7392,
                lng: -104.9903,
            },
        );
        m.insert(
            normalize_geocode_query("Boulder, CO"),
            GeoPoint {
                lat: 40.0150,
                lng: -105.2705,
            },
        );
        // Trip titles may match these keys
        m.insert(
            normalize_geocode_query("Chicago"),
            GeoPoint {
                lat: 41.8781,
                lng: -87.6298,
            },
        );
        m.insert(
            normalize_geocode_query("Denver"),
            GeoPoint {
                lat: 39.7392,
                lng: -104.9903,
            },
        );
        Self { places: m }
    }
}

impl Geocoder for FixedGeocoder {
    fn geocode(&self, query: &str) -> Result<GeoPoint> {
        let key = normalize_geocode_query(query);
        self.places
            .get(&key)
            .copied()
            .with_context(|| format!("no fixed coordinates for {query}"))
    }
}

/// Cached wrapper around any geocoder.
pub struct CachingGeocoder<G: Geocoder> {
    inner: G,
    cache: FileGeocodeCache,
}

impl<G: Geocoder> CachingGeocoder<G> {
    pub fn new(inner: G, cache_root: impl AsRef<Path>, ttl_days: i64) -> Self {
        Self {
            inner,
            cache: FileGeocodeCache::new(cache_root, ttl_days),
        }
    }
}

impl Geocoder for NominatimGeocoder {
    fn geocode(&self, query: &str) -> Result<GeoPoint> {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(self.geocode_async(query)))
        } else {
            tokio::runtime::Runtime::new()
                .context("tokio runtime for geocode")?
                .block_on(self.geocode_async(query))
        }
    }

    fn is_failure_cached(&self, query: &str) -> Result<bool> {
        self.cache.is_failure_cached(query)
    }
}

impl<G: Geocoder> Geocoder for CachingGeocoder<G> {
    fn geocode(&self, query: &str) -> Result<GeoPoint> {
        if let Some(cached) = self.cache.read(query)? {
            return Ok(cached);
        }
        let point = self.inner.geocode(query)?;
        self.cache.write(query, point)?;
        Ok(point)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_place_query_variants_adds_country_suffixes() {
        let variants = place_query_variants("Denver, CO");
        assert!(variants.contains(&"Denver, CO".to_string()));
        assert!(variants.contains(&"Denver, CO, USA".to_string()));
    }

    #[test]
    fn test_read_any_finds_suffixed_cache_entry() {
        let dir = tempfile::tempdir().unwrap();
        let cache = FileGeocodeCache::new(dir.path(), 7);
        let point = GeoPoint {
            lat: 39.7392,
            lng: -104.9903,
        };
        std::fs::create_dir_all(&cache.cache_dir).unwrap();
        let entry = CachedGeoEntry {
            lat: point.lat,
            lng: point.lng,
            fetched_at: Utc::now(),
        };
        let path = cache.cache_path("Denver, CO, USA");
        std::fs::write(path, serde_json::to_string_pretty(&entry).unwrap()).unwrap();
        let got = cache.read_any("Denver, CO", false).unwrap().unwrap();
        assert_eq!(got, point);
    }

    #[test]
    fn test_geocode_failure_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = FileGeocodeCache::new(dir.path(), 7);
        assert!(!cache.is_failure_cached("Nowhereville, ZZ").unwrap());
        cache.write_failure("Nowhereville, ZZ").unwrap();
        assert!(cache.is_failure_cached("Nowhereville, ZZ").unwrap());
    }

    #[test]
    fn test_haversine_chicago_evanston() {
        let chicago = GeoPoint {
            lat: 41.8781,
            lng: -87.6298,
        };
        let evanston = GeoPoint {
            lat: 42.0451,
            lng: -87.6877,
        };
        let km = haversine_km(chicago, evanston);
        assert!(km > 15.0 && km < 30.0);
    }

    #[test]
    fn test_haversine_chicago_littleton_far() {
        let chicago = GeoPoint {
            lat: 41.8781,
            lng: -87.6298,
        };
        let littleton = GeoPoint {
            lat: 39.6133,
            lng: -105.0166,
        };
        let km = haversine_km(chicago, littleton);
        assert!(km > 500.0);
    }
}
