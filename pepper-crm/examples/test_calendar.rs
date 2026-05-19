//! # Test Calendar Example
//!
//!   End-to-end check: fetch Google Calendar ICS, list next-week trips, then run a full travel
//!   snapshot build with geocoding and contact matching.
//!
//! INPUT:
//!   - `GOOGLE_CALENDAR_ICS_URL` and other env vars via `.env` (`CONTACTS_DIR`, cache paths).
//!
//! OUTPUT:
//!   - Stdout: ICS preview, trip list, snapshot week id, and per-trip match details.
//!
//! NOTES:
//!   - Run: `cargo run -p pepper-crm --example test_calendar`
//!   - Rejects embed URLs; expects secret iCal feed ending in `basic.ics`.
//!   - Forces a fresh snapshot build (`config.force = true`).
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use anyhow::Result;
use chrono::Local;
use pepper_crm::{build_travel_week_snapshot, fetch_ics, trips_for_next_week, TravelBuildConfig};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let url = std::env::var("GOOGLE_CALENDAR_ICS_URL")
        .expect("GOOGLE_CALENDAR_ICS_URL not set in .env");

    if url.contains("/embed?") {
        eprintln!("ERROR: This looks like a calendar *embed* URL, not an iCal feed.");
        eprintln!("In Google Calendar: Settings → your calendar → Integrate calendar");
        eprintln!("→ copy 'Secret address in iCal format' (ends with basic.ics)");
        std::process::exit(1);
    }

    println!("Fetching calendar feed...");
    let ics = fetch_ics(&url).await?;
    let preview: String = ics.chars().take(120).collect();
    println!("Received {} bytes. Starts with: {:?}", ics.len(), preview);

    if !ics.contains("BEGIN:VCALENDAR") {
        eprintln!("ERROR: Response is not iCalendar data (no BEGIN:VCALENDAR).");
        std::process::exit(1);
    }

    let as_of = Local::now().date_naive();
    let trips = trips_for_next_week(&ics, as_of)?;
    println!("\nTrips overlapping next week ({}):", as_of);
    if trips.is_empty() {
        println!("  (none — add multi-day events with title = destination)");
    } else {
        for t in &trips {
            println!("  · {}  {} → {}", t.title, t.start, t.end);
        }
    }

    println!("\nRunning full travel build (geocode + VCF match)...");
    let mut config = TravelBuildConfig::from_env(as_of);
    config.force = true;
    let snap = build_travel_week_snapshot(&config).await?;
    println!(
        "Snapshot {}: {} trip(s), {} match(es)",
        snap.week_id,
        snap.trips.len(),
        snap.match_count()
    );
    for trip in &snap.trips {
        println!("  {} ({} matches)", trip.title, trip.matches.len());
        for m in &trip.matches {
            println!("    - {} ({:.0} km)", m.full_name, m.distance_km);
        }
    }

    Ok(())
}
