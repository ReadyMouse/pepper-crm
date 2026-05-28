//! # Pepper CRM Library Root
//!
//!   Crate entry point for the personal CRM core: vCard contacts, reconnect tags,
//!   PostgreSQL sync, calendar travel matching, and geocoding.
//!
//! INPUT:
//!   - None at the crate root (consumers import modules and re-exports).
//!
//! OUTPUT:
//!   - Public modules and re-exported types/functions for contacts, tags, DB, travel, and geo.
//!
//! NOTES:
//!   - Re-exports mirror the most common integration surface for the dashboard and CLI.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

pub mod birthdays;
pub mod calendar;
pub mod carddav;
pub mod contact_geo;
pub mod digest;
pub mod db;
pub mod geo;
pub mod ical;
pub mod mail;
pub mod models;
pub mod random_pick;
pub mod tags;
pub mod tasks;
pub mod travel;
pub mod travel_cache;
pub mod vcard;
pub mod weekly;

// Re-export commonly used types
pub use models::{
    Birthday, Contact, DigestCounts, DueItems, DueReconnectInfo, IcsFile, PendingTaskInfo,
    RandomPickInfo,
    RandomPickWeek, UpcomingBirthdayInfo,
    Reconnect, ReconnectRow, ReconnectStatus, Task, TaskRow, TaskStatus, UpsertResult,
};

// Re-export commonly used functions
pub use birthdays::{
    parse_bday_value, upcoming_birthdays_from_contacts, BIRTHDAY_WINDOW_DAYS,
};
pub use digest::{
    birthdays_from_contacts, build_digest_input, build_digest_input_from_due,
    digest_subject, digest_tera_context, random_picks_for_digest, reconnects_from_infos,
    render_digest_email, tasks_from_pending, tasks_from_rows, travel_trips_from_snapshot,
    DigestBirthday, DigestInput, DigestOutput, DigestRandomPick, DigestReconnect, DigestTask,
    DigestTravelMatch, DigestTravelTrip,
};
pub use calendar::{fetch_ics, next_week_range, trips_for_next_week, week_range_containing};
pub use db::{
    get_due_reconnects, get_due_tasks, log_digest, prune_stale_sync_data, sync_contacts_to_db,
    upsert_contacts_batch, PruneStaleResult,
};
pub use geo::{
    haversine_km, km_to_miles, miles_to_km, GeoPoint, Geocoder, DEFAULT_METRO_RADIUS_MI,
    KM_PER_MILE,
};
pub use ical::{build_ics, build_ics_batch, build_ics_for_due};
pub use mail::{load_dotenv, send_html_email};
pub use models::{
    MatchReason, TravelMatch, TravelTrip, TravelTripWithMatches, TravelWeekSnapshot,
};
pub use random_pick::{
    contact_linkedin_url, dismiss_random_pick, is_linkedin_url, random_picks_for_week,
    random_picks_shuffled, resolve_random_picks,
    shuffle_and_save, RANDOM_PICK_COUNT,
};
pub use tasks::{pending_tasks_from_contacts, remove_todo_from_note};
pub use tags::{
    format_month_year_note_prefix, has_recent_month_year_interaction_note,
    is_random_pick_eligible, is_reconnect_due_for_travel, is_reconnect_never,
    is_travel_match_eligible, is_venue_contact, due_reconnects_from_contacts, is_venue_label,
    is_do_not_engage, reconnect_anchor_date, reconnect_due_date, RANDOM_PICK_CATEGORY_OPTIONS,
    RECONNECT_SNOOZE_OPTIONS,
    TRAVEL_INTERACTION_WINDOW_MONTHS, parse_reconnect_category, resolve_reconnect_tag,
    DO_NOT_ENGAGE_CATEGORY, RECONNECT_CATEGORY_PREFIX,
};
pub use travel::{
    build_travel_week_snapshot, build_travel_week_snapshot_blocking,
    build_travel_week_snapshot_with_geocoder, TravelBuildConfig,
};
pub use travel_cache::{
    load_current_snapshot, load_snapshot, remove_contact_from_current_snapshot, save_snapshot,
    snapshot_path, target_week_for_build,
};
pub use contact_geo::{
    ensure_contacts_geocoded, ensure_contacts_geocoded_in_dir, ensure_contacts_geocoded_sync,
    geo_coverage, is_geo_stale, needs_geocoding, should_ensure_contact_geo, GeocodeEnsureStats,
};
pub use vcard::{
    contact_address_query, geocode_queries_for_contact, contacts_use_carddav, find_contact_by_uid,
    log_interaction, parse_contacts, parse_vcard, parse_vcards_from_dir,
    parse_rev_value, parse_vcards_from_path, set_contact_location, set_contact_note,
    set_random_pick_category, set_reconnect_snooze, write_contact_geo,
    complete_task,
    PEPPER_GEO_SOURCE,
};
pub use weekly::{
    fetch_due_items, run_weekly_digest, WeeklyDigestConfig, WeeklyDigestResult,
    RECONNECT_WINDOW_DAYS as WEEKLY_RECONNECT_WINDOW_DAYS,
};
