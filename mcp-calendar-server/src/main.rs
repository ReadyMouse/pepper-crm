use anyhow::{Context, Result};
use chrono::Local;
use pepper_crm::{fetch_ics, trips_for_next_week};
use rmcp::*;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Deserialize)]
struct GetUpcomingTravelArgs {
    as_of: Option<String>,
}

#[derive(Debug, Serialize)]
struct TravelTripOut {
    title: String,
    start: String,
    end: String,
}

async fn handle_get_upcoming_travel(args: GetUpcomingTravelArgs) -> Result<Vec<TravelTripOut>> {
    let as_of = if let Some(d) = args.as_of {
        chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d")?
    } else {
        Local::now().date_naive()
    };

    let url = std::env::var("GOOGLE_CALENDAR_ICS_URL")
        .context("GOOGLE_CALENDAR_ICS_URL must be set")?;
    info!("Fetching calendar ICS for travel (as_of={})", as_of);
    let ics = fetch_ics(&url).await?;
    let trips = trips_for_next_week(&ics, as_of)?;

    Ok(trips
        .into_iter()
        .map(|t| TravelTripOut {
            title: t.title,
            start: t.start.to_string(),
            end: t.end.to_string(),
        })
        .collect())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();
    info!("Starting mcp-calendar-server");

    let server = Server::new("mcp-calendar-server").with_tool(
        "get_upcoming_travel",
        "Fetch Google Calendar ICS and return travel trips for next week (SUMMARY = destination)",
        |args: GetUpcomingTravelArgs| async move { handle_get_upcoming_travel(args).await },
    );

    server.run_stdio().await?;
    Ok(())
}
