//! # vCard Parser and Writer
//!
//!   Reads and writes Apple/Google-style VCF files: parse contacts, unfold folded lines,
//!   and write back GEO, NOTE, REV, and CATEGORIES for CRM interactions.
//!
//! INPUT:
//!   - VCF file paths or raw vCard strings; `Contact` values for write-back operations.
//!
//! OUTPUT:
//!   - `Contact` structs; updated VCF files on disk (`GEO`, `NOTE`, `CATEGORIES`, `REV`).
//!
//! NOTES:
//!   - When `CARDDAV_*` env vars are set, contacts load via CardDAV REPORT and write via PUT.
//!   - Otherwise reads/writes local VCF files under `CONTACTS_DIR`.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use crate::birthdays::parse_bday_value;
use crate::carddav::{CardDavClient, CardDavConfig};
use crate::geo::GeoPoint;
use crate::models::Contact;
use crate::tasks::remove_todo_from_note;
use crate::tags::{
    append_log_entry, extract_log_entries, format_month_year_note_prefix, parse_categories_value,
    parse_todos, resolve_reconnect_tag, DO_NOT_ENGAGE_CATEGORY, RECONNECT_CATEGORY_PREFIX,
};

/// vCard extension: normalized address query used when `GEO` was written.
pub const PEPPER_GEO_SOURCE: &str = "X-PEPPER-GEO-SOURCE";
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tracing::{debug, warn};

static CARDDAV_CLIENT: OnceLock<Option<CardDavClient>> = OnceLock::new();

fn carddav_client() -> Option<&'static CardDavClient> {
    CARDDAV_CLIENT
        .get_or_init(|| CardDavConfig::from_env().map(CardDavClient::new))
        .as_ref()
}

/// True when `CARDDAV_URL`, `CARDDAV_USER`, and `CARDDAV_PASS` are all set.
pub fn contacts_use_carddav() -> bool {
    CardDavConfig::from_env().is_some()
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|s| {
            let lower = s.to_lowercase();
            lower == "1" || lower == "true" || lower == "yes"
        })
        .unwrap_or(false)
}

/// True when `CONTACTS_READ_ONLY` is set — local VCF and CardDAV writes are blocked.
pub fn contacts_read_only() -> bool {
    env_flag("CONTACTS_READ_ONLY")
}

fn ensure_contacts_writable() -> Result<()> {
    if contacts_read_only() {
        anyhow::bail!(
            "contacts are read-only (CONTACTS_READ_ONLY); edit on your phone or disable read-only mode"
        );
    }
    Ok(())
}

fn contact_storage_label(contact: &Contact) -> String {
    contact
        .carddav_href
        .clone()
        .unwrap_or_else(|| contact.vcf_path.display().to_string())
}

fn read_contact_vcf_content(contact: &Contact) -> Result<String> {
    if let Some(href) = &contact.carddav_href {
        let client = carddav_client()
            .context("CARDDAV_* env vars required for CardDAV contact")?;
        return client
            .get_resource(href)
            .with_context(|| format!("Failed to read CardDAV resource {href}"));
    }
    fs::read_to_string(&contact.vcf_path)
        .with_context(|| format!("Failed to read VCF file: {}", contact.vcf_path.display()))
}

fn write_contact_vcf_content(contact: &Contact, content: &str) -> Result<()> {
    ensure_contacts_writable()?;
    if contacts_use_carddav() {
        let client = carddav_client().context("CardDAV client not initialized")?;
        let put_target = client.put_url_for_contact(
            contact.carddav_href.as_deref(),
            &contact.uid,
        )?;
        return client.put_resource(&put_target, content).with_context(|| {
            format!(
                "Failed to write CardDAV resource for {}",
                contact_storage_label(contact)
            )
        });
    }
    fs::write(&contact.vcf_path, content).with_context(|| {
        format!(
            "Failed to write VCF file: {}",
            contact.vcf_path.display()
        )
    })
}

/// Load contacts from CardDAV (when configured) or from VCF files in `contacts_dir`.
pub fn parse_contacts(contacts_dir: &Path) -> Result<Vec<Contact>> {
    if let Some(client) = carddav_client() {
        return parse_vcards_from_carddav(client);
    }
    parse_vcards_from_dir(contacts_dir)
}

/// [`parse_contacts`] off the tokio runtime (CardDAV uses `reqwest::blocking`).
pub async fn parse_contacts_async(contacts_dir: PathBuf) -> Result<Vec<Contact>> {
    tokio::task::spawn_blocking(move || parse_contacts(&contacts_dir))
        .await
        .context("contacts parse task join failed")?
}

/// Run blocking VCF/CardDAV I/O off the async runtime (writes and reads).
pub async fn run_contacts_io<F, T>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .context("contacts I/O task join failed")?
}

fn parse_vcards_from_carddav(client: &CardDavClient) -> Result<Vec<Contact>> {
    let mut contacts = Vec::new();
    for (href, content) in client.fetch_all_vcards()? {
        let filename = href
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("contact.vcf");
        let vcf_path = PathBuf::from(filename);
        let blocks = split_vcard_blocks(&content);
        if blocks.is_empty() {
            warn!("No BEGIN:VCARD in CardDAV resource {href}");
            continue;
        }
        for (index, block) in blocks.into_iter().enumerate() {
            match parse_vcard_content_with_index(
                &block,
                vcf_path.clone(),
                Some(index),
                Some(href.clone()),
            ) {
                Ok(contact) => contacts.push(contact),
                Err(e) => warn!("Skipping vCard #{index} in {href}: {e}"),
            }
        }
    }
    debug!(
        "Parsed {} contacts from CardDAV ({})",
        contacts.len(),
        client.collection_url()
    );
    Ok(contacts)
}

fn collect_vcf_paths(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        if path.is_dir() {
            if name.starts_with('.') {
                continue;
            }
            collect_vcf_paths(&path, out)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("vcf") {
            out.push(path);
        }
    }

    Ok(())
}

/// Parse all VCF files under `dir` (recursive; skips hidden directories like `.stversions`).
pub fn parse_vcards_from_dir(dir: &Path) -> Result<Vec<Contact>> {
    let mut contacts = Vec::new();
    let mut vcf_paths = Vec::new();
    collect_vcf_paths(dir, &mut vcf_paths)?;
    vcf_paths.sort();

    for path in vcf_paths {
        match parse_vcards_from_path(&path) {
            Ok(mut from_file) => contacts.append(&mut from_file),
            Err(e) => {
                warn!("Failed to parse {}: {}", path.display(), e);
            }
        }
    }

    debug!("Parsed {} contacts from {}", contacts.len(), dir.display());
    Ok(contacts)
}

