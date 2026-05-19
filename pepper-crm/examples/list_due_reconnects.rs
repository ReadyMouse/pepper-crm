//! # List Due Reconnects Example
//!
//!   CLI example that prints contacts whose Reconnect interval is due within a configurable window,
//!   using the same logic as the dashboard digest.
//!
//! INPUT:
//!   - `CONTACTS_DIR` (default `./contacts`); optional `RECONNECT_WINDOW_DAYS` (default 7).
//!
//! OUTPUT:
//!   - Stdout list of contact name, due date, and Reconnect tag.
//!
//! NOTES:
//!   - Run: `cargo run -p pepper-crm --example list_due_reconnects`
//!   - Excludes Never, city triggers, venues, and contacts without a recent interaction anchor.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use chrono::Local;
use pepper_crm::{due_reconnects_from_contacts, parse_vcards_from_dir};

fn main() -> anyhow::Result<()> {
    let dir = std::env::var("CONTACTS_DIR").unwrap_or_else(|_| "./contacts".into());
    let as_of = Local::now().date_naive();
    let window_days: u32 = std::env::var("RECONNECT_WINDOW_DAYS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(7);

    let contacts = parse_vcards_from_dir(dir.as_ref())?;
    let due = due_reconnects_from_contacts(&contacts, as_of, window_days);

    println!("As of {as_of}, due within {window_days} days: {} contacts", due.len());
    for r in &due {
        println!("  {} — due {} — Reconnect: {}", r.full_name, r.due_date, r.tag);
    }

    Ok(())
}
