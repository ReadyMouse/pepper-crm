//! # MCP Google Calendar Server
//!
//!   MCP server (stdio) that fetches a Google Calendar ICS feed and returns travel
//!   trips scheduled for next week (event SUMMARY = destination).
//!
//! INPUT:
//!   - Env: `GOOGLE_CALENDAR_ICS_URL` (public or private ICS URL)
//!   - MCP tool `get_upcoming_travel`: `{ "as_of"?: "YYYY-MM-DD" }` (defaults to today)
//!
//! OUTPUT:
//!   - `get_upcoming_travel` → `[{ "title", "start", "end" }, ...]` (ISO date strings)
//!
//! NOTES:
//!   - Server name: `mcp-calendar-server`
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use chrono::Local;
use pepper_crm::{fetch_ics, load_dotenv, trips_for_next_week};
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::{Json, Parameters}},
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::stdio,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Deserialize, JsonSchema)]
struct GetUpcomingTravelArgs {
    as_of: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
struct TravelTripOut {
    title: String,
    start: String,
    end: String,
}

#[derive(Clone)]
struct CalendarServer {
    tool_router: ToolRouter<Self>,
}

impl CalendarServer {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl CalendarServer {
    #[tool(
        description = "Fetch Google Calendar ICS and return travel trips for next week (SUMMARY = destination)"
    )]
    async fn get_upcoming_travel(
        &self,
        Parameters(args): Parameters<GetUpcomingTravelArgs>,
    ) -> Result<Json<Vec<TravelTripOut>>, String> {
        let as_of = if let Some(d) = args.as_of {
            chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").map_err(|e| e.to_string())?
        } else {
            Local::now().date_naive()
        };

        let url = std::env::var("GOOGLE_CALENDAR_ICS_URL")
            .map_err(|_| "GOOGLE_CALENDAR_ICS_URL must be set".to_string())?;
        info!("Fetching calendar ICS for travel (as_of={})", as_of);
        let ics = fetch_ics(&url).await.map_err(|e| e.to_string())?;
        let trips = trips_for_next_week(&ics, as_of).map_err(|e| e.to_string())?;

        Ok(Json(
            trips
                .into_iter()
                .map(|t| TravelTripOut {
                    title: t.title,
                    start: t.start.to_string(),
                    end: t.end.to_string(),
                })
                .collect(),
        ))
    }
}

#[tool_handler]
impl ServerHandler for CalendarServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Fetch upcoming travel trips from Google Calendar ICS for Pepper CRM.".into(),
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

    info!("Starting mcp-calendar-server");
    let service = CalendarServer::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
