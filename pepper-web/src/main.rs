//! Pepper Web Dashboard Server
//!
//!   Axum HTTP server for visualizing Pepper CRM data: tasks, reconnects, and travel matches.
//!
//! INPUT: CONTACTS_DIR, CACHE_DIR, GOOGLE_CALENDAR_ICS_URL (optional); VCF files.
//! OUTPUT: Routes `/`, `/preview`, `/travel/refresh`, `/travel/snooze`, `/tasks/complete`, `/random/shuffle`; static assets.
//! NOTES: Contacts and tasks live in VCF only (no PostgreSQL).
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use anyhow::{Context, Result};
use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use chrono::Local;
use pepper_crm::{
    build_travel_week_snapshot, complete_task, ensure_contacts_geocoded_in_dir,
    fetch_due_items, fetch_ics, find_contact_by_uid,
    trips_for_next_week, upcoming_birthdays_from_contacts, BIRTHDAY_WINDOW_DAYS,
    km_to_miles, load_current_snapshot, miles_to_km, parse_contacts, contacts_use_carddav,
    resolve_random_picks, shuffle_and_save, remove_contact_from_current_snapshot,
    dismiss_random_pick, set_random_pick_category, build_digest_input_from_due, render_digest_email,
    load_dotenv, DueReconnectInfo, MatchReason, RandomPickInfo, RandomPickWeek,
    RANDOM_PICK_COUNT, TravelBuildConfig, TravelWeekSnapshot, DEFAULT_METRO_RADIUS_MI,
    DO_NOT_ENGAGE_CATEGORY, RECONNECT_SNOOZE_OPTIONS,
};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};
use tera::{Context as TeraContext, Tera};
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing::info;

const RECONNECT_WINDOW_DAYS: u32 = 7;

#[derive(Clone)]
struct AppState {
    tera: Tera,
    cache_root: PathBuf,
    contacts_dir: PathBuf,
    /// Parsed VCF contacts (reloaded after edits). Avoids re-parsing on each request.
    contacts: Arc<RwLock<Arc<Vec<pepper_crm::Contact>>>>,
}

#[derive(Serialize)]
struct TaskView {
    uid: String,
    contact_name: String,
    description: String,
}

#[derive(Serialize)]
struct ReconnectView {
    uid: String,
    contact_name: String,
    due_date: String,
    tag: String,
}

#[derive(Serialize)]
struct TravelMatchView {
    uid: String,
    full_name: String,
    city: String,
    distance_mi: String,
    detail: String,
}

#[derive(Serialize)]
struct RandomPickView {
    uid: String,
    full_name: String,
    org: String,
    email: String,
    phone: String,
    location: String,
    reconnect_tag: String,
    categories: String,
    note: String,
    linkedin_url: String,
}

#[derive(Serialize)]
struct ReconnectSnoozeOption {
    value: String,
    label: String,
}

#[derive(Serialize)]
struct BirthdayView {
    uid: String,
    contact_name: String,
    date_label: String,
    when_label: String,
    age_label: String,
}

#[derive(Debug, Deserialize)]
struct DashboardQuery {
    refreshed: Option<String>,
    travel_error: Option<String>,
    snoozed: Option<String>,
    task_completed: Option<String>,
    random_shuffled: Option<String>,
    random_category_saved: Option<String>,
    geocoded: Option<String>,
    geo_error: Option<String>,
    geo_count: Option<String>,
    geo_failed: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TravelRefreshForm {
    search_location: String,
    #[serde(default = "default_form_metro_radius_mi")]
    metro_radius_mi: u32,
}

#[derive(Debug, Deserialize)]
struct TravelSnoozeForm {
    uid: String,
    reconnect: String,
}

#[derive(Debug, Deserialize)]
struct TaskCompleteForm {
    uid: String,
    description: String,
}

#[derive(Serialize)]
struct SnoozeJsonResponse {
    ok: bool,
    uid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn snooze_wants_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.contains("application/json"))
}

