//! Geocoding and distance helpers for travel matching.

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

/// File-backed geocode cache under `.cache/geocode/`.
pub struct FileGeocodeCache {
    cache_dir: PathBuf,
    ttl_days: i64,
}

impl FileGeocodeCache {
    pub fn new(cache_root: impl AsRef<Path>, ttl_days: i64) -> Self {
        Self {
            cache_dir: cache_root.as_ref().join("geocode"),
            ttl_days,
        }
    }

    fn cache_path(&self, query: &str) -> PathBuf {
        let key = normalize_geocode_query(query);
        let safe: String = key
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        self.cache_dir.join(format!("{safe}.json"))
    }

    fn read(&self, query: &str) -> Result<Option<GeoPoint>> {
        let path = self.cache_path(query);
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read_to_string(&path)
            .with_context(|| format!("read geocode cache {}", path.display()))?;
        let entry: CachedGeoEntry = serde_json::from_str(&data)?;
        let age = Utc::now() - entry.fetched_at;
        if age > Duration::days(self.ttl_days) {
            return Ok(None);
        }
        Ok(Some(GeoPoint {
            lat: entry.lat,
            lng: entry.lng,
        }))
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
        self.throttle();
        let client = reqwest::Client::new();
        let resp = client
            .get("https://nominatim.openstreetmap.org/search")
            .query(&[("q", query), ("format", "json"), ("limit", "1")])
            .header("User-Agent", &self.user_agent)
            .send()
            .await
            .context("nominatim request")?;
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
    /// Geocode with cache and Nominatim HTTP (async).
    pub async fn geocode_async(&self, query: &str) -> Result<GeoPoint> {
        if let Some(cached) = self.cache.read(query)? {
            return Ok(cached);
        }
        let point = self.fetch_nominatim(query).await?;
        self.cache.write(query, point)?;
        Ok(point)
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
