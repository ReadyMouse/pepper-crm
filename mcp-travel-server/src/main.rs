//! # MCP Travel Match Server
//!
//!   MCP server (stdio) that builds and retrieves weekly travel snapshots: upcoming trips
//!   matched to nearby contacts via geocoding (VCF/CardDAV + Google Calendar ICS).
//!
//! INPUT:
//!   - Env: `GOOGLE_CALENDAR_ICS_URL`, `CONTACTS_DIR`, `CACHE_DIR`, optional geocode settings
//!   - MCP tool `build_travel_week`: `{ "force"?: bool }`
//!   - MCP tool `get_travel_week`: `{ "week_id"?: "<id>" }`
//!
//! OUTPUT:
//!   - Both tools return `TravelWeekSnapshot` JSON (or null for missing cache on get)
//!
//! NOTES:
//!   - Server name: `mcp-travel-server`
//!   - Uses CardDAV when `CARDDAV_*` is set (same as pepper-web)
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use chrono::Local;
use pepper_crm::{
    build_travel_week_snapshot, load_current_snapshot, load_dotenv, load_snapshot,
    TravelBuildConfig, TravelWeekSnapshot,
};
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::{Json, Parameters}},
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::stdio,
};
use schemars::JsonSchema;
use serde::Deserialize;
use tracing::info;

#[derive(Debug, Deserialize, JsonSchema)]
struct BuildTravelWeekArgs {
    force: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GetTravelWeekArgs {
    week_id: Option<String>,
}

#[derive(Clone)]
struct TravelServer {
    tool_router: ToolRouter<Self>,
}

impl TravelServer {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl TravelServer {
    #[tool(description = "Build and cache the travel match list for next week")]
    async fn build_travel_week(
        &self,
        Parameters(args): Parameters<BuildTravelWeekArgs>,
    ) -> Result<Json<TravelWeekSnapshot>, String> {
        let mut config = TravelBuildConfig::from_env(Local::now().date_naive());
        config.force = args.force.unwrap_or(false);
        info!("Building travel week snapshot (force={})", config.force);
        build_travel_week_snapshot(&config)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[tool(description = "Load the cached travel snapshot for next week (or week_id)")]
    async fn get_travel_week(
        &self,
        Parameters(args): Parameters<GetTravelWeekArgs>,
    ) -> Result<Json<Option<TravelWeekSnapshot>>, String> {
        let as_of = Local::now().date_naive();
        let cache_root = std::env::var("CACHE_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from(".cache"));

        let snapshot = if let Some(week_id) = args.week_id {
            load_snapshot(&cache_root, &week_id).map_err(|e| e.to_string())?
        } else {
            load_current_snapshot(&cache_root, as_of).map_err(|e| e.to_string())?
        };

        Ok(Json(snapshot))
    }
}

#[tool_handler]
impl ServerHandler for TravelServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Build and load Pepper CRM travel match snapshots (contacts + calendar + geo)."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    load_dotenv()?;

    info!("Starting mcp-travel-server");
    let service = TravelServer::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
