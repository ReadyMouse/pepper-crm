//! # MCP vCard Server
//!
//!   MCP server (stdio) for reading contacts and appending CRM interaction logs.
//!
//! INPUT:
//!   - Env: optional `CONTACTS_DIR`; when `CARDDAV_*` is set, contacts load/write via CardDAV
//!   - MCP tool `parse_vcards`: `{ "directory"?: "<path>" }` — local `.vcf` dir (ignored for reads when CardDAV configured)
//!   - MCP tool `log_interaction`: `{ "uid", "note", "new_reconnect_tag"?, "contacts_dir"? }`
//!
//! OUTPUT:
//!   - `parse_vcards` → `[ContactSummary, ...]` with uid, full_name, email, phone, org,
//!     city, state, country, categories, note_raw, todos, reconnect_tag, vcf_path, carddav_href?
//!   - `log_interaction` → confirmation string (e.g. `"Logged interaction to <name>"`)
//!
//! NOTES:
//!   - Server name: `mcp-vcard-server`
//!   - `log_interaction` writes append-only CRM log (local file or CardDAV PUT)
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use pepper_crm::{
    contacts_use_carddav, find_contact_by_uid, load_dotenv, log_interaction, parse_contacts,
    Contact,
};
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::{Json, Parameters}},
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::stdio,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

fn default_contacts_dir() -> String {
    std::env::var("CONTACTS_DIR").unwrap_or_else(|_| "./contacts".to_string())
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
struct ContactSummary {
    uid: String,
    full_name: String,
    email: Option<String>,
    phone: Option<String>,
    org: Option<String>,
    city: Option<String>,
    state: Option<String>,
    country: Option<String>,
    categories: Vec<String>,
    note_raw: String,
    todos: Vec<String>,
    reconnect_tag: Option<String>,
    vcf_path: String,
    carddav_href: Option<String>,
}

impl From<Contact> for ContactSummary {
    fn from(c: Contact) -> Self {
        Self {
            uid: c.uid,
            full_name: c.full_name,
            email: c.email,
            phone: c.phone,
            org: c.org,
            city: c.city,
            state: c.state,
            country: c.country,
            categories: c.categories,
            note_raw: c.note_raw,
            todos: c.todos,
            reconnect_tag: c.reconnect_tag,
            vcf_path: c.vcf_path.display().to_string(),
            carddav_href: c.carddav_href,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ParseVcardsArgs {
    #[serde(default = "default_contacts_dir")]
    directory: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct LogInteractionArgs {
    uid: String,
    note: String,
    new_reconnect_tag: Option<String>,
    #[serde(default = "default_contacts_dir")]
    contacts_dir: String,
}

#[derive(Clone)]
struct VcardServer {
    tool_router: ToolRouter<Self>,
}

impl VcardServer {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl VcardServer {
    #[tool(description = "Parse all contacts from CONTACTS_DIR or CardDAV when CARDDAV_* is set")]
    fn parse_vcards(
        &self,
        Parameters(args): Parameters<ParseVcardsArgs>,
    ) -> Result<Json<Vec<ContactSummary>>, String> {
        let dir = PathBuf::from(&args.directory);
        if contacts_use_carddav() {
            info!("Parsing contacts from CardDAV (CONTACTS_DIR ignored for reads)");
        } else {
            info!("Parsing VCF files from: {}", dir.display());
        }

        let contacts = parse_contacts(&dir).map_err(|e| e.to_string())?;
        info!("Parsed {} contacts", contacts.len());
        Ok(Json(
            contacts.into_iter().map(ContactSummary::from).collect(),
        ))
    }

    #[tool(
        description = "Log an interaction to a contact (append-only CRM log; local VCF or CardDAV PUT)"
    )]
    fn log_interaction(
        &self,
        Parameters(args): Parameters<LogInteractionArgs>,
    ) -> Result<String, String> {
        info!("Logging interaction for uid: {}", args.uid);

        let dir = PathBuf::from(&args.contacts_dir);
        let contact = find_contact_by_uid(&dir, &args.uid).map_err(|e| e.to_string())?;

        log_interaction(
            &contact,
            &args.note,
            args.new_reconnect_tag.as_deref(),
        )
        .map_err(|e| e.to_string())?;

        Ok(format!("Logged interaction to {}", contact.full_name))
    }
}

#[tool_handler]
impl ServerHandler for VcardServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Read and update Pepper CRM contacts from local VCF files or CardDAV (Radicale on Pi). \
                 Set CARDDAV_URL, CARDDAV_USER, and CARDDAV_PASS for production."
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

    info!("Starting mcp-vcard-server");
    let service = VcardServer::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
