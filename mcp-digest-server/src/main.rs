//! # MCP Digest Server
//!
//!   MCP server (stdio) that renders an HTML email digest from pending tasks, due reconnects,
//!   travel matches, birthdays, and random people picks using Tera templates.
//!
//! INPUT:
//!   - MCP tool `render_digest`: full `DigestInput` JSON (see pepper-crm::digest)
//!
//! OUTPUT:
//!   - `render_digest` → `{ "html": "<html...>", "subject": "<subject line>" }`
//!
//! NOTES:
//!   - Server name: `mcp-digest-server`
//!   - Template: `templates/digest.html`
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use pepper_crm::{render_digest_email, DigestInput, DigestOutput};
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::{Json, Parameters}},
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::stdio,
};
use tracing::info;

#[derive(Clone)]
struct DigestServer {
    tool_router: ToolRouter<Self>,
}

impl DigestServer {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl DigestServer {
    #[tool(
        description = "Render HTML email digest from tasks, reconnects, travel, birthdays, and random picks"
    )]
    fn render_digest(
        &self,
        Parameters(input): Parameters<DigestInput>,
    ) -> Result<Json<DigestOutput>, String> {
        info!(
            "Rendering digest with {} tasks, {} reconnects, {} travel matches, {} birthdays, {} random picks",
            input.task_count(),
            input.reconnect_count(),
            input.travel_match_count,
            input.birthday_count(),
            input.random_pick_count()
        );

        render_digest_email(&input).map(Json).map_err(|e| e.to_string())
    }
}

#[tool_handler]
impl ServerHandler for DigestServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Render the Pepper weekly CRM digest email from a DigestInput payload.".into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    info!("Starting mcp-digest-server");
    let service = DigestServer::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
