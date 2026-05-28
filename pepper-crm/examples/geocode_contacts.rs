//! Geocode all contacts in CONTACTS_DIR and write GEO + X-PEPPER-GEO-SOURCE back to vCards.
//!
//! Run: `cargo run -p pepper-crm --example geocode_contacts`
//!
//! Expect ~1 Nominatim request per address (~1/sec). A 900-contact export takes ~15 minutes.

use anyhow::Result;
use pepper_crm::{ensure_contacts_geocoded_in_dir, geo_coverage};
use std::path::PathBuf;

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let contacts_dir = std::env::var("CONTACTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./contacts"));
    let cache_root = std::env::var("CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".cache"));
    let write_back = std::env::var("GEO_WRITE_TO_VCF")
        .map(|s| {
            let lower = s.to_lowercase();
            lower != "0" && lower != "false" && lower != "no"
        })
        .unwrap_or(true);

    let rt = tokio::runtime::Runtime::new()?;
    let (stats, contacts) = rt.block_on(ensure_contacts_geocoded_in_dir(
        &contacts_dir,
        &cache_root,
        write_back,
    ))?;

    let (with_geo, with_address) = geo_coverage(&contacts);
    println!("GEO pass complete for {}", contacts_dir.display());
    println!("  geocoded:       {}", stats.geocoded);
    println!("  already_ok:     {}", stats.already_ok);
    println!("  failed:         {}", stats.failed);
    println!("  failed_cached:  {}", stats.failed_cached);
    println!("  skipped (no address): {}", stats.skipped_no_address);
    println!("  coverage:       {with_geo}/{with_address} addresses have GEO");

    Ok(())
}