fn default_form_metro_radius_mi() -> u32 {
    DEFAULT_METRO_RADIUS_MI
}

const MIN_METRO_RADIUS_MI: u32 = 5;
const MAX_METRO_RADIUS_MI: u32 = 200;

fn clamp_metro_radius_mi(mi: u32) -> u32 {
    mi.clamp(MIN_METRO_RADIUS_MI, MAX_METRO_RADIUS_MI)
}

#[derive(Serialize)]
struct TravelTripView {
    title: String,
    date_range: String,
    matches: Vec<TravelMatchView>,
}

fn task_views(tasks: Vec<pepper_crm::PendingTaskInfo>) -> Vec<TaskView> {
    tasks
        .into_iter()
        .map(|t| TaskView {
            uid: t.uid,
            contact_name: t.full_name,
            description: t.description,
        })
        .collect()
}

fn format_location(city: Option<&str>, state: Option<&str>) -> String {
    match (city.filter(|s| !s.is_empty()), state.filter(|s| !s.is_empty())) {
        (Some(c), Some(s)) => format!("{c}, {s}"),
        (Some(c), None) => c.to_string(),
        (None, Some(s)) => s.to_string(),
        (None, None) => String::new(),
    }
}

fn random_pick_views(picks: Vec<RandomPickInfo>) -> Vec<RandomPickView> {
    picks
        .into_iter()
        .map(|p| RandomPickView {
            uid: p.uid,
            full_name: p.full_name,
            org: p.org.unwrap_or_default(),
            email: p.email.unwrap_or_default(),
            phone: p.phone.unwrap_or_default(),
            location: format_location(p.city.as_deref(), p.state.as_deref()),
            reconnect_tag: p.reconnect_tag.unwrap_or_default(),
            categories: p.categories.join(", "),
            note: p.note,
            linkedin_url: p.linkedin_url.unwrap_or_default(),
        })
        .collect()
}

fn reconnect_snooze_option_views() -> Vec<ReconnectSnoozeOption> {
    RECONNECT_SNOOZE_OPTIONS
        .iter()
        .map(|v| ReconnectSnoozeOption {
            value: (*v).to_string(),
            label: if *v == DO_NOT_ENGAGE_CATEGORY {
                DO_NOT_ENGAGE_CATEGORY.to_string()
            } else {
                format!("Reconnect: {v}")
            },
        })
        .collect()
}

fn random_pick_category_options() -> Vec<ReconnectSnoozeOption> {
    reconnect_snooze_option_views()
}

fn contacts_snapshot(state: &AppState) -> Arc<Vec<pepper_crm::Contact>> {
    state.contacts.read().expect("contacts lock poisoned").clone()
}

fn contact_by_uid(state: &AppState, uid: &str) -> Result<pepper_crm::Contact, String> {
    let contacts = state.contacts.read().expect("contacts lock poisoned");
    if let Some(contact) = contacts.iter().find(|c| c.uid == uid) {
        return Ok(contact.clone());
    }
    drop(contacts);
    find_contact_by_uid(&state.contacts_dir, uid).map_err(|e| format!("Contact not found: {e}"))
}

/// Parse VCF on a blocking thread — large exports overflow the default tokio worker stack.
async fn parse_contacts_blocking(contacts_dir: PathBuf) -> Result<Arc<Vec<pepper_crm::Contact>>> {
    tokio::task::spawn_blocking(move || {
        if !contacts_use_carddav() && !contacts_dir.exists() {
            return Ok(Arc::new(Vec::new()));
        }
        let contacts = parse_contacts(&contacts_dir).with_context(|| {
            if contacts_use_carddav() {
                "Failed to parse contacts from CardDAV".to_string()
            } else {
                format!("Failed to parse VCF files in {}", contacts_dir.display())
            }
        })?;
        Ok(Arc::new(contacts))
    })
    .await
    .context("contacts parse task failed")?
}