/// Parse one file that may contain multiple vCards (common Apple/Google export format).
pub fn parse_vcards_from_path(path: &Path) -> Result<Vec<Contact>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read VCF file: {}", path.display()))?;

    let blocks = split_vcard_blocks(&content);
    if blocks.is_empty() {
        anyhow::bail!("No BEGIN:VCARD blocks in {}", path.display());
    }

    let mut contacts = Vec::with_capacity(blocks.len());
    for (index, block) in blocks.into_iter().enumerate() {
        match parse_vcard_content_with_index(&block, path.to_path_buf(), Some(index), None) {
            Ok(contact) => contacts.push(contact),
            Err(e) => warn!("Skipping vCard #{} in {}: {}", index, path.display(), e),
        }
    }

    if contacts.is_empty() {
        anyhow::bail!("No valid contacts parsed from {}", path.display());
    }

    if contacts.len() > 1 {
        debug!(
            "Parsed {} contacts from single file {}",
            contacts.len(),
            path.display()
        );
    }

    Ok(contacts)
}

/// Parse a single VCF file (first contact only; use [`parse_vcards_from_path`] for multi-contact files).
pub fn parse_vcard(path: &Path) -> Result<Contact> {
    parse_vcards_from_path(path)?
        .into_iter()
        .next()
        .context("empty vCard file")
}

/// Split file content into individual vCard blocks.
fn split_vcard_blocks(content: &str) -> Vec<String> {
    let unfolded = unfold_vcard_lines(content);
    let mut blocks = Vec::new();
    let mut current = String::new();
    let mut in_card = false;

    for line in unfolded.lines() {
        let trimmed = line.trim();
        if trimmed == "BEGIN:VCARD" {
            if in_card && !current.is_empty() {
                blocks.push(current.clone());
            }
            current.clear();
            current.push_str("BEGIN:VCARD\n");
            in_card = true;
            continue;
        }
        if trimmed == "END:VCARD" {
            if in_card {
                current.push_str(line);
                current.push('\n');
                blocks.push(current.clone());
                current.clear();
                in_card = false;
            }
            continue;
        }
        if in_card {
            current.push_str(line);
            current.push('\n');
        }
    }

    if in_card && !current.is_empty() {
        if !current.contains("END:VCARD") {
            current.push_str("END:VCARD\n");
        }
        blocks.push(current);
    }

    blocks
}

/// Parse VCF content from a string (single vCard block).
pub fn parse_vcard_content(content: &str, vcf_path: PathBuf) -> Result<Contact> {
    parse_vcard_content_with_index(content, vcf_path, None, None)
}

fn parse_vcard_content_with_index(
    content: &str,
    vcf_path: PathBuf,
    card_index: Option<usize>,
    carddav_href: Option<String>,
) -> Result<Contact> {
    let unfolded = unfold_vcard_lines(content);

    let mut uid = String::new();
    let mut full_name = String::new();
    let mut structured_name = String::new();
    let mut email = None;
    let mut phone = None;
    let mut urls: Vec<String> = Vec::new();
    let mut org = None;
    let mut street = None;
    let mut city = None;
    let mut state = None;
    let mut country = None;
    let mut geo = None;
    let mut geo_source = None;
    let mut categories: Vec<String> = Vec::new();
    let mut note_raw = String::new();
    let mut birthday = None;
    let mut rev = None;

    for line in unfolded.lines() {
        let line = line.trim();
        
        if line.is_empty() || line.starts_with("BEGIN:") || line.starts_with("END:") || line.starts_with("VERSION:") {
            continue;
        }
        
        // Split on first colon
        if let Some(colon_pos) = line.find(':') {
            let (key_part, value_part) = line.split_at(colon_pos);
            let key = key_part.trim();
            let mut value = value_part[1..].trim().to_string();
            if key.contains("QUOTED-PRINTABLE") {
                value = decode_quoted_printable(&value);
            }

            if key.starts_with("UID") {
                uid = value;
            } else if key.starts_with("FN") {
                full_name = value;
            } else if key.starts_with("N") && !key.starts_with("NOTE") {
                structured_name = value;
            } else if key.starts_with("EMAIL") {
                if email.is_none() {
                    email = Some(value);
                }
            } else if key.starts_with("TEL") {
                if phone.is_none() {
                    phone = Some(value);
                }
            } else if key.starts_with("URL") {
                if !value.is_empty() {
                    urls.push(value);
                }
            } else if key.starts_with("ORG") {
                org = Some(value);
            } else if key.starts_with("NOTE") {
                note_raw = normalize_note_field(&value);
            } else if key.starts_with("ADR") {
                let adr_value = normalize_adr_field_value(&value);
                apply_adr_value(&adr_value, &mut street, &mut city, &mut state, &mut country);
            } else if key.starts_with("GEO") {
                if let Some(p) = parse_geo_value(&value) {
                    geo = Some(p);
                }
            } else if key.starts_with(PEPPER_GEO_SOURCE) {
                geo_source = Some(value);
            } else if key.starts_with("CATEGORIES") || key.starts_with("CATEGORY") {
                categories.extend(parse_categories_value(&value));
            } else if key.starts_with("BDAY") {
                if birthday.is_none() {
                    birthday = parse_bday_value(&value);
                }
            } else if key.starts_with("REV") {
                if rev.is_none() {
                    rev = parse_rev_value(&value);
                }
            }
        }
    }

    if full_name.is_empty() {
        full_name = resolve_display_name(&structured_name, &org, &email).unwrap_or_default();
    }
    if full_name.is_empty() {
        anyhow::bail!(
            "vCard missing FN/N/ORG/EMAIL in {}",
            vcf_path.display()
        );
    }

    if uid.is_empty() {
        uid = synthesize_uid(&full_name, &email, &phone, &vcf_path, card_index);
    }
    
    let todos = parse_todos(&note_raw);
    let reconnect_tag = resolve_reconnect_tag(&categories, &note_raw);
    let log_entries = extract_log_entries(&note_raw);

    Ok(Contact {
        uid,
        full_name,
        email,
        phone,
        urls,
        org,
        street,
        city,
        state,
        country,
        geo,
        geo_source,
        categories,
        note_raw,
        todos,
        reconnect_tag,
        birthday,
        rev,
        log_entries,
        vcf_path,
        carddav_href,
    })
}

/// Parse vCard `REV` to a calendar date (`20260519T034118Z`, `2026-05-19`, etc.).
pub fn parse_rev_value(value: &str) -> Option<NaiveDate> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Some(dt.date_naive());
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%SZ") {
        return Some(dt.date());
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%SZ") {
        return Some(dt.date());
    }
    if let Ok(d) = NaiveDate::parse_from_str(value, "%Y%m%d") {
        return Some(d);
    }
    if let Ok(d) = NaiveDate::parse_from_str(&value[..10.min(value.len())], "%Y-%m-%d") {
        return Some(d);
    }
    None
}

/// Write `REV` in vCard UTC timestamp form.
pub fn format_rev_timestamp(at: DateTime<Utc>) -> String {
    at.format("%Y%m%dT%H%M%SZ").to_string()
}

