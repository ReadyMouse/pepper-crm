//! Set ADR on a CardDAV contact (integration smoke test).
//!
//! Run: `cargo run -p pepper-crm --example carddav_write_location -- <uid> <city> [state]`

use anyhow::Result;
use pepper_crm::{find_contact_by_uid, load_dotenv, parse_contacts, set_contact_location};
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    load_dotenv()?;
    let mut args = env::args().skip(1);
    let uid = args
        .next()
        .unwrap_or_else(|| "2f1571de-a405-4be7-b497-f150813a53aa".to_string());
    let city = args.next().unwrap_or_else(|| "Chicago".to_string());
    let state = args.next();

    let dir = env::var("CONTACTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./contacts"));

    let _contacts = parse_contacts(&dir)?;
    let contact = find_contact_by_uid(&dir, &uid)?;
    println!("Before: city={:?} state={:?}", contact.city, contact.state);

    set_contact_location(&contact, &city, state.as_deref(), None)?;

    let updated = find_contact_by_uid(&dir, &uid)?;
    println!(
        "After: city={:?} state={:?} href={:?}",
        updated.city, updated.state, updated.carddav_href
    );
    Ok(())
}
