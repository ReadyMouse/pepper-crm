use crate::geo::GeoPoint;
use crate::models::Contact;
use crate::tags::{
    append_log_entry, extract_log_entries, format_month_year_note_prefix, parse_categories_value,
    parse_todos, resolve_reconnect_tag, RECONNECT_CATEGORY_PREFIX,
};

/// vCard extension: normalized address query used when `GEO` was written.
pub const PEPPER_GEO_SOURCE: &str = "X-PEPPER-GEO-SOURCE";
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Parse all VCF files in a directory (each file may contain multiple `BEGIN:VCARD` blocks).
pub fn parse_vcards_from_dir(dir: &Path) -> Result<Vec<Contact>> {
    let mut contacts = Vec::new();

    let entries = fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("vcf") {
            match parse_vcards_from_path(&path) {
                Ok(mut from_file) => contacts.append(&mut from_file),
                Err(e) => {
                    warn!("Failed to parse {}: {}", path.display(), e);
                }
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
        match parse_vcard_content_with_index(&block, path.to_path_buf(), Some(index)) {
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
    parse_vcard_content_with_index(content, vcf_path, None)
}

fn parse_vcard_content_with_index(
    content: &str,
    vcf_path: PathBuf,
    card_index: Option<usize>,
) -> Result<Contact> {
    let unfolded = unfold_vcard_lines(content);

    let mut uid = String::new();
    let mut full_name = String::new();
    let mut structured_name = String::new();
    let mut email = None;
    let mut phone = None;
    let mut org = None;
    let mut city = None;
    let mut state = None;
    let mut country = None;
    let mut geo = None;
    let mut geo_source = None;
    let mut categories: Vec<String> = Vec::new();
    let mut note_raw = String::new();
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
            } else if key.starts_with("ORG") {
                org = Some(value);
            } else if key.starts_with("NOTE") {
                note_raw = normalize_note_field(&value);
            } else if key.starts_with("ADR") {
                apply_adr_value(&value, &mut city, &mut state, &mut country);
            } else if key.starts_with("GEO") {
                if let Some(p) = parse_geo_value(&value) {
                    geo = Some(p);
                }
            } else if key.starts_with(PEPPER_GEO_SOURCE) {
                geo_source = Some(value);
            } else if key.starts_with("CATEGORIES") || key.starts_with("CATEGORY") {
                categories.extend(parse_categories_value(&value));
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
        org,
        city,
        state,
        country,
        geo,
        geo_source,
        categories,
        note_raw,
        todos,
        reconnect_tag,
        rev,
        log_entries,
        vcf_path,
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
    let city = contact.city.as_ref()?.trim();
    if city.is_empty() {
        return None;
    }
    let state = contact
        .state
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let country = contact
        .country
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if city.contains(',') {
        return Some(city.to_string());
    }

    match (state, country) {
        (Some(st), _) => Some(format!("{city}, {st}")),
        (None, Some(ctry)) => Some(format!("{city}, {ctry}")),
        (None, None) if city.len() <= 3 => Some(format!("{city}, USA")),
        (None, None) => Some(city.to_string()),
    }
}

fn parse_geo_value(value: &str) -> Option<GeoPoint> {
    let value = value.trim();
    // vCard 3: GEO:lat;lng — vCard 4 may use geo:lat,lng prefix; strip optional "geo:"
    let coords = value.strip_prefix("geo:").unwrap_or(value);
    let sep = if coords.contains(';') { ';' } else { ',' };
    let mut parts = coords.split(sep);
    let lat: f64 = parts.next()?.trim().parse().ok()?;
    let lng: f64 = parts.next()?.trim().parse().ok()?;
    Some(GeoPoint { lat, lng })
}

/// Write or update `GEO` and [`PEPPER_GEO_SOURCE`] on the contact's vCard block (by UID).
pub fn write_contact_geo(contact: &Contact, point: GeoPoint, source_query: &str) -> Result<()> {
    let content = fs::read_to_string(&contact.vcf_path)
        .with_context(|| format!("Failed to read VCF file: {}", contact.vcf_path.display()))?;

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
            contact.vcf_path.display()
        );
    }

    let updated_content = join_vcard_blocks(&updated_blocks);
    fs::write(&contact.vcf_path, updated_content).with_context(|| {
        format!(
            "Failed to write GEO to VCF file: {}",
            contact.vcf_path.display()
        )
    })?;

    debug!(
        "Wrote GEO to {} for {}",
        contact.vcf_path.display(),
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
fn apply_adr_value(value: &str, city: &mut Option<String>, state: &mut Option<String>, country: &mut Option<String>) {
    let parts: Vec<&str> = value.split(';').collect();
    let trimmed: Vec<&str> = parts.iter().map(|p| p.trim()).collect();

    let street = trimmed.get(2).filter(|s| !s.is_empty());
    let locality = trimmed.get(3).filter(|s| !s.is_empty());
    let region = trimmed.get(4).filter(|s| !s.is_empty());
    let nation = trimmed.get(6).filter(|s| !s.is_empty());

    // Prefer city component; fall back to street (Google often puts full address in part 2).
    if let Some(c) = locality {
        *city = Some((*c).to_string());
    } else if let Some(s) = street {
        *city = Some((*s).to_string());
    }

    if let Some(s) = region {
        *state = Some((*s).to_string());
    }
    if let Some(n) = nation {
        *country = Some((*n).to_string());
    }

    // ;;;City;State;; — city in region slot when locality empty
    if city.is_none() {
        if let Some(c) = region {
            *city = Some((*c).to_string());
            *state = trimmed
                .get(5)
                .filter(|s| !s.is_empty())
                .map(|s| (*s).to_string());
        }
    }
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
    
    // Add newline before TODO: if it's not at the start
    result = result.replace("TODO:", "\nTODO:");
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

/// Find a contact by UID across all VCF files in a directory.
pub fn find_contact_by_uid(contacts_dir: &Path, uid: &str) -> Result<Contact> {
    for contact in parse_vcards_from_dir(contacts_dir)? {
        if contact.uid == uid {
            return Ok(contact);
        }
    }
    anyhow::bail!("contact UID {uid} not found under {}", contacts_dir.display())
}

/// Set `Reconnect: …`, update `REV` (reconnect anchor), and stamp NOTE.
pub fn set_reconnect_snooze(contact: &Contact, reconnect_body: &str, as_of: NaiveDate) -> Result<()> {
    let rev_stamp = format_rev_timestamp(Utc::now());
    let stamp = format!(
        "{}: Updated reconnect time.",
        format_month_year_note_prefix(as_of)
    );
    let updated_note = prepend_note_line(&contact.note_raw, &stamp);
    let updated_categories = upsert_reconnect_categories(&contact.categories, reconnect_body);

    let content = fs::read_to_string(&contact.vcf_path)
        .with_context(|| format!("Failed to read VCF file: {}", contact.vcf_path.display()))?;

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
            upsert_reconnect_in_block(&block, &updated_note, &updated_categories, &rev_stamp)
        })
        .collect();

    if !found {
        anyhow::bail!(
            "UID {} not found in {}",
            contact.uid,
            contact.vcf_path.display()
        );
    }

    fs::write(&contact.vcf_path, join_vcard_blocks(&updated_blocks)).with_context(|| {
        format!(
            "Failed to write reconnect snooze to {}",
            contact.vcf_path.display()
        )
    })?;

    debug!(
        "Set reconnect snooze on {} for {}",
        contact.vcf_path.display(),
        contact.uid
    );
    Ok(())
}

fn prepend_note_line(note: &str, line: &str) -> String {
    let trimmed = note.trim();
    if trimmed.is_empty() {
        line.to_string()
    } else {
        format!("{line}\n{trimmed}")
    }
}

fn upsert_reconnect_categories(categories: &[String], reconnect_body: &str) -> Vec<String> {
    let mut out: Vec<String> = categories
        .iter()
        .filter(|c| {
            let t = c.trim();
            !t.starts_with(RECONNECT_CATEGORY_PREFIX)
        })
        .cloned()
        .collect();
    out.push(format!("{RECONNECT_CATEGORY_PREFIX} {reconnect_body}"));
    out
}

fn upsert_reconnect_in_block(
    block: &str,
    new_note: &str,
    categories: &[String],
    rev_value: &str,
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
            if !note_done {
                out.push_str(&format!("NOTE:{new_note}\n"));
                note_done = true;
            }
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
            if !note_done {
                out.push_str(&format!("NOTE:{new_note}\n"));
                note_done = true;
            }
            out.push_str(&format!("REV:{rev_value}\n"));
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Write a log entry back to the contact's VCF file
/// This is the local file write-back for prototyping
/// TODO: CardDAV - replace this with an HTTP PUT to Radicale
pub fn log_interaction(
    contact: &Contact,
    note: &str,
    new_reconnect_tag: Option<&str>,
) -> Result<()> {
    // Read the current VCF file
    let content = fs::read_to_string(&contact.vcf_path)
        .with_context(|| format!("Failed to read VCF file: {}", contact.vcf_path.display()))?;
    
    // Update the NOTE field
    let updated_note = append_log_entry(&contact.note_raw, note, new_reconnect_tag);
    let updated_content = update_note_field(&content, &updated_note)?;
    
    // Write back to file
    // TODO: CardDAV - replace fs::write with HTTP PUT to https://[pi-ip]/[user]/contacts/[uid].vcf
    // Use HTTP Basic auth with CARDDAV_URL, CARDDAV_USER, CARDDAV_PASS from .env
    fs::write(&contact.vcf_path, updated_content)
        .with_context(|| format!("Failed to write VCF file: {}", contact.vcf_path.display()))?;
    
    debug!("Logged interaction to {}", contact.vcf_path.display());
    Ok(())
}

/// Update the NOTE field in VCF content
fn update_note_field(content: &str, new_note: &str) -> Result<String> {
    let unfolded = unfold_vcard_lines(content);
    let mut result = String::new();
    let mut in_note = false;
    
    for line in unfolded.lines() {
        if line.starts_with("NOTE:") {
            // Replace the NOTE line
            result.push_str(&format!("NOTE:{}\n", new_note));
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
        let note_line = format!("NOTE:{}", new_note);
        let lines: Vec<&str> = result.lines().collect();
        if let Some(pos) = lines.iter().position(|&line| line.starts_with("END:VCARD")) {
            let mut owned_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
            owned_lines.insert(pos, note_line);
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
        let c1 = parse_vcard_content_with_index(&blocks[0], PathBuf::from("/tmp/a.vcf"), Some(0)).unwrap();
        let c2 = parse_vcard_content_with_index(&blocks[1], PathBuf::from("/tmp/a.vcf"), Some(1)).unwrap();
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
        assert_eq!(c.city.as_deref(), Some("98 Lakeview Dr, Chepachet, RI"));
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
        let alice = parse_vcard_content_with_index(&blocks[0], path.clone(), Some(0)).unwrap();
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
}
