//! Smoke test: resolve random picks against CONTACTS_DIR (same path as dashboard).

use anyhow::Result;
use chrono::Local;
use pepper_crm::{parse_vcards_from_dir, resolve_random_picks, RANDOM_PICK_COUNT};
use std::path::PathBuf;

fn main() -> Result<()> {
    let dir = std::env::var("CONTACTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./contacts"));
    let cache = std::env::var("CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".cache"));

    let contacts = parse_vcards_from_dir(&dir)?;
    println!("Parsed {} contacts", contacts.len());

    let as_of = Local::now().date_naive();
    let week = resolve_random_picks(&contacts, &cache, as_of, RANDOM_PICK_COUNT)?;
    println!(
        "Week {} — {} picks (eligible {}, shuffled={})",
        week.week_id,
        week.picks.len(),
        week.eligible_count,
        week.shuffled
    );
    for p in &week.picks {
        println!("  · {}", p.full_name);
    }
    Ok(())
}