/// Display name from `N:Last;First;…`, then ORG, then email local-part.
fn resolve_display_name(
    structured_name: &str,
    org: &Option<String>,
    email: &Option<String>,
) -> Option<String> {
    if let Some(name) = display_name_from_n(structured_name) {
        return Some(name);
    }
    if let Some(o) = org.as_ref().filter(|s| !s.trim().is_empty()) {
        return Some(o.trim().to_string());
    }
    if let Some(e) = email.as_ref().filter(|s| !s.trim().is_empty()) {
        let local = e.split('@').next().unwrap_or(e).trim();
        if !local.is_empty() {
            return Some(local.to_string());
        }
    }
    None
}

/// Parse vCard `N:Family;Given;…` into a display name.
fn display_name_from_n(value: &str) -> Option<String> {
    let parts: Vec<&str> = value.split(';').collect();
    let family = parts.first().copied().unwrap_or("").trim();
    let given = parts.get(1).copied().unwrap_or("").trim();
    match (given.is_empty(), family.is_empty()) {
        (false, false) => Some(format!("{given} {family}")),
        (false, true) => Some(given.to_string()),
        (true, false) => Some(family.to_string()),
        (true, true) => None,
    }
}

/// Build a stable UID when Google/Apple exports omit the `UID:` field.
fn synthesize_uid(
    full_name: &str,
    email: &Option<String>,
    phone: &Option<String>,
    vcf_path: &Path,
    card_index: Option<usize>,
) -> String {
    let file = vcf_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("vcf");
    let index = card_index.map(|i| i.to_string()).unwrap_or_else(|| "0".into());
    let mail = email.as_deref().unwrap_or("");
    let tel = phone.as_deref().unwrap_or("");
    format!("gen:{file}:{index}:{}:{mail}:{tel}", full_name.trim())
}

/// Build a geocoder query from parsed ADR fields (same logic travel matching uses).
pub fn contact_address_query(contact: &Contact) -> Option<String> {
    geocode_queries_for_contact(contact).into_iter().next()
}

/// Ordered geocode queries for a contact (best first, fallbacks after).
pub fn geocode_queries_for_contact(contact: &Contact) -> Vec<String> {
    let street_raw = contact
        .street
        .as_deref()
        .map(clean_location_token)
        .filter(|s| !s.is_empty());
    let city_raw = contact
        .city
        .as_deref()
        .map(clean_location_token)
        .filter(|s| !s.is_empty());
    let state_raw = contact
        .state
        .as_deref()
        .map(clean_location_token)
        .filter(|s| !s.is_empty());
    let country_raw = contact
        .country
        .as_deref()
        .map(clean_location_token)
        .filter(|s| !s.is_empty());

    let Some(city) = city_raw else {
        return Vec::new();
    };

    let mut queries = Vec::new();

    if let Some(street) = street_raw {
        if let Some(st) = &state_raw {
            push_unique_query(&mut queries, format!("{street}, {city}, {st}"));
        }
        push_unique_query(&mut queries, format!("{street}, {city}"));
    }

    if city.contains(',') {
        if let Some(q) = city_state_query_from_comma_address(&city) {
            push_unique_query(&mut queries, q);
        }
    }

    if let Some(st) = &state_raw {
        push_unique_query(&mut queries, format!("{city}, {st}"));
    } else if let Some(ctry) = &country_raw {
        push_unique_query(&mut queries, format!("{city}, {ctry}"));
    } else if city.len() <= 3 {
        push_unique_query(&mut queries, format!("{city}, USA"));
    }

    push_unique_query(&mut queries, city);
    queries
}

fn push_unique_query(out: &mut Vec<String>, query: String) {
    let q = query.trim();
    if q.is_empty() {
        return;
    }
    let key = crate::geo::normalize_geocode_query(q);
    if out
        .iter()
        .any(|existing| crate::geo::normalize_geocode_query(existing) == key)
    {
        return;
    }
    out.push(q.to_string());
}

