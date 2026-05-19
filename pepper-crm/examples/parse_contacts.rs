//! # Parse Contacts Example
//!
//!   CLI smoke test that parses all VCF files in CONTACTS_DIR and prints summary counts.
//!
//! INPUT:
//!   - `CONTACTS_DIR` from env or `.env` (default `./contacts`).
//!
//! OUTPUT:
//!   - Stdout: total parsed contacts, count with city/address, and RI-related heuristic count.
//!
//! NOTES:
//!   - Run: `cargo run -p pepper-crm --example parse_contacts`
//!   - Handles multi-vCard single-file exports (Apple/Google format).
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use anyhow::Result;
use pepper_crm::parse_vcards_from_dir;
use std::path::PathBuf;

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let dir = std::env::var("CONTACTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./contacts"));

    let contacts = parse_vcards_from_dir(&dir)?;
    let with_city = contacts.iter().filter(|c| c.city.is_some()).count();
    let ri = contacts
        .iter()
        .filter(|c| {
            let city = c.city.as_deref().unwrap_or("");
            let state = c.state.as_deref().unwrap_or("");
            city.contains("Chepachet")
                || city.eq_ignore_ascii_case("RI")
                || state.eq_ignore_ascii_case("RI")
                || city.contains("Providence")
        })
        .count();

    println!("Directory: {}", dir.display());
    println!("Parsed contacts: {}", contacts.len());
    println!("With city/address: {}", with_city);
    println!("Likely RI-related: {}", ri);

    Ok(())
}
