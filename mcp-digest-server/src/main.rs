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

use anyhow::Result;
use pepper_crm::{render_digest_email, DigestInput};
use rmcp::*;
use tracing::info;

async fn handle_render_digest(args: DigestInput) -> Result<pepper_crm::DigestOutput> {
    info!(
        "Rendering digest with {} tasks, {} reconnects, {} travel matches, {} birthdays, {} random picks",
        args.task_count(),
        args.reconnect_count(),
        args.travel_match_count,
        args.birthday_count(),
        args.random_pick_count()
    );

    render_digest_email(&args)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("Starting mcp-digest-server");

    let server = Server::new("mcp-digest-server").with_tool(
        "render_digest",
        "Render HTML email digest from tasks, reconnects, travel, birthdays, and random picks",
        |args: DigestInput| async move { handle_render_digest(args).await },
    );

    server.run_stdio().await?;

    Ok(())
}
