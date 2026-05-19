use anyhow::{Context, Result};
use clap::Parser;
use rmcp::*;
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::process::Command;
use tracing::{error, info};

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
    
    info!("🌶️  Pepper CRM completed successfully!");
    
    Ok(())
}
