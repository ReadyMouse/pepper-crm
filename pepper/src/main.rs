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
use pepper_crm::{
    digest_schedule_for_now, fetch_schedule_ics, load_dotenv, mark_digest_sent,
    run_weekly_digest, should_send_weekly_digest_now, WeeklyDigestConfig,
};
use std::path::PathBuf;
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

    /// Only send when calendar-aware Monday 6:00 window is active (for hourly cron)
    #[arg(long)]
    send_if_due: bool,

    /// Print digest timezone / send window and exit (no email)
    #[arg(long)]
    schedule_status: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    load_dotenv()?;

    let args = Args::parse();

    let cache_root = std::env::var("CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".cache"));

    if args.schedule_status {
        let ics = fetch_schedule_ics().await;
        let now = chrono::Utc::now();
        let info = digest_schedule_for_now(&ics, now, &cache_root)?;
        let due = pepper_crm::should_send_weekly_digest(&ics, now, &cache_root)?;
        println!("monday={}", info.monday);
        println!("timezone={}", info.timezone);
        if let Some(ref trip) = info.trip_title {
            println!("trip={trip}");
        }
        println!("send_window_active={}", due.is_some());
        if let Some(last) = pepper_crm::read_last_sent(&cache_root)? {
            println!("last_sent_week={}", last.iso_week_id);
        }
        return Ok(());
    }

    let mut due_schedule = None;
    if args.send_if_due {
        match should_send_weekly_digest_now(&cache_root).await? {
            None => {
                info!("Weekly digest not due this hour; exiting without send");
                return Ok(());
            }
            Some(info) => {
                info!(
                    "Digest due: Monday {} at {}:00 ({})",
                    info.monday,
                    pepper_crm::DIGEST_LOCAL_HOUR,
                    info.timezone
                );
                if let Some(ref trip) = info.trip_title {
                    info!("Trip on calendar: {trip}");
                }
                due_schedule = Some(info);
            }
        }
    }

    info!("Pepper CRM starting...");
    if args.dry_run {
        info!("Running in DRY RUN mode (no email will be sent)");
    }

    let result = run_weekly_digest(WeeklyDigestConfig {
        contacts_dir: PathBuf::from(&args.contacts_dir),
        cache_root: cache_root.clone(),
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

    if args.send_if_due && !args.dry_run && result.sent {
        if let Some(info) = due_schedule {
            if let Err(e) = mark_digest_sent(&cache_root, &info) {
                warn!("Could not record digest send marker: {e}");
            }
        }
    }

    info!("Pepper CRM completed successfully!");
    Ok(())
}