fn fetch_random_picks(contacts: &Arc<Vec<pepper_crm::Contact>>, cache_root: &PathBuf) -> Result<RandomPickWeek> {
    let as_of = Local::now().date_naive();
    resolve_random_picks(contacts, cache_root, as_of, RANDOM_PICK_COUNT)
}

async fn reload_contacts_cache(state: &AppState) -> Result<()> {
    let contacts = parse_contacts_blocking(state.contacts_dir.clone()).await?;
    info!("Reloaded {} contacts from {}", contacts.len(), if contacts_use_carddav() { "CardDAV" } else { "VCF" });
    *state.contacts.write().expect("contacts lock poisoned") = contacts;
    Ok(())
}

fn birthday_views(
    birthdays: Vec<pepper_crm::UpcomingBirthdayInfo>,
) -> Vec<BirthdayView> {
    birthdays
        .into_iter()
        .map(|b| {
            let date_label = b.occurrence.format("%b %-d").to_string();
            let when_label = match b.days_until {
                0 => "Today".to_string(),
                1 => "Tomorrow".to_string(),
                n => format!("In {n} days"),
            };
            let age_label = b
                .turning_age
                .map(|a| format!("Turning {a}"))
                .unwrap_or_default();
            BirthdayView {
                uid: b.uid,
                contact_name: b.full_name,
                date_label,
                when_label,
                age_label,
            }
        })
        .collect()
}

fn reconnect_views(reconnects: Vec<DueReconnectInfo>) -> Vec<ReconnectView> {
    reconnects
        .into_iter()
        .map(|r| ReconnectView {
            uid: r.uid,
            contact_name: r.full_name,
            due_date: r.due_date.format("%b %-d, %Y").to_string(),
            tag: r.tag,
        })
        .collect()
}

fn format_date_range(start: chrono::NaiveDate, end: chrono::NaiveDate) -> String {
    format!(
        "{} – {}",
        start.format("%b %-d"),
        end.format("%b %-d, %Y")
    )
}

fn snapshot_to_views(snapshot: &TravelWeekSnapshot) -> (Vec<TravelTripView>, usize) {
    let mut total = 0;
    let trips = snapshot
        .trips
        .iter()
        .map(|trip| {
            let matches: Vec<TravelMatchView> = trip
                .matches
                .iter()
                .map(|m| {
                    total += 1;
                    let city = m.city.clone().unwrap_or_else(|| "Unknown".to_string());
                    let distance_mi = format!("{:.0}", m.distance_km * 0.621371);
                    let detail = match &m.reason {
                        MatchReason::TaggedBeforeTrip => {
                            let tag = m
                                .reconnect_tag
                                .as_deref()
                                .unwrap_or("before trip");
                            format!(
                                "{city} · ~{distance_mi} mi · tagged \"Reconnect: {tag}\""
                            )
                        }
                        MatchReason::Proximity => {
                            format!("{city} · ~{distance_mi} mi")
                        }
                    };
                    TravelMatchView {
                        uid: m.uid.clone(),
                        full_name: m.full_name.clone(),
                        city,
                        distance_mi,
                        detail,
                    }
                })
                .collect();
            TravelTripView {
                title: trip.title.clone(),
                date_range: format_date_range(trip.start, trip.end),
                matches,
            }
        })
        .collect();
    (trips, total)
}

async fn load_contacts_from_vcf(contacts_dir: &PathBuf) -> Result<Arc<Vec<pepper_crm::Contact>>> {
    if !contacts_dir.exists() {
        info!(
            "Contacts directory {} not found, starting with empty contact list",
            contacts_dir.display()
        );
        return Ok(Arc::new(Vec::new()));
    }

    let contacts = parse_contacts_blocking(contacts_dir.clone()).await?;
    info!(
        "Loaded {} contacts from {}",
        contacts.len(),
        contacts_dir.display()
    );
    Ok(contacts)
}

