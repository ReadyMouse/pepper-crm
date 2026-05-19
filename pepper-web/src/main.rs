//! Pepper Web Dashboard Server
//!
//!   Axum HTTP server for visualizing Pepper CRM data: tasks, reconnects, and travel matches.
//!
//! INPUT: DATABASE_URL, CONTACTS_DIR, CACHE_DIR, GOOGLE_CALENDAR_ICS_URL (optional); VCF files; PostgreSQL.
//! OUTPUT: Routes `/` (dashboard), `/preview` (digest preview), `/travel/refresh`, `/travel/snooze`; static assets.
//! NOTES: Syncs VCF → PostgreSQL on startup; travel snapshot built on demand, not every page load.
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
    build_travel_week_snapshot, due_reconnects_from_contacts, find_contact_by_uid, get_due_tasks,
    km_to_miles, load_current_snapshot, miles_to_km, parse_vcards_from_dir,
    remove_contact_from_current_snapshot, set_reconnect_snooze, upsert_contacts_batch, DueReconnectInfo,
    MatchReason, TravelBuildConfig, TravelWeekSnapshot, DEFAULT_METRO_RADIUS_MI,
    RECONNECT_SNOOZE_OPTIONS,
};
use serde::Serialize;
use sqlx::PgPool;
use std::{path::PathBuf, sync::Arc};
use tera::{Context as TeraContext, Tera};
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing::info;

const RECONNECT_WINDOW_DAYS: u32 = 7;

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    tera: Tera,
    cache_root: PathBuf,
    contacts_dir: PathBuf,
}

