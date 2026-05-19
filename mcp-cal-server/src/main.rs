//! # MCP Calendar Export Server
//!
//!   MCP server (stdio) that generates iCalendar (.ics) attachments for due reconnects.
//!
//! INPUT:
//!   - MCP tool `export_ics`: `{ "reconnects": [ReconnectItem, ...] }`
//!     — ReconnectItem: contact_name, contact_email?, due_date, tag
//!
//! OUTPUT:
//!   - `export_ics` → `[{ "filename": "reconnect_<name>.ics", "content": "<ics>" }, ...]`
//!     — one .ics file per reconnect with calendar event and alarm
//!
//! NOTES:
//!   - Server name: `mcp-cal-server`
//!   - Filenames derive from contact_name (spaces → underscores, lowercased)
//!   - Output lacks `content_type`; mailer callers should add `text/calendar` per attachment
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use anyhow::Result;
use pepper_crm::build_ics;
use rmcp::*;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Deserialize)]
struct ExportIcsArgs {
    reconnects: Vec<ReconnectItem>,
}

#[derive(Debug, Deserialize)]
struct ReconnectItem {
    contact_name: String,
    contact_email: Option<String>,
    due_date: String,
    tag: String,
}

#[derive(Debug, Serialize)]
struct IcsFile {
    filename: String,
    content: String,
}

async fn handle_export_ics(args: ExportIcsArgs) -> Result<Vec<IcsFile>> {
    info!("Exporting {} reconnects to .ics files", args.reconnects.len());
    
    let mut ics_files = Vec::new();
    
    for reconnect in args.reconnects {
        let ics_content = build_ics(
            &reconnect.contact_name,
            reconnect.contact_email.as_deref(),
            &reconnect.due_date,
            &reconnect.tag,
        )?;
        
        let filename = format!(
            "reconnect_{}.ics",
            reconnect.contact_name.replace(' ', "_").to_lowercase()
        );
        
        ics_files.push(IcsFile {
            filename,
            content: ics_content,
        });
    }
    
    info!("Generated {} .ics files", ics_files.len());
    
    Ok(ics_files)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    info!("Starting mcp-cal-server");
    
    let server = Server::new("mcp-cal-server")
        .with_tool(
            "export_ics",
            "Export reconnects as iCalendar (.ics) files with alarms",
            |args: ExportIcsArgs| async move {
                handle_export_ics(args).await
            },
        );
    
    server.run_stdio().await?;
    
    Ok(())
}