fn fetch_due(contacts: &Arc<Vec<pepper_crm::Contact>>) -> (Vec<TaskView>, Vec<ReconnectView>) {
    let as_of = Local::now().date_naive();
    let (tasks, reconnects) = fetch_due_items(contacts, as_of);
    (task_views(tasks), reconnect_views(reconnects))
}

fn insert_travel_context(context: &mut TeraContext, snapshot: Option<&TravelWeekSnapshot>) {
    let has_ics = std::env::var("GOOGLE_CALENDAR_ICS_URL").is_ok();

    match snapshot {
        Some(snap) => {
            let (trips, count) = snapshot_to_views(snap);
            context.insert("travel_ready", &true);
            context.insert("travel_trips", &trips);
            context.insert("travel_match_count", &count);
            context.insert(
                "travel_built_at",
                &snap.built_at.format("%b %-d, %Y %H:%M UTC").to_string(),
            );
            context.insert(
                "travel_week_label",
                &format!(
                    "{} – {}",
                    snap.week_start.format("%b %-d"),
                    snap.week_end.format("%b %-d, %Y")
                ),
            );
            context.insert("travel_error", &Option::<String>::None);
            let radius_mi = km_to_miles(snap.metro_radius_km).round() as u32;
            context.insert("metro_radius_mi", &radius_mi);
            context.insert("metro_radius_built", &true);
            context.insert(
                "travel_search_near",
                &snap
                    .search_location
                    .as_ref()
                    .filter(|s| !s.trim().is_empty())
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        None => {
            context.insert("travel_ready", &false);
            context.insert("travel_trips", &Vec::<TravelTripView>::new());
            context.insert("travel_match_count", &0usize);
            context.insert("travel_built_at", &String::new());
            context.insert("travel_week_label", &String::new());
            context.insert("travel_search_near", &String::new());
            let err = if !has_ics {
                Some("Set GOOGLE_CALENDAR_ICS_URL in .env, then click Refresh travel matches.".to_string())
            } else {
                Some("No travel list for next week yet. Click Refresh travel matches to build it.".to_string())
            };
            context.insert("travel_error", &err);
            context.insert("metro_radius_mi", &DEFAULT_METRO_RADIUS_MI);
            context.insert("metro_radius_built", &false);
        }
    }
    context.insert("has_calendar_ics", &has_ics);
    context.insert("metro_radius_min", &MIN_METRO_RADIUS_MI);
    context.insert("metro_radius_max", &MAX_METRO_RADIUS_MI);
    let snooze_options = reconnect_snooze_option_views();
    context.insert("reconnect_snooze_options", &snooze_options);
}

async fn default_travel_search_location(
    as_of: chrono::NaiveDate,
    snapshot: Option<&TravelWeekSnapshot>,
) -> String {
    if let Some(snap) = snapshot {
        if let Some(loc) = snap.search_location.as_ref().filter(|s| !s.trim().is_empty()) {
            return loc.clone();
        }
        if let Some(trip) = snap.trips.first() {
            return trip.title.clone();
        }
    }
    if let Ok(url) = std::env::var("GOOGLE_CALENDAR_ICS_URL") {
        if let Ok(ics) = fetch_ics(&url).await {
            if let Ok(trips) = trips_for_next_week(&ics, as_of) {
                if let Some(trip) = trips.first() {
                    return trip.title.clone();
                }
            }
        }
    }
    String::new()
}

async fn index(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DashboardQuery>,
) -> Response {
    match render_dashboard(&state, &query).await {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("Error rendering dashboard: {}", e);
            Html(format!("<h1>Error</h1><p>{}</p>", e)).into_response()
        }
    }
}

async fn render_dashboard(state: &AppState, query: &DashboardQuery) -> Result<String> {
    let as_of = Local::now().date_naive();
    let snapshot = load_current_snapshot(&state.cache_root, as_of)?;
    let contacts = contacts_snapshot(&state);
    let (tasks, reconnects) = fetch_due(&contacts);
    let birthdays = birthday_views(upcoming_birthdays_from_contacts(
        &contacts,
        as_of,
        BIRTHDAY_WINDOW_DAYS,
    ));
    let random_week = fetch_random_picks(&contacts, &state.cache_root)?;
    let random_picks = random_pick_views(random_week.picks);
    let random_pick_count = random_picks.len();
    let random_can_shuffle = random_week.eligible_count >= RANDOM_PICK_COUNT;
    let random_fewer_than_target = random_pick_count < RANDOM_PICK_COUNT;
    let travel_just_refreshed = query.refreshed.as_deref() == Some("1");
    let reconnect_snoozed = query.snoozed.as_deref() == Some("1");
    let task_completed = query.task_completed.as_deref() == Some("1");
    let random_just_shuffled = query.random_shuffled.as_deref() == Some("1");
    let random_category_saved = query.random_category_saved.as_deref() == Some("1");

    let mut context = TeraContext::new();
    context.insert("nav_active", "dashboard");
    context.insert("date", &Local::now().format("%B %d, %Y").to_string());
    context.insert("tasks", &tasks);
    context.insert("reconnects", &reconnects);
    context.insert("task_count", &tasks.len());
    context.insert("reconnect_count", &reconnects.len());
    context.insert("reconnect_window_days", &RECONNECT_WINDOW_DAYS);
    context.insert("reconnect_snoozed", &reconnect_snoozed);
    context.insert("task_completed", &task_completed);
    context.insert("random_picks", &random_picks);
    context.insert("random_pick_count", &random_pick_count);
    context.insert("random_week_label", &random_week.week_label);
    context.insert("random_eligible_count", &random_week.eligible_count);
    context.insert("random_pick_target", &RANDOM_PICK_COUNT);
    context.insert("random_can_shuffle", &random_can_shuffle);
    context.insert("random_fewer_than_target", &random_fewer_than_target);
    context.insert("random_shuffled", &random_week.shuffled);
    context.insert("random_just_shuffled", &random_just_shuffled);
    context.insert("random_category_saved", &random_category_saved);
    context.insert("random_pick_category_options", &random_pick_category_options());
    context.insert("birthdays", &birthdays);
    context.insert("birthday_count", &birthdays.len());
    context.insert("birthday_window_days", &BIRTHDAY_WINDOW_DAYS);
    let travel_search_location =
        default_travel_search_location(as_of, snapshot.as_ref()).await;
    context.insert("travel_search_location", &travel_search_location);
    insert_travel_context(&mut context, snapshot.as_ref());
    context.insert("travel_snoozed", &reconnect_snoozed);
    context.insert("travel_just_refreshed", &travel_just_refreshed);
    context.insert(
        "travel_refresh_error",
        &query.travel_error.as_deref().is_some(),
    );
    let geo_just_finished = query.geocoded.as_deref() == Some("1");
    context.insert("geo_just_finished", &geo_just_finished);
    context.insert("geo_refresh_error", &query.geo_error.as_deref().is_some());
    context.insert(
        "geo_geocoded_count",
        &query
            .geo_count
            .as_deref()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0),
    );
    context.insert(
        "geo_failed_count",
        &query
            .geo_failed
            .as_deref()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0),
    );
    let (geo_with_coords, geo_with_address) = {
        let contacts = contacts_snapshot(state);
        pepper_crm::geo_coverage(contacts.as_ref())
    };
    context.insert("geo_with_coords", &geo_with_coords);
    context.insert("geo_with_address", &geo_with_address);

    Ok(state.tera.render("dashboard.html", &context)?)
}

