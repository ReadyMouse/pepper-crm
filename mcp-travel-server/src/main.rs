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