#[derive(Serialize)]
struct TaskView {
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
struct ReconnectSnoozeOption {
    value: String,
    label: String,
}

#[derive(Debug, Deserialize)]
struct DashboardQuery {
    refreshed: Option<String>,
    travel_error: Option<String>,
    snoozed: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TravelRefreshForm {
    #[serde(default = "default_form_metro_radius_mi")]
    metro_radius_mi: u32,
}

#[derive(Debug, Deserialize)]
struct TravelSnoozeForm {
    uid: String,
    reconnect: String,
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

fn task_views(tasks: Vec<pepper_crm::TaskRow>) -> Vec<TaskView> {
    tasks
        .into_iter()
        .map(|t| TaskView {
            contact_name: t.full_name,
            description: t.body,
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

async fn sync_contacts_from_vcf(pool: &PgPool, contacts_dir: &PathBuf) -> Result<()> {
    if !contacts_dir.exists() {
        info!(
            "Contacts directory {} not found, skipping VCF sync",
            contacts_dir.display()
        );
        return Ok(());
    }

    let contacts = parse_vcards_from_dir(contacts_dir)
        .with_context(|| format!("Failed to parse VCF files in {}", contacts_dir.display()))?;

    if contacts.is_empty() {
        info!("No VCF contacts found in {}", contacts_dir.display());
        return Ok(());
    }

    let result = upsert_contacts_batch(pool, &contacts).await?;
    info!(
        "Synced {} contacts ({} tasks, {} reconnects) from {}",
        result.contacts_upserted,
        result.tasks_created,
        result.reconnects_created,
        contacts_dir.display()
    );

    Ok(())
}

async fn fetch_due(
    pool: &PgPool,
    contacts_dir: &PathBuf,
) -> Result<(Vec<TaskView>, Vec<ReconnectView>)> {
    let today = Local::now().date_naive();
    let tasks = get_due_tasks(pool, today).await?;

    let reconnects = if contacts_dir.exists() {
        let contacts = parse_vcards_from_dir(contacts_dir)
            .with_context(|| format!("Failed to parse VCF in {}", contacts_dir.display()))?;
        reconnect_views(due_reconnects_from_contacts(
            &contacts,
            today,
            RECONNECT_WINDOW_DAYS,
        ))
    } else {
        Vec::new()
    };

    Ok((task_views(tasks), reconnects))
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
        }
        None => {
            context.insert("travel_ready", &false);
            context.insert("travel_trips", &Vec::<TravelTripView>::new());
            context.insert("travel_match_count", &0usize);
            context.insert("travel_built_at", &String::new());
            context.insert("travel_week_label", &String::new());
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
    let snooze_options: Vec<ReconnectSnoozeOption> = RECONNECT_SNOOZE_OPTIONS
        .iter()
        .map(|v| ReconnectSnoozeOption {
            label: format!("Reconnect: {v}"),
            value: (*v).to_string(),
        })
        .collect();
    context.insert("reconnect_snooze_options", &snooze_options);
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
    let (tasks, reconnects) = fetch_due(&state.pool, &state.contacts_dir).await?;
    let travel_just_refreshed = query.refreshed.as_deref() == Some("1");
    let reconnect_snoozed = query.snoozed.as_deref() == Some("1");

    let mut context = TeraContext::new();
    context.insert("nav_active", "dashboard");
    context.insert("date", &Local::now().format("%B %d, %Y").to_string());
    context.insert("tasks", &tasks);
    context.insert("reconnects", &reconnects);
    context.insert("task_count", &tasks.len());
    context.insert("reconnect_count", &reconnects.len());
    context.insert("reconnect_window_days", &RECONNECT_WINDOW_DAYS);
    context.insert("reconnect_snoozed", &reconnect_snoozed);
    insert_travel_context(&mut context, snapshot.as_ref());
    context.insert("travel_snoozed", &reconnect_snoozed);
    context.insert("travel_just_refreshed", &travel_just_refreshed);
    context.insert(
        "travel_refresh_error",
        &query.travel_error.as_deref().is_some(),
    );

    Ok(state.tera.render("dashboard.html", &context)?)
}

async fn apply_travel_snooze(state: &AppState, form: &TravelSnoozeForm) -> Result<(), String> {
    let reconnect = form.reconnect.trim();
    if reconnect.is_empty() {
        return Err("Choose a reconnect interval.".into());
    }
    if !RECONNECT_SNOOZE_OPTIONS.contains(&reconnect) {
        return Err("Invalid reconnect interval.".into());
    }

    let as_of = Local::now().date_naive();
    let contact = find_contact_by_uid(&state.contacts_dir, &form.uid)
        .map_err(|e| format!("Contact not found: {e}"))?;

    set_reconnect_snooze(&contact, reconnect, as_of)
        .map_err(|e| format!("Failed to update contact: {e}"))?;

    if let Err(e) = remove_contact_from_current_snapshot(&state.cache_root, as_of, &form.uid) {
        tracing::warn!("Could not update travel snapshot after snooze: {}", e);
    }
    if let Err(e) = sync_contacts_from_vcf(&state.pool, &state.contacts_dir).await {
        tracing::warn!("VCF sync after snooze failed: {}", e);
    }

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

async fn travel_refresh(
    State(state): State<Arc<AppState>>,
    axum::Form(form): axum::Form<TravelRefreshForm>,
) -> Response {
    if let Err(e) = sync_contacts_from_vcf(&state.pool, &state.contacts_dir).await {
        tracing::error!("VCF sync before travel refresh failed: {}", e);
    }

    let metro_radius_mi = clamp_metro_radius_mi(form.metro_radius_mi);
    let mut config = TravelBuildConfig::from_env(Local::now().date_naive());
    config.contacts_dir = state.contacts_dir.clone();
    config.cache_root = state.cache_root.clone();
    config.force = true;
    config.metro_radius_km = miles_to_km(metro_radius_mi as f64);

    info!(
        metro_radius_mi,
        "Starting travel match build (fetch calendar + geocode contacts; may take several minutes for large exports)..."
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
    match render_digest_preview(&state).await {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("Error rendering digest preview: {}", e);
            Html(format!("<h1>Error</h1><p>{}</p>", e)).into_response()
        }
    }
}

async fn render_digest_preview(state: &AppState) -> Result<String> {
    let (tasks, reconnects) = fetch_due(&state.pool, &state.contacts_dir).await?;

    let mut context = TeraContext::new();
    context.insert("nav_active", "preview");
    context.insert("tasks", &tasks);
    context.insert("reconnects", &reconnects);
    context.insert("date", &Local::now().format("%B %d, %Y").to_string());
    context.insert("task_count", &tasks.len());
    context.insert("reconnect_count", &reconnects.len());

    Ok(state.tera.render("preview.html", &context)?)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    info!("Starting Pepper Web Dashboard...");

    let database_url =
        std::env::var("DATABASE_URL").context("DATABASE_URL must be set in .env")?;

    let pool = PgPool::connect(&database_url).await?;
    info!("Connected to database");

    let contacts_dir = std::env::var("CONTACTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./contacts"));
    let cache_root = std::env::var("CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".cache"));

    sync_contacts_from_vcf(&pool, &contacts_dir).await?;

    let mut tera = Tera::new("pepper-web/templates/**/*.html")?;
    tera.autoescape_on(vec!["html"]);
    info!("Loaded templates");

    let state = Arc::new(AppState {
        pool,
        tera,
        cache_root,
        contacts_dir,
    });

    let app = Router::new()
        .route("/", get(index))
        .route("/travel/refresh", post(travel_refresh))
        .route("/travel/snooze", post(travel_snooze))
        .route("/preview", get(digest_preview))
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