async fn reload_contact_after_vcf_edit(state: &AppState, uid: &str) {
    let as_of = Local::now().date_naive();
    if let Err(e) = remove_contact_from_current_snapshot(&state.cache_root, as_of, uid) {
        tracing::warn!("Could not update travel snapshot after VCF edit: {}", e);
    }
    if let Err(e) = reload_contacts_cache(state).await {
        tracing::warn!("VCF reload after edit failed: {}", e);
    }
}

async fn apply_random_pick_category(state: &AppState, form: &TravelSnoozeForm) -> Result<(), String> {
    let choice = form.reconnect.trim();
    if choice.is_empty() {
        return Err("Choose a reconnect interval or Do Not Engage.".into());
    }
    if !RECONNECT_SNOOZE_OPTIONS.contains(&choice) {
        return Err("Invalid category option.".into());
    }

    let as_of = Local::now().date_naive();
    let contact = contact_by_uid(state, &form.uid)?;

    set_random_pick_category(&contact, choice, as_of)
        .map_err(|e| format!("Failed to update contact: {e}"))?;

    dismiss_random_pick(&state.cache_root, as_of, &form.uid)
        .map_err(|e| format!("Failed to dismiss random pick: {e}"))?;

    reload_contact_after_vcf_edit(state, &form.uid).await;

    Ok(())
}

