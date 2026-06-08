use anyhow::Result;
use chrono::Local;
use pepper_crm::{find_contact_by_uid, load_dotenv, parse_contacts, set_random_pick_category};
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    load_dotenv()?;
    let uid = env::args().nth(1).unwrap_or_else(|| "test-contact".to_string());
    let choice = env::args().nth(2).unwrap_or_else(|| "3 months".to_string());
    let dir = env::var("CONTACTS_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("./contacts"));
    let _ = parse_contacts(&dir)?;
    let contact = find_contact_by_uid(&dir, &uid)?;
    let as_of = Local::now().date_naive();
    println!("Before categories: {:?}", contact.categories);
    set_random_pick_category(&contact, &choice, as_of)?;
    let updated = find_contact_by_uid(&dir, &uid)?;
    println!("After categories: {:?}", updated.categories);
    println!("After note: {:?}", updated.note_raw);
    Ok(())
}
