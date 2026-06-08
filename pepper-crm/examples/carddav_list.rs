//! # CardDAV List Example
//!
//!   CLI demo that loads contacts from CardDAV or local VCF and prints a short summary.
//!
//! INPUT:
//!   - `CARDDAV_URL`, `CARDDAV_USER`, `CARDDAV_PASS` (optional; falls back to `CONTACTS_DIR`).
//!
//! OUTPUT:
//!   - stdout listing contact count and source (CardDAV vs local path).
//!
//! NOTES:
//!   - Run: `cargo run -p pepper-crm --example carddav_list`
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use anyhow::Result;
use pepper_crm::{contacts_use_carddav, load_dotenv, parse_contacts};
use std::path::PathBuf;

fn main() -> Result<()> {
    load_dotenv()?;
    let dir = std::env::var("CONTACTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./contacts"));

    let source = if contacts_use_carddav() {
        "CardDAV".to_string()
    } else {
        format!("VCF ({})", dir.display())
    };

    let contacts = parse_contacts(&dir)?;
    println!("Loaded {} contacts from {source}", contacts.len());
    for c in contacts.iter().take(10) {
        println!("  {} — {}", c.uid, c.full_name);
    }
    if contacts.len() > 10 {
        println!("  … and {} more", contacts.len() - 10);
    }
    Ok(())
}