async fn random_category(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::Form(form): axum::Form<TravelSnoozeForm>,
) -> Response {
    let wants_json = snooze_wants_json(&headers);
    let uid = form.uid.clone();

    match apply_random_pick_category(&state, &form).await {
        Ok(()) => {
            if wants_json {
                Json(SnoozeJsonResponse {
                    ok: true,
                    uid,
                    error: None,
                })
                .into_response()
            } else {
                Redirect::to("/?random_category_saved=1").into_response()
            }
        }
        Err(message) => {
            tracing::error!("Random pick category failed for {}: {}", uid, message);
            if wants_json {
                (
                    StatusCode::BAD_REQUEST,
                    Json(SnoozeJsonResponse {
                        ok: false,
                        uid,
                        error: Some(message),
                    }),
                )
                    .into_response()
            } else {
                Redirect::to("/?travel_error=1").into_response()
            }
        }
    }
}

async fn apply_task_complete(state: &AppState, form: &TaskCompleteForm) -> Result<(), String> {
    let description = form.description.trim();
    if description.is_empty() {
        return Err("Missing task description.".into());
    }

    let contact = contact_by_uid(state, &form.uid)?;

    complete_task(&contact, description).map_err(|e| format!("Failed to update contact: {e}"))?;

    reload_contact_after_vcf_edit(state, &form.uid).await;

    Ok(())
}

async fn task_complete(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::Form(form): axum::Form<TaskCompleteForm>,
) -> Response {
    let wants_json = snooze_wants_json(&headers);
    let uid = form.uid.clone();
    let description = form.description.clone();

    match apply_task_complete(&state, &form).await {
        Ok(()) => {
            if wants_json {
                Json(SnoozeJsonResponse {
                    ok: true,
                    uid,
                    error: None,
                })
                .into_response()
            } else {
                Redirect::to("/?task_completed=1").into_response()
            }
        }
        Err(message) => {
            tracing::error!(
                "Task complete failed for {} ({}): {}",
                uid,
                description,
                message
            );
            if wants_json {
                Json(SnoozeJsonResponse {
                    ok: false,
                    uid,
                    error: Some(message),
                })
                .into_response()
            } else {
                Redirect::to("/?travel_error=1").into_response()
            }
        }
    }
}

async fn apply_travel_snooze(state: &AppState, form: &TravelSnoozeForm) -> Result<(), String> {
    let reconnect = form.reconnect.trim();
    if reconnect.is_empty() {
        return Err("Choose a reconnect interval or Do Not Engage.".into());
    }
    if !RECONNECT_SNOOZE_OPTIONS.contains(&reconnect) {
        return Err("Invalid reconnect interval.".into());
    }

    let as_of = Local::now().date_naive();
    let contact = contact_by_uid(state, &form.uid)?;

    set_random_pick_category(&contact, reconnect, as_of)
        .map_err(|e| format!("Failed to update contact: {e}"))?;

    reload_contact_after_vcf_edit(state, &form.uid).await;

    Ok(())
}

