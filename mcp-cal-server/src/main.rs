//! # MCP Calendar Export Server
//!
//!   MCP server (stdio) that generates iCalendar (.ics) attachments for due reconnects.
//!
//! INPUT:
//!   - MCP tool `export_ics`: `{ "reconnects": [ReconnectItem, ...] }`
//!
//! OUTPUT:
//!   - `export_ics` → `[{ "filename": "reconnect_<uid>.ics", "content": "<ics>" }, ...]`
//!
//! NOTES:
//!   - Server name: `mcp-cal-server`
//!   - Output lacks `content_type`; mailer callers should add `text/calendar` per attachment
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use chrono::NaiveDate;
use pepper_crm::{build_ics_for_due, DueReconnectInfo};
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
struct ExportIcsArgs {
    reconnects: Vec<ReconnectItem>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ReconnectItem {
    contact_uid: String,
    contact_name: String,
    contact_email: Option<String>,
    due_date: String,
    tag: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
struct IcsFileOut {
    filename: String,
    content: String,
}

#[derive(Clone)]
struct CalServer {
    tool_router: ToolRouter<Self>,
}

impl CalServer {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl CalServer {
    #[tool(description = "Export reconnects as iCalendar (.ics) files with alarms")]
    fn export_ics(
        &self,
        Parameters(args): Parameters<ExportIcsArgs>,
    ) -> Result<Json<Vec<IcsFileOut>>, String> {
        info!("Exporting {} reconnects to .ics files", args.reconnects.len());

        let mut ics_files = Vec::new();

        for reconnect in args.reconnects {
            let due_date = NaiveDate::parse_from_str(&reconnect.due_date, "%Y-%m-%d")
                .map_err(|e| e.to_string())?;
            let due = DueReconnectInfo {
                uid: reconnect.contact_uid,
                full_name: reconnect.contact_name,
                due_date,
                tag: reconnect.tag,
            };
            let ics = build_ics_for_due(&due, reconnect.contact_email.as_deref())
                .map_err(|e| e.to_string())?;

            ics_files.push(IcsFileOut {
                filename: ics.filename,
                content: ics.content,
            });
        }

        info!("Generated {} .ics files", ics_files.len());
        Ok(Json(ics_files))
    }
}

#[tool_handler]
impl ServerHandler for CalServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Generate iCalendar (.ics) attachments for Pepper CRM reconnect reminders.".into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    info!("Starting mcp-cal-server");
    let service = CalServer::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
