//! # Weekly Digest Pipeline
//!
//!   End-to-end weekly digest: parse VCF → build digest → attach ICS → send email.
//!
//! INPUT: `WeeklyDigestConfig` — paths, dry-run flag, optional recipient override.
//! OUTPUT: `WeeklyDigestResult` with subject, counts, and whether email was sent.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use crate::digest::{build_digest_input_from_due, render_digest_email};
use crate::ical::build_ics_for_due;
use crate::mail::send_html_email;
use crate::models::{Contact, DueReconnectInfo, IcsFile, PendingTaskInfo};
use crate::tags::due_reconnects_from_contacts;
use crate::tasks::pending_tasks_from_contacts;
use crate::{build_travel_week_snapshot, load_current_snapshot, parse_contacts_async, TravelBuildConfig};
use anyhow::{Context, Result};
use chrono::NaiveDate;
use std::path::PathBuf;
use tracing::{info, warn};

pub const RECONNECT_WINDOW_DAYS: u32 = 7;

#[derive(Debug, Clone)]
pub struct WeeklyDigestConfig {
    pub contacts_dir: PathBuf,
    pub cache_root: PathBuf,
    pub as_of: NaiveDate,
    pub dry_run: bool,
    pub force_travel: bool,
    pub recipient: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WeeklyDigestResult {
    pub subject: String,
    pub html_len: usize,
    pub attachment_count: usize,
    pub task_count: usize,
    pub reconnect_count: usize,
    pub contact_count: usize,
    pub sent: bool,
    pub recipient: String,
}

pub fn fetch_due_items(
    contacts: &[Contact],
    as_of: NaiveDate,
) -> (Vec<PendingTaskInfo>, Vec<DueReconnectInfo>) {
    let tasks = pending_tasks_from_contacts(contacts);
    let reconnects = due_reconnects_from_contacts(contacts, as_of, RECONNECT_WINDOW_DAYS);
    (tasks, reconnects)
}

fn contact_email<'a>(contacts: &'a [Contact], uid: &str) -> Option<&'a str> {
    contacts
        .iter()
        .find(|c| c.uid == uid)
        .and_then(|c| c.email.as_deref())
}

fn ics_attachments_for_reconnects(
    contacts: &[Contact],
    reconnects: &[DueReconnectInfo],
) -> Vec<IcsFile> {
    reconnects
        .iter()
        .filter_map(|r| build_ics_for_due(r, contact_email(contacts, &r.uid)).ok())
        .collect()
}

async fn ensure_travel_snapshot(config: &WeeklyDigestConfig) -> Result<()> {
    let needs_travel = config.force_travel
        || load_current_snapshot(&config.cache_root, config.as_of)?.is_none();

    if !needs_travel {
        info!("Travel snapshot already exists for next week (use --force-travel to rebuild)");
        return Ok(());
    }

    if std::env::var("GOOGLE_CALENDAR_ICS_URL").is_err() {
        info!("Skipping travel build (GOOGLE_CALENDAR_ICS_URL not set)");
        return Ok(());
    }

    info!("Building travel match snapshot for digest...");
    let mut travel_config = TravelBuildConfig::from_env(config.as_of);
    travel_config.contacts_dir = config.contacts_dir.clone();
    travel_config.force = config.force_travel;
    match build_travel_week_snapshot(&travel_config).await {
        Ok(snap) => info!(
            "Travel snapshot: {} trip(s), {} match(es)",
            snap.trips.len(),
            snap.match_count()
        ),
        Err(e) => warn!("Travel snapshot build failed: {}", e),
    }
    Ok(())
}

pub async fn run_weekly_digest(config: WeeklyDigestConfig) -> Result<WeeklyDigestResult> {
    let contacts_dir = &config.contacts_dir;
    info!("Parsing VCF files from {}...", contacts_dir.display());
    let contacts = parse_contacts_async(contacts_dir.clone())
        .await
        .with_context(|| format!("Failed to parse contacts in {}", contacts_dir.display()))?;
    info!("Parsed {} contacts", contacts.len());

    ensure_travel_snapshot(&config).await?;

    let (tasks, reconnect_infos) = fetch_due_items(&contacts, config.as_of);
    info!(
        "Found {} pending tasks, {} due reconnects",
        tasks.len(),
        reconnect_infos.len()
    );

    let snapshot = load_current_snapshot(&config.cache_root, config.as_of)?;
    let digest_input = build_digest_input_from_due(
        &tasks,
        &reconnect_infos,
        &contacts,
        snapshot.as_ref(),
        &config.cache_root,
        config.as_of,
    )?;

    let digest = render_digest_email(&digest_input)?;
    let attachments = ics_attachments_for_reconnects(&contacts, &reconnect_infos);
    info!(
        "Digest rendered: {} ({} chars, {} attachment(s))",
        digest.subject,
        digest.html.len(),
        attachments.len()
    );

    let recipient = config
        .recipient
        .or_else(|| std::env::var("DIGEST_RECIPIENT").ok())
        .context("No recipient specified (use --recipient or DIGEST_RECIPIENT env var)")?;

    if config.dry_run {
        info!("=== DRY RUN: Would send email ===");
        info!("To: {}", recipient);
        info!("Subject: {}", digest.subject);
        info!("Body length: {} chars", digest.html.len());
        info!("Attachments: {}", attachments.len());
        info!("=== End dry run ===");
    } else {
        info!("Sending email to {}...", recipient);
        send_html_email(&recipient, &digest.subject, &digest.html, &attachments)?;
        info!("Email sent successfully");
    }

    Ok(WeeklyDigestResult {
        subject: digest.subject,
        html_len: digest.html.len(),
        attachment_count: attachments.len(),
        task_count: digest_input.task_count(),
        reconnect_count: digest_input.reconnect_count(),
        contact_count: contacts.len(),
        sent: !config.dry_run,
        recipient,
    })
}