async fn travel_snooze(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::Form(form): axum::Form<TravelSnoozeForm>,
) -> Response {
    let wants_json = snooze_wants_json(&headers);
    let uid = form.uid.clone();

    match apply_travel_snooze(&state, &form).await {
        Ok(()) => {
            if wants_json {
                Json(SnoozeJsonResponse {
                    ok: true,
                    uid,
                    error: None,
                })
                .into_response()
            } else {
                Redirect::to("/?snoozed=1").into_response()
            }
        }
        Err(message) => {
            tracing::error!("Snooze failed for {}: {}", uid, message);
            if wants_json {
                (
                    StatusCode::BAD_REQUEST,
                    Json(SnoozeJsonResponse {
                        ok: false,
                        uid,
                        error: Some(message),
                    }),
                )
                    .into_response()
            } else {
                Redirect::to("/?travel_error=1").into_response()
            }
        }
    }
}

async fn random_shuffle(State(state): State<Arc<AppState>>) -> Response {
    let as_of = Local::now().date_naive();
    let contacts = contacts_snapshot(&state);
    let current_uids = match resolve_random_picks(
        contacts.as_ref(),
        &state.cache_root,
        as_of,
        RANDOM_PICK_COUNT,
    ) {
        Ok(week) => week.picks.into_iter().map(|p| p.uid).collect(),
        Err(_) => Vec::new(),
    };
    match shuffle_and_save(
        contacts.as_ref(),
        &state.cache_root,
        as_of,
        RANDOM_PICK_COUNT,
        &current_uids,
    ) {
        Ok(week) => {
            info!(
                "Random picks shuffled: {:?}",
                week.picks
                    .iter()
                    .map(|p| p.full_name.as_str())
                    .collect::<Vec<_>>()
            );
            Redirect::to("/?random_shuffled=1").into_response()
        }
        Err(e) => {
            tracing::error!("Random shuffle failed: {}", e);
            Redirect::to("/?random_shuffle_error=1").into_response()
        }
    }
}

fn geo_write_to_vcf_enabled() -> bool {
    std::env::var("GEO_WRITE_TO_VCF")
        .map(|s| {
            let lower = s.to_lowercase();
            lower != "0" && lower != "false" && lower != "no"
        })
        .unwrap_or(true)
}

async fn contacts_geocode(State(state): State<Arc<AppState>>) -> Response {
    let contacts_dir = state.contacts_dir.clone();
    let cache_root = state.cache_root.clone();
    let write_back = geo_write_to_vcf_enabled();

    info!(
        write_back,
        "Starting contact GEO pass (Nominatim ~1 req/sec; large exports take 15+ minutes)..."
    );

    match ensure_contacts_geocoded_in_dir(&contacts_dir, &cache_root, write_back).await {
        Ok((stats, contacts)) => {
            *state.contacts.write().expect("contacts lock poisoned") = Arc::new(contacts);
            info!(
                geocoded = stats.geocoded,
                already_ok = stats.already_ok,
                failed = stats.failed,
                failed_cached = stats.failed_cached,
                skipped = stats.skipped_no_address,
                "Contact GEO pass finished"
            );
            Redirect::to(&format!(
                "/?geocoded=1&geo_count={}&geo_failed={}",
                stats.geocoded, stats.failed
            ))
            .into_response()
        }
        Err(e) => {
            tracing::error!("Contact GEO pass failed: {}", e);
            Redirect::to("/?geo_error=1").into_response()
        }
    }
}

