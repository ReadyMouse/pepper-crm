//! # Domain Models
//!
//!   Shared data types for parsed vCard contacts, digest payloads, and weekly travel snapshots.
//!
//! INPUT:
//!   - VCF-derived fields and serde JSON for travel cache files.
//!
//! OUTPUT:
//!   - `Contact`, pending task/reconnect info, and travel structs.
//!
//! NOTES:
//!   - `Contact` is the in-memory source of truth; task and reconnect state lives in vCard fields.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use crate::geo::GeoPoint;
use chrono::{NaiveDate, DateTime, Utc};
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use std::path::PathBuf;

/// Represents a parsed contact from a VCF file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub uid: String,
    pub full_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    /// All `URL:` fields from the vCard (LinkedIn, personal site, etc.).
    pub urls: Vec<String>,
    pub org: Option<String>,
    /// Parsed from ADR street component (when city is in locality slot).
    pub street: Option<String>,
    pub city: Option<String>,       // parsed from ADR field
    pub state: Option<String>,      // parsed from ADR field
    pub country: Option<String>,
    /// Parsed from `GEO` (lat;lng). Written back after geocoding.
    pub geo: Option<GeoPoint>,
    /// Normalized address query stored in `X-PEPPER-GEO-SOURCE` when GEO was set.
    pub geo_source: Option<String>,
    pub categories: Vec<String>,    // vCard CATEGORIES (Reconnect: … lives here)
    pub note_raw: String,           // full raw NOTE field
    pub todos: Vec<String>,         // TODO: texts above CRM Log separator
    pub reconnect_tag: Option<String>,  // resolved from CATEGORIES, else NOTE
    /// Parsed from vCard `BDAY` (month/day required; year optional).
    pub birthday: Option<Birthday>,
    /// vCard `REV` revision date (anchor for reconnect interval math).
    pub rev: Option<NaiveDate>,
    pub log_entries: Vec<String>,   // lines from CRM Log block
    pub vcf_path: PathBuf,          // local file path (or synthetic name for CardDAV)
    /// Absolute or collection-relative href when loaded from CardDAV.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carddav_href: Option<String>,
}

/// Month/day (and optional birth year) from vCard `BDAY`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Birthday {
    pub month: u32,
    pub day: u32,
    pub year: Option<i32>,
}

/// Why a contact needs dashboard data enrichment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataEnrichmentIssue {
    MissingAddress,
    IllFormedAddress,
    GeocodeFailed,
}

/// One contact surfaced for address / GEO enrichment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataEnrichmentInfo {
    pub uid: String,
    pub full_name: String,
    pub org: Option<String>,
    pub street: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub issue: DataEnrichmentIssue,
}

/// Weekly data-enrichment spotlight (up to three picks).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataEnrichmentWeek {
    pub picks: Vec<DataEnrichmentInfo>,
    pub eligible_count: usize,
    /// True when picks came from a manual shuffle (not the default weekly draw).
    #[serde(default)]
    pub shuffled: bool,
}

/// One contact with a birthday in the dashboard window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpcomingBirthdayInfo {
    pub uid: String,
    pub full_name: String,
    pub occurrence: NaiveDate,
    pub turning_age: Option<u32>,
    pub days_until: u32,
}

/// A contact spotlighted in the weekly random-pick dashboard section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RandomPickInfo {
    pub uid: String,
    pub full_name: String,
    pub org: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    /// LinkedIn profile from vCard `URL:` when present.
    pub linkedin_url: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub reconnect_tag: Option<String>,
    /// Full vCard `NOTE` field.
    pub note: String,
    /// vCard `CATEGORIES` values (comma-joined for display).
    pub categories: Vec<String>,
}

/// Result of selecting random contacts for the current ISO week.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RandomPickWeek {
    pub week_id: String,
    pub week_label: String,
    pub picks: Vec<RandomPickInfo>,
    pub eligible_count: usize,
    /// True when picks came from a manual shuffle (not the default weekly draw).
    #[serde(default)]
    pub shuffled: bool,
}

/// A contact whose reconnect interval is due within a time window (computed from VCF).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DueReconnectInfo {
    pub uid: String,
    pub full_name: String,
    pub due_date: NaiveDate,
    pub tag: String,
}

/// An open TODO parsed from a contact vCard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingTaskInfo {
    pub uid: String,
    pub full_name: String,
    pub description: String,
}

/// ICS file attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IcsFile {
    pub filename: String,
    pub content: String,
}

/// A travel destination from calendar (event title = location).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TravelTrip {
    pub title: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
}

/// Why a contact was suggested for a trip.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MatchReason {
    TaggedBeforeTrip,
    Proximity,
}

/// One contact suggested for a trip.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct TravelMatch {
    pub uid: String,
    pub full_name: String,
    pub city: Option<String>,
    pub distance_km: f64,
    pub reason: MatchReason,
    pub reconnect_tag: Option<String>,
}

/// A trip with ranked contact matches (weekly snapshot unit).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct TravelTripWithMatches {
    pub title: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub matches: Vec<TravelMatch>,
}

/// Cached output for one target week (`next week`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct TravelWeekSnapshot {
    pub week_id: String,
    pub week_start: NaiveDate,
    pub week_end: NaiveDate,
    pub built_at: DateTime<Utc>,
    /// Haversine radius used for this build (km).
    #[serde(default = "default_snapshot_metro_radius_km")]
    pub metro_radius_km: f64,
    /// User-selected search place when the build was run (if any).
    #[serde(default)]
    pub search_location: Option<String>,
    pub trips: Vec<TravelTripWithMatches>,
}

fn default_snapshot_metro_radius_km() -> f64 {
    crate::geo::miles_to_km(crate::geo::DEFAULT_METRO_RADIUS_MI as f64)
}

impl TravelWeekSnapshot {
    /// Total contact suggestions across all trips.
    pub fn match_count(&self) -> usize {
        self.trips.iter().map(|t| t.matches.len()).sum()
    }
}
