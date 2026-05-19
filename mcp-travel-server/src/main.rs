//! # MCP Travel Match Server
//!
//!   MCP server (stdio) that builds and retrieves weekly travel snapshots: upcoming trips
//!   matched to nearby contacts via geocoding (VCF + Google Calendar ICS).
//!
//! INPUT:
//!   - Env: `GOOGLE_CALENDAR_ICS_URL`, `CONTACTS_DIR` (default `./contacts`),
//!     `CACHE_DIR` (default `.cache`), optional `METRO_RADIUS_KM` / `METRO_RADIUS_MI`,
//!     `GEO_WRITE_TO_VCF`, `NOMINATIM_USER_AGENT`, `GEOCODE_CACHE_TTL_DAYS`
//!   - MCP tool `build_travel_week`: `{ "force"?: bool }` — rebuild even if cached
//!   - MCP tool `get_travel_week`: `{ "week_id"?: "<id>" }` — omit for current next-week snapshot
//!
//! OUTPUT:
//!   - `build_travel_week` → `TravelWeekSnapshot` (week_id, week_start, week_end, built_at,
//!     metro_radius_km, trips[{ title, start, end, matches[{ uid, full_name, city,
//!     distance_km, reason, reconnect_tag }] }])
//!   - `get_travel_week` → `TravelWeekSnapshot` or `null` if not cached
//!
//! NOTES:
//!   - Server name: `mcp-travel-server`
//!   - Pepper orchestrator calls pepper-crm travel helpers directly instead of this server
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use anyhow::Result;
use chrono::Local;
use pepper_crm::{
    build_travel_week_snapshot, load_current_snapshot, load_snapshot, TravelBuildConfig,
    TravelWeekSnapshot,
};
use rmcp::*;
use serde::Deserialize;
use tracing::info;

#[derive(Debug, Deserialize)]
struct BuildTravelWeekArgs {
    force: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct GetTravelWeekArgs {
    week_id: Option<String>,
}

async fn handle_build_travel_week(args: BuildTravelWeekArgs) -> Result<TravelWeekSnapshot> {
    let mut config = TravelBuildConfig::from_env(Local::now().date_naive());
    config.force = args.force.unwrap_or(false);
    info!("Building travel week snapshot (force={})", config.force);
    build_travel_week_snapshot(&config).await
}

async fn handle_get_travel_week(args: GetTravelWeekArgs) -> Result<Option<TravelWeekSnapshot>> {
    let as_of = Local::now().date_naive();
    let cache_root = std::env::var("CACHE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(".cache"));

    if let Some(week_id) = args.week_id {
        load_snapshot(&cache_root, &week_id)
    } else {
        Ok(load_current_snapshot(&cache_root, as_of)?)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();
    info!("Starting mcp-travel-server");

    let server = Server::new("mcp-travel-server")
        .with_tool(
            "build_travel_week",
            "Build and cache the travel match list for next week (VCF + calendar + geocode)",
            |args: BuildTravelWeekArgs| async move { handle_build_travel_week(args).await },
        )
        .with_tool(
            "get_travel_week",
            "Load the cached travel snapshot for next week (or week_id)",
            |args: GetTravelWeekArgs| async move { handle_get_travel_week(args).await },
        );

    server.run_stdio().await?;
    Ok(())
}
