//! # MCP Scheduler Server
//!
//!   MCP server (stdio) that upserts contacts from VCF summaries into Postgres and
//!   returns pending tasks and due reconnects for the digest pipeline.
//!
//! INPUT:
//!   - Env: `DATABASE_URL` (Postgres connection string)
//!   - MCP tool `upsert_contacts`: `{ "contacts": [ContactInput, ...] }`
//!     — each ContactInput: uid, full_name, email?, phone?, org?, city?, country?,
//!       todos[], reconnect_tag?, vcf_path
//!   - MCP tool `get_due`: `{ "as_of"?: "YYYY-MM-DD" }` (defaults to today)
//!
//! OUTPUT:
//!   - `upsert_contacts` → summary string (e.g. `"Upserted N contacts"`)
//!   - `get_due` → `{ "tasks": [TaskSummary], "reconnects": [ReconnectSummary] }`
//!     — TaskSummary: id, contact_uid, contact_name, description, status
//!     — ReconnectSummary: id, contact_uid, contact_name, due_date, tag, status
//!
//! NOTES:
//!   - Server name: `mcp-scheduler-server`
//!   - Upsert syncs embedded todos and reconnect tags from each contact
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use anyhow::Result;
use chrono::NaiveDate;
use pepper_crm::*;
use rmcp::*;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::info;

#[derive(Debug, Deserialize)]
struct UpsertContactsArgs {
    contacts: Vec<ContactInput>,
}

#[derive(Debug, Deserialize)]
struct ContactInput {
    uid: String,
    full_name: String,
    email: Option<String>,
    phone: Option<String>,
    org: Option<String>,
    city: Option<String>,
    country: Option<String>,
    todos: Vec<String>,
    reconnect_tag: Option<String>,
    vcf_path: String,
}

#[derive(Debug, Deserialize)]
struct GetDueArgs {
    /// Optional date to check (defaults to today)
    as_of: Option<String>,
}

#[derive(Debug, Serialize)]
struct DueItems {
    tasks: Vec<TaskSummary>,
    reconnects: Vec<ReconnectSummary>,
}

#[derive(Debug, Serialize)]
struct TaskSummary {
    id: i32,
    contact_uid: String,
    contact_name: String,
    description: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct ReconnectSummary {
    id: i32,
    contact_uid: String,
    contact_name: String,
    due_date: String,
    tag: String,
    status: String,
}

async fn handle_upsert_contacts(pool: &PgPool, args: UpsertContactsArgs) -> Result<String> {
    info!("Upserting {} contacts", args.contacts.len());
    
    for input in args.contacts {
        // Upsert contact
        upsert_contact(
            pool,
            &input.uid,
            &input.full_name,
            input.email.as_deref(),
            input.phone.as_deref(),
            input.org.as_deref(),
            input.city.as_deref(),
            input.country.as_deref(),
            &input.vcf_path,
        )
        .await?;
        
        // Sync tasks
        sync_tasks(pool, &input.uid, &input.todos).await?;
        
        // Sync reconnect
        if let Some(tag) = input.reconnect_tag {
            sync_reconnect(pool, &input.uid, &tag).await?;
        }
    }
    
    Ok(format!("Upserted {} contacts", args.contacts.len()))
}

async fn handle_get_due(pool: &PgPool, args: GetDueArgs) -> Result<DueItems> {
    let as_of = if let Some(date_str) = args.as_of {
        NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")?
    } else {
        chrono::Local::now().date_naive()
    };
    
    info!("Getting due items as of {}", as_of);
    
    let tasks = get_pending_tasks(pool).await?;
    let reconnects = get_due_reconnects(pool, as_of).await?;
    
    let task_summaries: Vec<TaskSummary> = tasks
        .into_iter()
        .map(|t| TaskSummary {
            id: t.id,
            contact_uid: t.contact_uid,
            contact_name: t.contact_name,
            description: t.description,
            status: format!("{:?}", t.status),
        })
        .collect();
    
    let reconnect_summaries: Vec<ReconnectSummary> = reconnects
        .into_iter()
        .map(|r| ReconnectSummary {
            id: r.id,
            contact_uid: r.contact_uid,
            contact_name: r.contact_name,
            due_date: r.due_date.to_string(),
            tag: r.tag,
            status: format!("{:?}", r.status),
        })
        .collect();
    
    Ok(DueItems {
        tasks: task_summaries,
        reconnects: reconnect_summaries,
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();
    
    info!("Starting mcp-scheduler-server");
    
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in .env");
    
    let pool = PgPool::connect(&database_url).await?;
    info!("Connected to database");
    
    let server = Server::new("mcp-scheduler-server")
        .with_tool(
            "upsert_contacts",
            "Upsert contacts to database and sync their tasks/reconnects",
            {
                let pool = pool.clone();
                move |args: UpsertContactsArgs| {
                    let pool = pool.clone();
                    async move { handle_upsert_contacts(&pool, args).await }
                }
            },
        )
        .with_tool(
            "get_due",
            "Get all pending tasks and due reconnects",
            {
                let pool = pool.clone();
                move |args: GetDueArgs| {
                    let pool = pool.clone();
                    async move { handle_get_due(&pool, args).await }
                }
            },
        );
    
    server.run_stdio().await?;
    
    Ok(())
}
