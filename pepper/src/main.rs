//! # Pepper CRM Orchestrator
//!
//!   CLI entry point that spawns MCP servers over stdio and runs the weekly digest
//!   pipeline: parse contacts, sync to Postgres, fetch due items, render HTML, export
//!   .ics attachments, and send email (or dry-run). Optionally builds a travel match
//!   snapshot via the pepper-crm library when GOOGLE_CALENDAR_ICS_URL is set.
//!
//! INPUT:
//!   - CLI: `--contacts-dir` (default `./contacts`), `--dry-run`, `--recipient`, `--force-travel`
//!   - Env: `DIGEST_RECIPIENT`, `CACHE_DIR`, `GOOGLE_CALENDAR_ICS_URL` (travel step)
//!   - MCP tool calls (in order):
//!     - `parse_vcards` — `{ "directory": "<contacts_dir>" }`
//!     - `upsert_contacts` — `{ "contacts": [<ContactSummary>, ...] }`
//!     - `get_due` — `{}`
//!     - `render_digest` — `{ "tasks": [...], "reconnects": [...] }`
//!     - `export_ics` — `{ "reconnects": [...] }`
//!     - `send_email` — `{ "to", "subject", "html_body", "attachments" }` (skipped in dry-run)
//!
//! OUTPUT:
//!   - Logs pipeline progress; exits 0 on success
//!   - Dry-run: prints recipient, subject, body length, attachment count (no email sent)
//!   - `send_email` result string when email is sent
//!   - Travel snapshot written to cache (Step 7) when needed and calendar URL is configured
//!
//! NOTES:
//!   - Spawns `./target/debug/mcp-{vcard,scheduler,digest,cal,mailer}-server` binaries
//!   - Does not spawn mcp-calendar-server or mcp-travel-server; travel uses pepper-crm directly
//!   - Travel step runs once per week unless `--force-travel` or no cached snapshot exists
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use anyhow::{Context, Result};
use chrono::Local;
use clap::Parser;
use pepper_crm::{build_travel_week_snapshot, load_current_snapshot, TravelBuildConfig};
use rmcp::*;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(name = "pepper")]
#[command(about = "Pepper CRM - Weekly digest orchestrator", long_about = None)]
struct Args {
    /// Contacts directory path
    #[arg(short, long, default_value = "./contacts")]
    contacts_dir: String,
    
    /// Dry run mode (don't send email)
    #[arg(short, long)]
    dry_run: bool,
    
    /// Email recipient (overrides DIGEST_RECIPIENT env var)
    #[arg(short, long)]
    recipient: Option<String>,

    /// Rebuild the travel match snapshot even if one exists for next week
    #[arg(long)]
    force_travel: bool,
}