fn clean_location_token(s: &str) -> String {
    s.trim()
        .trim_end_matches('=')
        .trim()
        .replace('\n', ", ")
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn normalize_adr_field_value(value: &str) -> String {
    let decoded = if value.contains('=') {
        decode_quoted_printable(value)
    } else {
        value.to_string()
    };
    decoded
        .replace('\n', ", ")
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn city_state_query_from_comma_address(s: &str) -> Option<String> {
    let parts: Vec<&str> = s
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty() && *p != "=")
        .collect();
    if parts.len() < 2 {
        return None;
    }

    let last = parts.last()?.to_ascii_lowercase();
    let end = if parts.len() >= 3
        && (last == "usa"
            || last == "us"
            || last == "united states"
            || last.len() > 3 && !parse_state_token(parts.last()?).is_some())
    {
        parts.len() - 1
    } else {
        parts.len()
    };

    for i in (1..end).rev() {
        if let Some(st) = parse_state_token(parts[i]) {
            let city = clean_location_token(parts[i - 1]);
            if !city.is_empty() {
                return Some(format!("{city}, {st}"));
            }
        }
    }
    None
}

fn parse_state_token(s: &str) -> Option<String> {
    let s = clean_location_token(s);
    let first = s.split_whitespace().next()?;
    if first.len() == 2 && first.chars().all(|c| c.is_ascii_alphabetic()) {
        Some(first.to_ascii_uppercase())
    } else {
        None
    }
}

fn parse_geo_value(value: &str) -> Option<GeoPoint> {
    let value = value
        .trim()
        .replace("\\;", ";")
        .replace("\\,", ",");
    // vCard 3: GEO:lat;lng — vCard 4 may use geo:lat,lng prefix; strip optional "geo:"
    let coords = value.strip_prefix("geo:").unwrap_or(&value);
    let sep = if coords.contains(';') { ';' } else { ',' };
    let mut parts = coords.split(sep);
    let lat: f64 = parts.next()?.trim().parse().ok()?;
    let lng: f64 = parts.next()?.trim().parse().ok()?;
    Some(GeoPoint { lat, lng })
}

/// Write or update `GEO` and [`PEPPER_GEO_SOURCE`] on the contact's vCard block (by UID).
pub fn write_contact_geo(contact: &Contact, point: GeoPoint, source_query: &str) -> Result<()> {
    let content = read_contact_vcf_content(contact)?;

    let blocks = split_vcard_blocks(&content);
    let mut found = false;
    let updated_blocks: Vec<String> = blocks
        .into_iter()
        .enumerate()
        .map(|(index, block)| {
            if !vcard_block_matches_contact(&block, contact, &contact.vcf_path, index) {
                return block;
            }
            found = true;
            upsert_geo_in_block(&block, point, source_query, &contact.uid)
        })
        .collect();

    if !found {
        anyhow::bail!(
            "UID {} not found in {}",
            contact.uid,
            contact_storage_label(contact)
        );
    }

    let updated_content = join_vcard_blocks(&updated_blocks);
    write_contact_vcf_content(contact, &updated_content).with_context(|| {
        format!(
            "Failed to write GEO to {}",
            contact_storage_label(contact)
        )
    })?;

    debug!(
        "Wrote GEO to {} for {}",
        contact_storage_label(contact),
        contact.uid
    );
    Ok(())
}

fn vcard_block_uid(block: &str) -> Option<String> {
    for line in unfold_vcard_lines(block).lines() {
        if let Some(colon_pos) = line.find(':') {
            let key = line[..colon_pos].trim();
            if key.starts_with("UID") {
                return Some(line[colon_pos + 1..].trim().to_string());
            }
        }
    }
    None
}

/// FN plus first EMAIL/TEL from a vCard block (for synthetic UID matching).
fn extract_block_identity(block: &str) -> (String, Option<String>, Option<String>) {
    let unfolded = unfold_vcard_lines(block);
    let mut full_name = String::new();
    let mut email = None;
    let mut phone = None;
    for line in unfolded.lines() {
        let line = line.trim();
        if let Some(colon_pos) = line.find(':') {
            let key = line[..colon_pos].trim();
            let mut value = line[colon_pos + 1..].trim().to_string();
            if key.contains("QUOTED-PRINTABLE") {
                value = decode_quoted_printable(&value);
            }
            if key.starts_with("FN") {
                full_name = value;
            } else if key.starts_with("EMAIL") && email.is_none() {
                email = Some(value);
            } else if key.starts_with("TEL") && phone.is_none() {
                phone = Some(value);
            }
        }
    }
    (full_name, email, phone)
}

fn block_effective_uid(block: &str, vcf_path: &Path, card_index: usize) -> String {
    if let Some(uid) = vcard_block_uid(block) {
        if !uid.is_empty() {
            return uid;
        }
    }
    let (full_name, email, phone) = extract_block_identity(block);
    synthesize_uid(&full_name, &email, &phone, vcf_path, Some(card_index))
}

fn vcard_block_matches_contact(
    block: &str,
    contact: &Contact,
    vcf_path: &Path,
    card_index: usize,
) -> bool {
    block_effective_uid(block, vcf_path, card_index) == contact.uid
}

fn upsert_geo_in_block(block: &str, point: GeoPoint, source_query: &str, uid: &str) -> String {
    let unfolded = unfold_vcard_lines(block);
    let mut out = String::new();
    for line in unfolded.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if line_starts_with_property(line, "GEO") || line_starts_with_property(line, PEPPER_GEO_SOURCE)
        {
            continue;
        }
        if line.trim() == "END:VCARD" {
            if vcard_block_uid(block).filter(|u| !u.is_empty()).is_none() && !uid.is_empty() {
                out.push_str(&format!("UID:{uid}\n"));
            }
            out.push_str(&format!("GEO:{};{}\n", point.lat, point.lng));
            out.push_str(&format!("{PEPPER_GEO_SOURCE}:{source_query}\n"));
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn line_starts_with_property(line: &str, prop: &str) -> bool {
    let key = line.split(':').next().unwrap_or(line).trim();
    key == prop || key.starts_with(&format!("{prop};"))
}

fn join_vcard_blocks(blocks: &[String]) -> String {
    blocks
        .iter()
        .map(|b| b.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Parse ADR semicolon fields; tolerate Google exports (full address in street/city slots).
fn apply_adr_value(
    value: &str,
    street_out: &mut Option<String>,
    city: &mut Option<String>,
    state: &mut Option<String>,
    country: &mut Option<String>,
) {
    let parts: Vec<&str> = value.split(';').collect();
    let trimmed: Vec<&str> = parts.iter().map(|p| p.trim()).collect();

    let street_part = trimmed.get(2).filter(|s| !s.is_empty());
    let locality = trimmed.get(3).filter(|s| !s.is_empty());
    let region = trimmed.get(4).filter(|s| !s.is_empty());
    let nation = trimmed.get(6).filter(|s| !s.is_empty());

    // Prefer city component; fall back to street (Google often puts full address in part 2).
    if let Some(c) = locality {
        *city = Some(clean_location_token(c));
        if let Some(s) = street_part {
            let street_clean = clean_location_token(s);
            if !street_clean.eq_ignore_ascii_case(c) {
                *street_out = Some(street_clean);
            }
        }
    } else if let Some(s) = street_part {
        let street_clean = clean_location_token(s);
        if let Some((c, st, ctry)) = extract_city_state_from_comma_address(&street_clean) {
            *city = Some(c);
            if state.is_none() {
                *state = Some(st);
            }
            if country.is_none() {
                *country = ctry;
            }
        } else {
            *city = Some(street_clean);
        }
    }

    if let Some(s) = region {
        *state = Some(clean_location_token(s));
    }
    if let Some(n) = nation {
        *country = Some(clean_location_token(n));
    }

    // ;;;City;State;; — city in region slot when locality empty
    if city.is_none() {
        if let Some(c) = region {
            *city = Some(clean_location_token(c));
            *state = trimmed
                .get(5)
                .filter(|s| !s.is_empty())
                .map(|s| clean_location_token(s));
        }
    }

    if city.is_none() {
        return;
    }
    if state.is_none() || city.as_ref().is_some_and(|c| c.contains(',')) {
        if let Some(c) = city.as_ref() {
            if let Some((parsed_city, parsed_state, parsed_country)) =
                extract_city_state_from_comma_address(c)
            {
                *city = Some(parsed_city);
                if state.is_none() {
                    *state = Some(parsed_state);
                }
                if country.is_none() {
                    *country = parsed_country;
                }
            }
        }
    }
}

fn extract_city_state_from_comma_address(s: &str) -> Option<(String, String, Option<String>)> {
    let q = city_state_query_from_comma_address(s)?;
    let (city, st) = q.split_once(", ")?;
    let parts: Vec<&str> = s.split(',').map(str::trim).filter(|p| !p.is_empty()).collect();
    let last = parts.last()?.to_ascii_lowercase();
    let country = if parts.len() >= 3
        && (last == "usa" || last == "us" || last == "united states")
    {
        None
    } else if parts.len() >= 3 && parse_state_token(parts.last()?).is_none() {
        Some(clean_location_token(parts.last()?))
    } else {
        None
    };
    Some((city.to_string(), st.to_string(), country))
}

fn decode_quoted_printable(input: &str) -> String {
    let mut out = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'\n' || bytes[i + 1] == b'\r' {
                i += 2;
                continue;
            }
            if i + 2 < bytes.len() {
                if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                    if let Ok(byte) = u8::from_str_radix(hex, 16) {
                        out.push(byte);
                        i += 3;
                        continue;
                    }
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Unfold vCard lines (lines can be wrapped with leading whitespace)
fn unfold_vcard_lines(content: &str) -> String {
    let mut result = String::new();
    let mut current_line = String::new();
    
    for line in content.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            // Continuation line - append without newline (standard vCard unfolding)
            current_line.push_str(line.trim_start());
        } else {
            // New field line - flush current and start new
            if !current_line.is_empty() {
                result.push_str(&current_line);
                result.push('\n');
            }
            current_line = line.to_string();
        }
    }
    
    // Don't forget the last line
    if !current_line.is_empty() {
        result.push_str(&current_line);
        result.push('\n');
    }
    
    result
}

/// Post-process NOTE field to add newlines before tag patterns
/// The vCard folding removes semantic line breaks, so we add them back for tag parsing
fn normalize_note_field(note: &str) -> String {
    let mut result = note.to_string();
    
    // Add newline before TODO: (any casing) if it's not at the start
    static TODO_PREFIX: OnceLock<Regex> = OnceLock::new();
    let todo_re = TODO_PREFIX.get_or_init(|| Regex::new(r"(?i)TODO:").expect("todo prefix regex"));
    result = todo_re.replace_all(&result, "\nTODO:").into_owned();
    // Add newline before Reconnect: if it's not at the start  
    result = result.replace("Reconnect:", "\nReconnect:");
    // Add newline before --- CRM Log ---
    result = result.replace("--- CRM Log ---", "\n--- CRM Log ---");

    // Clean up: remove leading newline if we added one at the start
    if result.starts_with('\n') {
        result = result[1..].to_string();
    }
    
    result
}

/// Find a contact by UID (CardDAV or local VCF directory).
pub fn find_contact_by_uid(contacts_dir: &Path, uid: &str) -> Result<Contact> {
    for contact in parse_contacts(contacts_dir)? {
        if contact.uid == uid {
            return Ok(contact);
        }
    }
    anyhow::bail!("contact UID {uid} not found under {}", contacts_dir.display())
}

/// Set reconnect interval or `Do Not Engage` from the Random People dashboard dropdown.
pub fn set_random_pick_category(contact: &Contact, choice: &str, as_of: NaiveDate) -> Result<()> {
    if choice.trim().eq_ignore_ascii_case(DO_NOT_ENGAGE_CATEGORY) {
        set_do_not_engage(contact, as_of)
    } else {
        set_reconnect_snooze(contact, choice, as_of)
    }
}

/// Set `Reconnect: …` in `CATEGORIES` and refresh `REV` as the snooze anchor.
pub fn set_reconnect_snooze(contact: &Contact, reconnect_body: &str, _as_of: NaiveDate) -> Result<()> {
    let rev_stamp = format_rev_timestamp(Utc::now());
    let updated_categories = upsert_reconnect_categories(&contact.categories, reconnect_body);

    let content = read_contact_vcf_content(contact)?;

    let blocks = split_vcard_blocks(&content);
    let mut found = false;
    let updated_blocks: Vec<String> = blocks
        .into_iter()
        .enumerate()
        .map(|(index, block)| {
            if !vcard_block_matches_contact(&block, contact, &contact.vcf_path, index) {
                return block;
            }
            found = true;
            upsert_reconnect_in_block(&block, &updated_categories, &rev_stamp, None)
        })
        .collect();

    if !found {
        anyhow::bail!(
            "UID {} not found in {}",
            contact.uid,
            contact_storage_label(contact)
        );
    }

    write_contact_vcf_content(contact, &join_vcard_blocks(&updated_blocks)).with_context(|| {
        format!(
            "Failed to write reconnect snooze to {}",
            contact_storage_label(contact)
        )
    })?;

    debug!(
        "Set reconnect snooze on {} for {}",
        contact_storage_label(contact),
        contact.uid
    );
    Ok(())
}

/// Mark a TODO done by removing its line from the vCard NOTE field.
pub fn complete_task(contact: &Contact, todo_body: &str) -> Result<()> {
    let updated_note = remove_todo_from_note(&contact.note_raw, todo_body);
    if updated_note == contact.note_raw {
        anyhow::bail!("TODO not found on contact");
    }
    let note = if updated_note.trim().is_empty() {
        "."
    } else {
        updated_note.trim()
    };
    set_contact_note(contact, note)
}

/// Replace the vCard `NOTE` field (used when enriching a contact from the dashboard).
pub fn set_contact_note(contact: &Contact, note: &str) -> Result<()> {
    let note = note.trim();
    if note.is_empty() {
        anyhow::bail!("Note cannot be empty");
    }

    let content = read_contact_vcf_content(contact)?;

    let blocks = split_vcard_blocks(&content);
    let mut found = false;
    let updated_blocks: Vec<String> = blocks
        .into_iter()
        .enumerate()
        .map(|(index, block)| {
            if !vcard_block_matches_contact(&block, contact, &contact.vcf_path, index) {
                return block;
            }
            found = true;
            upsert_note_in_block(&block, note)
        })
        .collect();

    if !found {
        anyhow::bail!(
            "UID {} not found in {}",
            contact.uid,
            contact_storage_label(contact)
        );
    }

    write_contact_vcf_content(contact, &join_vcard_blocks(&updated_blocks)).with_context(|| {
        format!(
            "Failed to write note to {}",
            contact_storage_label(contact)
        )
    })?;

    debug!(
        "Set note on {} for {}",
        contact_storage_label(contact),
        contact.uid
    );
    Ok(())
}

/// Set street/city/state on the vCard `ADR` field; clears stale `GEO` so travel can re-geocode.
pub fn set_contact_location(
    contact: &Contact,
    city: &str,
    state: Option<&str>,
    street: Option<&str>,
) -> Result<()> {
    let city = city.trim();
    if city.is_empty() {
        anyhow::bail!("City is required");
    }
    let state = state.map(str::trim).filter(|s| !s.is_empty());
    let street = street.map(str::trim).filter(|s| !s.is_empty());

    let content = read_contact_vcf_content(contact)?;

    let blocks = split_vcard_blocks(&content);
    let mut found = false;
    let updated_blocks: Vec<String> = blocks
        .into_iter()
        .enumerate()
        .map(|(index, block)| {
            if !vcard_block_matches_contact(&block, contact, &contact.vcf_path, index) {
                return block;
            }
            found = true;
            upsert_adr_in_block(&block, street, city, state)
        })
        .collect();

    if !found {
        anyhow::bail!(
            "UID {} not found in {}",
            contact.uid,
            contact_storage_label(contact)
        );
    }

    write_contact_vcf_content(contact, &join_vcard_blocks(&updated_blocks)).with_context(|| {
        format!(
            "Failed to write location to {}",
            contact_storage_label(contact)
        )
    })?;

    debug!(
        "Set location on {} for {}",
        contact_storage_label(contact),
        contact.uid
    );
    Ok(())
}

fn upsert_note_in_block(block: &str, new_note: &str) -> String {
    let unfolded = unfold_vcard_lines(block);
    let mut out = String::new();
    let mut note_done = false;

    for line in unfolded.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if line_starts_with_property(line, "NOTE") {
            if !note_done {
                out.push_str(&format_note_property(new_note));
                note_done = true;
            }
            continue;
        }
        if line.trim() == "END:VCARD" {
            if !note_done {
                out.push_str(&format_note_property(new_note));
                note_done = true;
            }
            out.push_str("END:VCARD\n");
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }

    out
}

fn adr_line_key_from_block(block: &str) -> String {
    for line in unfold_vcard_lines(block).lines() {
        if line_starts_with_property(line, "ADR") {
            return line.split(':').next().unwrap_or("ADR;TYPE=HOME").to_string();
        }
    }
    "ADR;TYPE=HOME".to_string()
}

fn format_adr_value(street: Option<&str>, city: &str, state: Option<&str>) -> String {
    let street = street.map(str::trim).filter(|s| !s.is_empty());
    match (street, state) {
        (Some(street), Some(st)) => format!(";;{street};{city};{st};;"),
        (Some(street), None) => format!(";;{street};{city};;;"),
        (None, Some(st)) => format!(";;;{city};{st};;"),
        (None, None) => format!(";;;{city};;;"),
    }
}

fn upsert_adr_in_block(block: &str, street: Option<&str>, city: &str, state: Option<&str>) -> String {
    let adr_key = adr_line_key_from_block(block);
    let adr_value = format_adr_value(street, city, state);
    let unfolded = unfold_vcard_lines(block);
    let mut out = String::new();
    let mut adr_done = false;

    for line in unfolded.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if line_starts_with_property(line, "ADR") {
            if !adr_done {
                out.push_str(&format!("{adr_key}:{adr_value}\n"));
                adr_done = true;
            }
            continue;
        }
        if line_starts_with_property(line, "GEO") || line_starts_with_property(line, PEPPER_GEO_SOURCE)
        {
            continue;
        }
        if line.trim() == "END:VCARD" {
            if !adr_done {
                out.push_str(&format!("{adr_key}:{adr_value}\n"));
                adr_done = true;
            }
            out.push_str("END:VCARD\n");
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }

    out
}

fn prepend_note_line(note: &str, line: &str) -> String {
    let trimmed = note.trim();
    if trimmed.is_empty() {
        line.to_string()
    } else {
        format!("{line}\n{trimmed}")
    }
}

/// Emit a `NOTE` property with RFC 2425 line folding for embedded newlines.
fn format_note_property(note: &str) -> String {
    let trimmed = note.trim();
    if trimmed.is_empty() || trimmed == "." {
        return "NOTE:.\n".to_string();
    }
    let mut lines = trimmed.lines();
    let first = lines.next().unwrap_or(".");
    let mut out = format!("NOTE:{first}\n");
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        out.push_str(&format!(" {line}\n"));
    }
    out
}

fn upsert_reconnect_categories(categories: &[String], reconnect_body: &str) -> Vec<String> {
    let mut out: Vec<String> = categories
        .iter()
        .filter(|c| {
            let t = c.trim();
            !t.starts_with(RECONNECT_CATEGORY_PREFIX)
                && !t.eq_ignore_ascii_case(DO_NOT_ENGAGE_CATEGORY)
        })
        .cloned()
        .collect();
    out.push(format!("{RECONNECT_CATEGORY_PREFIX} {reconnect_body}"));
    out
}

fn upsert_do_not_engage_categories(categories: &[String]) -> Vec<String> {
    let mut out: Vec<String> = categories
        .iter()
        .filter(|c| {
            let t = c.trim();
            !t.starts_with(RECONNECT_CATEGORY_PREFIX)
                && !t.eq_ignore_ascii_case(DO_NOT_ENGAGE_CATEGORY)
        })
        .cloned()
        .collect();
    out.push(DO_NOT_ENGAGE_CATEGORY.to_string());
    out
}

fn set_do_not_engage(contact: &Contact, as_of: NaiveDate) -> Result<()> {
    let rev_stamp = format_rev_timestamp(Utc::now());
    let stamp = format!(
        "{}: Marked Do Not Engage.",
        format_month_year_note_prefix(as_of)
    );
    let updated_note = prepend_note_line(&contact.note_raw, &stamp);
    let updated_categories = upsert_do_not_engage_categories(&contact.categories);

    let content = read_contact_vcf_content(contact)?;

    let blocks = split_vcard_blocks(&content);
    let mut found = false;
    let updated_blocks: Vec<String> = blocks
        .into_iter()
        .enumerate()
        .map(|(index, block)| {
            if !vcard_block_matches_contact(&block, contact, &contact.vcf_path, index) {
                return block;
            }
            found = true;
            upsert_reconnect_in_block(&block, &updated_categories, &rev_stamp, Some(&updated_note))
        })
        .collect();

    if !found {
        anyhow::bail!(
            "UID {} not found in {}",
            contact.uid,
            contact_storage_label(contact)
        );
    }

    write_contact_vcf_content(contact, &join_vcard_blocks(&updated_blocks)).with_context(|| {
        format!(
            "Failed to write Do Not Engage to {}",
            contact_storage_label(contact)
        )
    })?;

    debug!(
        "Set Do Not Engage on {} for {}",
        contact_storage_label(contact),
        contact.uid
    );
    Ok(())
}

fn upsert_reconnect_in_block(
    block: &str,
    categories: &[String],
    rev_value: &str,
    note_override: Option<&str>,
) -> String {
    let categories_value = categories.join(",");
    let unfolded = unfold_vcard_lines(block);
    let mut out = String::new();
    let mut note_done = false;
    let mut categories_done = false;

    for line in unfolded.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if line_starts_with_property(line, "REV") {
            continue;
        }
        if line_starts_with_property(line, "NOTE") {
            if let Some(new_note) = note_override {
                if !note_done {
                    out.push_str(&format_note_property(new_note));
                    note_done = true;
                }
                continue;
            }
            note_done = true;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if line_starts_with_property(line, "CATEGORIES") || line_starts_with_property(line, "CATEGORY")
        {
            if !categories_done {
                out.push_str(&format!("CATEGORIES:{categories_value}\n"));
                categories_done = true;
            }
            continue;
        }
        if line.trim() == "END:VCARD" {
            if !categories_done {
                out.push_str(&format!("CATEGORIES:{categories_value}\n"));
                categories_done = true;
            }
            if let Some(new_note) = note_override {
                if !note_done {
                    out.push_str(&format_note_property(new_note));
                    note_done = true;
                }
            }
            out.push_str(&format!("REV:{rev_value}\n"));
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Write a log entry back to the contact's vCard (local file or CardDAV PUT).
pub fn log_interaction(
    contact: &Contact,
    note: &str,
    new_reconnect_tag: Option<&str>,
) -> Result<()> {
    let content = read_contact_vcf_content(contact)?;

    let updated_note = append_log_entry(&contact.note_raw, note, new_reconnect_tag);
    let updated_content = update_note_field(&content, &updated_note)?;

    write_contact_vcf_content(contact, &updated_content).with_context(|| {
        format!(
            "Failed to write interaction to {}",
            contact_storage_label(contact)
        )
    })?;

    debug!("Logged interaction to {}", contact_storage_label(contact));
    Ok(())
}

/// Update the NOTE field in VCF content
fn update_note_field(content: &str, new_note: &str) -> Result<String> {
    let unfolded = unfold_vcard_lines(content);
    let mut result = String::new();
    let mut in_note = false;
    
    for line in unfolded.lines() {
        if line.starts_with("NOTE:") {
            result.push_str(&format_note_property(new_note));
            in_note = true;
        } else if in_note && (line.starts_with(' ') || line.starts_with('\t')) {
            // Skip continuation lines of old NOTE
            continue;
        } else {
            in_note = false;
            result.push_str(line);
            result.push('\n');
        }
    }
    
    // If no NOTE field existed, add one before END:VCARD
    if !result.contains("NOTE:") {
        let note_block = format_note_property(new_note).trim_end().to_string();
        let lines: Vec<&str> = result.lines().collect();
        if let Some(pos) = lines.iter().position(|&line| line.starts_with("END:VCARD")) {
            let mut owned_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
            for (offset, note_line) in note_block.lines().enumerate() {
                owned_lines.insert(pos + offset, note_line.to_string());
            }
            result = owned_lines.join("\n");
            result.push('\n');
        }
    }
    
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_upsert_preserves_geo_adr_and_note() {
        let block = "BEGIN:VCARD\nVERSION:4.0\nUID:test-contact\nADR;TYPE=HOME:;;;Chicago;IL;;\nFN:Jane Doe\nGEO:41.8;-87.6\nNOTE:May 2025: Met at networking event.\nREV:20260501T120000Z\nX-PEPPER-GEO-SOURCE:chicago\nEND:VCARD\n";
        let out = upsert_reconnect_in_block(
            block,
            &["Reconnect: 3 months".to_string()],
            "20260608T120000Z",
            None,
        );
        assert!(out.contains("GEO:41.8;-87.6"));
        assert!(out.contains("ADR;TYPE=HOME:;;;Chicago;IL;;"));
        assert!(out.contains("CATEGORIES:Reconnect: 3 months"));
        assert!(out.contains("NOTE:May 2025: Met at networking event."));
        assert!(!out.contains("Updated reconnect time"));
        assert!(out.contains("REV:20260608T120000Z"));
    }

    #[test]
    fn format_note_property_folds_multiline_notes() {
        let note = "June 2026: Updated reconnect time.\nMay 2025: Met at networking event.";
        let prop = format_note_property(note);
        assert!(prop.starts_with("NOTE:June 2026: Updated reconnect time.\n"));
        assert!(prop.contains("\n May 2025: Met at networking event.\n"));
    }

    #[test]
    fn test_unfold_vcard_lines() {
        let folded = "FN:John Doe\nNOTE:This is a long note\n that continues here\n and here too\nEND:VCARD";
        let unfolded = unfold_vcard_lines(folded);
        assert!(unfolded.contains("NOTE:This is a long notethat continues hereand here too"));
    }

    #[test]
    fn test_split_vcard_blocks_multiple() {
        let content = r#"BEGIN:VCARD
VERSION:3.0
UID:one
FN:Alice
END:VCARD
BEGIN:VCARD
VERSION:3.0
UID:two
FN:Bob
END:VCARD"#;
        let blocks = split_vcard_blocks(content);
        assert_eq!(blocks.len(), 2);
        let c1 = parse_vcard_content_with_index(&blocks[0], PathBuf::from("/tmp/a.vcf"), Some(0), None).unwrap();
        let c2 = parse_vcard_content_with_index(&blocks[1], PathBuf::from("/tmp/a.vcf"), Some(1), None).unwrap();
        assert_eq!(c1.full_name, "Alice");
        assert_eq!(c2.full_name, "Bob");
    }

    #[test]
    fn test_parse_vcards_from_path_multiple() {
        let content = r#"BEGIN:VCARD
UID:a1
FN:First Last
ADR;TYPE=HOME:;;1 Main;Boston;MA;02101;USA
END:VCARD
BEGIN:VCARD
UID:a2
FN:Second Person
ADR;TYPE=HOME:;;2 Oak;Providence;RI;02903;USA
END:VCARD"#;
        let dir = std::env::temp_dir().join("pepper_vcf_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("all.vcf");
        fs::write(&path, content).unwrap();
        let contacts = parse_vcards_from_path(&path).unwrap();
        assert_eq!(contacts.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_rev_value() {
        assert_eq!(
            parse_rev_value("20260519T034118Z"),
            Some(NaiveDate::from_ymd_opt(2026, 5, 19).unwrap())
        );
        assert_eq!(
            parse_rev_value("2025-03-01T12:00:00Z"),
            Some(NaiveDate::from_ymd_opt(2025, 3, 1).unwrap())
        );
    }

    #[test]
    fn test_parse_url_fields() {
        let vcf = r#"BEGIN:VCARD
FN:Pat Example
URL:https://www.linkedin.com/in/patexample
URL:https://example.com
END:VCARD"#;
        let c = parse_vcard_content(vcf, PathBuf::from("c.vcf")).unwrap();
        assert_eq!(c.urls.len(), 2);
        assert!(c.urls[0].contains("linkedin.com"));
    }

    #[test]
    fn test_parse_org_only_without_fn() {
        let vcf = r#"BEGIN:VCARD
VERSION:2.1
ORG:Global Kink And Leather
NOTE:Met at event.
END:VCARD"#;
        let c = parse_vcard_content(vcf, PathBuf::from("contacts.vcf")).unwrap();
        assert_eq!(c.full_name, "Global Kink And Leather");
        assert_eq!(c.org.as_deref(), Some("Global Kink And Leather"));
    }

    #[test]
    fn test_google_export_without_uid() {
        let vcf = r#"BEGIN:VCARD
VERSION:2.1
N:Test;Person;;;
FN:Person Test
TEL;CELL:555-0100
ADR;HOME:;;98 Lakeview Dr, Chepachet, RI;;;;
END:VCARD"#;
        let c = parse_vcard_content(vcf, PathBuf::from("contacts.vcf")).unwrap();
        assert!(c.uid.starts_with("gen:"));
        assert_eq!(c.city.as_deref(), Some("Chepachet"));
        assert_eq!(c.state.as_deref(), Some("RI"));
        assert_eq!(
            contact_address_query(&c).as_deref(),
            Some("Chepachet, RI")
        );
    }

    #[test]
    fn test_write_geo_matches_synthesized_uid_in_multi_vcf() {
        let content = r#"BEGIN:VCARD
VERSION:3.0
FN:Alice Example
EMAIL:a@example.com
ADR;HOME:;;;Boston;MA;02101;USA
END:VCARD
BEGIN:VCARD
VERSION:3.0
FN:Bob Example
TEL:555-0001
ADR;HOME:;;;Chicago;IL;60601;USA
END:VCARD"#;
        let dir = std::env::temp_dir().join("pepper_geo_multi");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("contacts.vcf");
        fs::write(&path, content).unwrap();

        let contacts = parse_vcards_from_path(&path).unwrap();
        assert_eq!(contacts.len(), 2);
        assert!(contacts[0].uid.starts_with("gen:contacts:0:"));
        let point = GeoPoint {
            lat: 42.36,
            lng: -71.06,
        };
        write_contact_geo(&contacts[0], point, "boston, ma").unwrap();

        let updated = fs::read_to_string(&path).unwrap();
        assert!(updated.contains("GEO:42.36;-71.06"));
        assert!(updated.contains(&format!("UID:{}", contacts[0].uid)));
        let blocks = split_vcard_blocks(&updated);
        let alice = parse_vcard_content_with_index(&blocks[0], path.clone(), Some(0), None).unwrap();
        assert_eq!(alice.geo, Some(point));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_geo_write_and_read_roundtrip() {
        let vcf = r#"BEGIN:VCARD
VERSION:3.0
UID:geo-test-1
FN:Geo Person
ADR;TYPE=HOME:;;10 Main St;Boston;MA;02101;USA
END:VCARD"#;
        let dir = std::env::temp_dir().join("pepper_geo_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("geo.vcf");
        fs::write(&path, vcf).unwrap();

        let contact = parse_vcard(&path).unwrap();
        assert!(contact.geo.is_none());

        let point = GeoPoint {
            lat: 42.3601,
            lng: -71.0589,
        };
        write_contact_geo(&contact, point, "boston, ma").unwrap();

        let mut reloaded = parse_vcard(&path).unwrap();
        assert_eq!(reloaded.geo, Some(point));
        assert_eq!(reloaded.geo_source.as_deref(), Some("boston, ma"));
        assert!(!crate::contact_geo::needs_geocoding(&reloaded));

        reloaded.city = Some("Cambridge".to_string());
        assert!(crate::contact_geo::is_geo_stale(&reloaded));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_vcard_basic() {
        // Use actual multiline NOTE field with line folding (space continuation)
        let vcf_content = r#"BEGIN:VCARD
VERSION:3.0
UID:12345
FN:Alice Smith
EMAIL;TYPE=INTERNET:alice@example.com
TEL;TYPE=CELL:555-1234
ORG:Acme Corp
NOTE:Met at conference.
 TODO: send follow-up
 Reconnect: 3 months
END:VCARD"#;
        
        let contact = parse_vcard_content(vcf_content, PathBuf::from("/tmp/test.vcf")).unwrap();
        
        println!("Parsed contact: {:?}", contact);
        println!("Note raw: {}", contact.note_raw);
        println!("Todos: {:?}", contact.todos);
        
        assert_eq!(contact.uid, "12345");
        assert_eq!(contact.full_name, "Alice Smith");
        assert_eq!(contact.email, Some("alice@example.com".to_string()));
        assert_eq!(contact.todos.len(), 1, "Expected 1 TODO, got {} - Note: {}", contact.todos.len(), contact.note_raw);
        assert_eq!(contact.reconnect_tag, Some("3 months".to_string()));
    }

    #[test]
    fn test_geocode_queries_from_full_google_address() {
        let vcf = r#"BEGIN:VCARD
FN:Alex Example
ADR;TYPE=HOME:;;24 Peabody Terrace apt 709, Cambridge, MA 02138, USA;;;;
END:VCARD"#;
        let c = parse_vcard_content(vcf, PathBuf::from("c.vcf")).unwrap();
        let queries = geocode_queries_for_contact(&c);
        assert!(queries.iter().any(|q| q == "Cambridge, MA"));
    }

    #[test]
    fn test_geocode_queries_from_quoted_printable_adr() {
        let vcf = r#"BEGIN:VCARD
FN:Anna Example
ADR;TYPE=HOME:;;=32=35=20=4D=6F=75=6E=74=20=56=65=72=6E=6F=6E=20=53=74=0A=53=6F=6D=65=72=76=69=6C=6C=65=2C=20=4D=41=20=30=32=31=34=33=2C=20=55=53=41;;;;
END:VCARD"#;
        let c = parse_vcard_content(vcf, PathBuf::from("c.vcf")).unwrap();
        let queries = geocode_queries_for_contact(&c);
        assert!(queries.iter().any(|q| q.contains("Somerville")));
        assert!(queries.iter().any(|q| q == "Somerville, MA"));
    }

    #[test]
    fn test_geocode_queries_city_state_from_semicolon_adr() {
        let vcf = r#"BEGIN:VCARD
FN:Mathew Example
ADR;TYPE=HOME:;;;Somervile;MA;;;
END:VCARD"#;
        let c = parse_vcard_content(vcf, PathBuf::from("c.vcf")).unwrap();
        assert_eq!(contact_address_query(&c).as_deref(), Some("Somervile, MA"));
    }

    #[test]
    fn test_set_contact_note_and_location() {
        let vcf = r#"BEGIN:VCARD
VERSION:3.0
UID:note-loc-test
FN:Jamie Test
ORG:Example Co
END:VCARD"#;
        let dir = std::env::temp_dir().join("pepper_note_loc");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("jamie.vcf");
        fs::write(&path, vcf).unwrap();

        let contact = parse_vcards_from_path(&path).unwrap().remove(0);
        assert!(contact.note_raw.is_empty());
        assert!(contact.city.is_none());

        set_contact_note(&contact, "Met at a meetup. Works on robotics.").unwrap();
        set_contact_location(&contact, "Portland", Some("OR"), None).unwrap();

        let updated = parse_vcards_from_path(&path).unwrap().remove(0);
        assert!(updated.note_raw.contains("Met at a meetup"));
        assert_eq!(updated.city.as_deref(), Some("Portland"));
        assert_eq!(updated.state.as_deref(), Some("OR"));

        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("NOTE:Met at a meetup"));
        assert!(raw.contains("ADR;TYPE=HOME:;;;Portland;OR;;"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_vcards_from_dir_recursive() {
        let dir = std::env::temp_dir().join("pepper_vcf_nested_test");
        let _ = fs::remove_dir_all(&dir);
        let nested = dir.join("sync-folder");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(dir.join(".stversions")).unwrap();
        fs::write(
            nested.join("a.vcf"),
            "BEGIN:VCARD\nUID:n1\nFN:Nested One\nEND:VCARD\n",
        )
        .unwrap();
        fs::write(
            dir.join("root.vcf"),
            "BEGIN:VCARD\nUID:n2\nFN:Root Two\nEND:VCARD\n",
        )
        .unwrap();

        let contacts = parse_vcards_from_dir(&dir).unwrap();
        assert_eq!(contacts.len(), 2);
        let names: Vec<_> = contacts.iter().map(|c| c.full_name.as_str()).collect();
        assert!(names.contains(&"Nested One"));
        assert!(names.contains(&"Root Two"));
        let _ = fs::remove_dir_all(&dir);
    }
}
