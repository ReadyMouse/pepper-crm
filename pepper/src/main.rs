//! # Pepper CRM Orchestrator
//!
//!   CLI entry point for the weekly digest pipeline: parse VCF, render HTML digest,
//!   attach `.ics` reconnect reminders, and send email (or dry-run).
//!
//! INPUT:
//!   - CLI: `--contacts-dir`, `--dry-run`, `--recipient`, `--force-travel`
//!   - Env: `DIGEST_RECIPIENT`, `SMTP_*`, `CACHE_DIR`, optional calendar URL
//!
//! OUTPUT:
//!   - Logs pipeline progress; exits 0 on success
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use anyhow::Result;
use chrono::Local;
use clap::Parser;
use pepper_crm::{load_dotenv, run_weekly_digest, WeeklyDigestConfig};
use std::path::PathBuf;
use tracing::info;

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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    load_dotenv()?;

    let args = Args::parse();

    info!("Pepper CRM starting...");
    if args.dry_run {
        info!("Running in DRY RUN mode (no email will be sent)");
    }

    let cache_root = std::env::var("CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".cache"));

    let result = run_weekly_digest(WeeklyDigestConfig {
        contacts_dir: PathBuf::from(&args.contacts_dir),
        cache_root,
        as_of: Local::now().date_naive(),
        dry_run: args.dry_run,
        force_travel: args.force_travel,
        recipient: args.recipient,
    })
    .await?;

    if result.sent {
        info!(
            "Weekly digest sent to {} — {}",
            result.recipient, result.subject
        );
    } else {
        info!(
            "Dry run complete — would send to {} — {}",
            result.recipient, result.subject
        );
    }

    info!("Pepper CRM completed successfully!");
    Ok(())
}