async fn spawn_server(name: &str, binary: &str) -> Result<Client> {
    info!("Spawning {} server...", name);
    
    let mut cmd = Command::new(binary);
    cmd.stdin(Stdio::piped())
       .stdout(Stdio::piped())
       .stderr(Stdio::inherit());
    
    let client = Client::new_stdio(cmd).await
        .with_context(|| format!("Failed to spawn {}", name))?;
    
    info!("{} server ready", name);
    Ok(client)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();
    
    let args = Args::parse();
    
    info!("🌶️  Pepper CRM starting...");
    if args.dry_run {
        info!("Running in DRY RUN mode (no email will be sent)");
    }
    
    // Spawn all MCP servers
    let vcard_client = spawn_server("vcard", "./target/debug/mcp-vcard-server").await?;
    let scheduler_client = spawn_server("scheduler", "./target/debug/mcp-scheduler-server").await?;
    let digest_client = spawn_server("digest", "./target/debug/mcp-digest-server").await?;
    let cal_client = spawn_server("cal", "./target/debug/mcp-cal-server").await?;
    let mailer_client = spawn_server("mailer", "./target/debug/mcp-mailer-server").await?;
    
    info!("All servers spawned successfully");
    
    // Step 1: Parse VCF files
    info!("Step 1: Parsing VCF files from {}...", args.contacts_dir);
    let parse_result = vcard_client
        .call_tool("parse_vcards", json!({ "directory": args.contacts_dir }))
        .await?;
    
    let contacts: Vec<Value> = serde_json::from_value(parse_result)?;
    info!("Parsed {} contacts", contacts.len());
    
    // Step 2: Upsert contacts to database
    info!("Step 2: Syncing contacts to database...");
    let upsert_result = scheduler_client
        .call_tool("upsert_contacts", json!({ "contacts": contacts }))
        .await?;
    info!("Sync result: {}", upsert_result);
    
    // Step 3: Get due items
    info!("Step 3: Getting due tasks and reconnects...");
    let due_items = scheduler_client
        .call_tool("get_due", json!({}))
        .await?;
    
    let tasks: Vec<Value> = serde_json::from_value(due_items["tasks"].clone())?;
    let reconnects: Vec<Value> = serde_json::from_value(due_items["reconnects"].clone())?;
    
    info!("Found {} pending tasks, {} due reconnects", tasks.len(), reconnects.len());
    
    // Step 4: Render digest
    info!("Step 4: Rendering email digest...");
    let digest_result = digest_client
        .call_tool("render_digest", json!({
            "tasks": tasks,
            "reconnects": reconnects
        }))
        .await?;
    
    let html_body: String = serde_json::from_value(digest_result["html"].clone())?;
    let subject: String = serde_json::from_value(digest_result["subject"].clone())?;
    
    info!("Digest rendered: {}", subject);
    
    // Step 5: Generate .ics files
    info!("Step 5: Generating iCalendar files...");
    let ics_files = cal_client
        .call_tool("export_ics", json!({
            "reconnects": reconnects
        }))
        .await?;
    
    let attachments: Vec<Value> = serde_json::from_value(ics_files)?;
    info!("Generated {} .ics attachments", attachments.len());
    
    // Step 6: Send email (or dry run)
    if args.dry_run {
        info!("=== DRY RUN: Would send email ===");
        info!("To: {}", args.recipient.as_ref()
            .or(std::env::var("DIGEST_RECIPIENT").ok().as_ref())
            .unwrap_or(&"(not set)".to_string()));
        info!("Subject: {}", subject);
        info!("Body length: {} chars", html_body.len());
        info!("Attachments: {}", attachments.len());
        info!("=== End dry run ===");
    } else {
        info!("Step 6: Sending email...");
        let recipient = args.recipient
            .or_else(|| std::env::var("DIGEST_RECIPIENT").ok())
            .context("No recipient specified (use --recipient or DIGEST_RECIPIENT env var)")?;
        
        let send_result = mailer_client
            .call_tool("send_email", json!({
                "to": recipient,
                "subject": subject,
                "html_body": html_body,
                "attachments": attachments
            }))
            .await?;
        
        info!("Email sent: {}", send_result);
    }
    
    // Step 7: Weekly travel snapshot (once per week unless --force-travel)
    let as_of = Local::now().date_naive();
    let cache_root = std::env::var("CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".cache"));
    let needs_travel = args.force_travel
        || load_current_snapshot(&cache_root, as_of)?.is_none();

    if needs_travel {
        if std::env::var("GOOGLE_CALENDAR_ICS_URL").is_ok() {
            info!("Step 7: Building travel match snapshot...");
            let mut config = TravelBuildConfig::from_env(as_of);
            config.contacts_dir = PathBuf::from(&args.contacts_dir);
            config.force = args.force_travel;
            match build_travel_week_snapshot(&config).await {
                Ok(snap) => info!(
                    "Travel snapshot: {} trip(s), {} match(es)",
                    snap.trips.len(),
                    snap.match_count()
                ),
                Err(e) => warn!("Travel snapshot build failed: {}", e),
            }
        } else {
            info!("Step 7: Skipping travel build (GOOGLE_CALENDAR_ICS_URL not set)");
        }
    } else {
        info!("Step 7: Travel snapshot already exists for next week (use --force-travel to rebuild)");
    }

    info!("🌶️  Pepper CRM completed successfully!");

    Ok(())
}