async fn travel_refresh(
    State(state): State<Arc<AppState>>,
    axum::Form(form): axum::Form<TravelRefreshForm>,
) -> Response {
    if let Err(e) = reload_contacts_cache(&state).await {
        tracing::error!("VCF reload before travel refresh failed: {}", e);
    }

    let metro_radius_mi = clamp_metro_radius_mi(form.metro_radius_mi);
    let search_location = form.search_location.trim();
    let mut config = TravelBuildConfig::from_env(Local::now().date_naive());
    config.contacts_dir = state.contacts_dir.clone();
    config.cache_root = state.cache_root.clone();
    config.force = true;
    config.ensure_contact_geo = false;
    config.metro_radius_km = miles_to_km(metro_radius_mi as f64);
    config.search_location = if search_location.is_empty() {
        None
    } else {
        Some(search_location.to_string())
    };

    info!(
        metro_radius_mi,
        search_location = config.search_location.as_deref().unwrap_or("(all calendar trips)"),
        "Starting travel match build (calendar + cached contact GEO only; full geocode runs on weekly scan)..."
    );

    match build_travel_week_snapshot(&config).await {
        Ok(snap) => {
            info!(
                "Travel snapshot built: {} trips, {} matches (radius {} mi)",
                snap.trips.len(),
                snap.match_count(),
                metro_radius_mi
            );
            Redirect::to("/?refreshed=1").into_response()
        }
        Err(e) => {
            tracing::error!("Travel build failed: {}", e);
            Redirect::to("/?travel_error=1").into_response()
        }
    }
}

async fn digest_preview(State(state): State<Arc<AppState>>) -> Response {
    let mut context = TeraContext::new();
    context.insert("nav_active", "preview");

    match state.tera.render("preview.html", &context) {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("Error rendering digest preview page: {}", e);
            Html(format!("<h1>Error</h1><p>{}</p>", e)).into_response()
        }
    }
}

async fn digest_preview_email(State(state): State<Arc<AppState>>) -> Response {
    match render_digest_email_html(&state).await {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("Error rendering digest email preview: {}", e);
            Html(format!("<h1>Error</h1><p>{}</p>", e)).into_response()
        }
    }
}

async fn render_digest_email_html(state: &AppState) -> Result<String> {
    let contacts = contacts_snapshot(state);
    let as_of = Local::now().date_naive();
    let (task_rows, reconnect_infos) = fetch_due_items(&contacts, as_of);
    let snapshot = load_current_snapshot(&state.cache_root, as_of)?;
    let digest_input = build_digest_input_from_due(
        &task_rows,
        &reconnect_infos,
        contacts.as_ref(),
        snapshot.as_ref(),
        &state.cache_root,
        as_of,
    )?;
    Ok(render_digest_email(&digest_input)?.html)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    load_dotenv()?;

    info!("Starting Pepper Web Dashboard...");

    let contacts_dir = std::env::var("CONTACTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./contacts"));
    let cache_root = std::env::var("CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".cache"));

    let contacts = load_contacts_from_vcf(&contacts_dir).await?;

    let mut tera = Tera::new("pepper-web/templates/**/*.html")?;
    tera.autoescape_on(vec!["html"]);
    info!("Loaded templates");

    let state = Arc::new(AppState {
        tera,
        cache_root,
        contacts_dir,
        contacts: Arc::new(RwLock::new(contacts)),
    });

    let app = Router::new()
        .route("/", get(index))
        .route("/travel/refresh", post(travel_refresh))
        .route("/contacts/geocode", post(contacts_geocode))
        .route("/travel/snooze", post(travel_snooze))
        .route("/tasks/complete", post(task_complete))
        .route("/random/shuffle", post(random_shuffle))
        .route("/random/category", post(random_category))
        .route("/preview", get(digest_preview))
        .route("/preview/email", get(digest_preview_email))
        .nest_service("/static", ServeDir::new("pepper-web/static"))
        .nest_service("/assets", ServeDir::new("assets"))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = "127.0.0.1:3000";
    info!("Server running at http://{}", addr);
    info!("   Dashboard:       http://{}/", addr);
    info!("   Digest Preview:  http://{}/preview", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
