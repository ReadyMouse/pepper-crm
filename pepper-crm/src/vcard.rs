use crate::models::Contact;
use crate::tags::{extract_log_entries, parse_reconnect_tag, parse_todos, append_log_entry};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Parse all VCF files in a directory
pub fn parse_vcards_from_dir(dir: &Path) -> Result<Vec<Contact>> {
    let mut contacts = Vec::new();
    
    let entries = fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?;
    
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        
        if path.extension().and_then(|s| s.to_str()) == Some("vcf") {
            match parse_vcard(&path) {
                Ok(contact) => contacts.push(contact),
                Err(e) => {
                    warn!("Failed to parse {}: {}", path.display(), e);
                }
            }
        }
    }
    
    debug!("Parsed {} contacts from {}", contacts.len(), dir.display());
    Ok(contacts)
}

/// Parse a single VCF file
pub fn parse_vcard(path: &Path) -> Result<Contact> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read VCF file: {}", path.display()))?;
    
    parse_vcard_content(&content, path.to_path_buf())
}

/// Parse VCF content from a string
pub fn parse_vcard_content(content: &str, vcf_path: PathBuf) -> Result<Contact> {
    // Unfold lines (handle vCard line folding where lines wrapped with leading space)
    let unfolded = unfold_vcard_lines(content);
    
    let mut uid = String::new();
    let mut full_name = String::new();
    let mut email = None;
    let mut phone = None;
    let mut org = None;
    let mut city = None;
    let mut country = None;
    let mut note_raw = String::new();
    
    for line in unfolded.lines() {
        let line = line.trim();
        
        if line.is_empty() || line.starts_with("BEGIN:") || line.starts_with("END:") || line.starts_with("VERSION:") {
            continue;
        }
        
        // Split on first colon
        if let Some(colon_pos) = line.find(':') {
            let (key_part, value_part) = line.split_at(colon_pos);
            let key = key_part.trim();
            let value = value_part[1..].trim(); // Skip the colon
            
            // Handle various vCard field variations
            if key.starts_with("UID") {
                uid = value.to_string();
            } else if key.starts_with("FN") {
                full_name = value.to_string();
            } else if key.starts_with("EMAIL") {
                if email.is_none() {
                    email = Some(value.to_string());
                }
            } else if key.starts_with("TEL") {
                if phone.is_none() {
                    phone = Some(value.to_string());
                }
            } else if key.starts_with("ORG") {
                org = Some(value.to_string());
            } else if key.starts_with("NOTE") {
                // Normalize the NOTE field to add newlines before tag patterns
                note_raw = normalize_note_field(value);
            } else if key.starts_with("ADR") {
                // ADR format: ;;street;city;state;zip;country
                let parts: Vec<&str> = value.split(';').collect();
                if parts.len() >= 4 {
                    if !parts[3].is_empty() {
                        city = Some(parts[3].to_string());
                    }
                }
                if parts.len() >= 7 && !parts[6].is_empty() {
                    country = Some(parts[6].to_string());
                }
            }
        }
    }
    
    if uid.is_empty() {
        anyhow::bail!("VCF file missing UID field");
    }
    
    if full_name.is_empty() {
        warn!("VCF file {} has no FN field, using UID", vcf_path.display());
        full_name = uid.clone();
    }
    
    // Parse tags from note field
    let todos = parse_todos(&note_raw);
    let reconnect_tag = parse_reconnect_tag(&note_raw);
    let log_entries = extract_log_entries(&note_raw);
    
    Ok(Contact {
        uid,
        full_name,
        email,
        phone,
        org,
        city,
        country,
        note_raw,
        todos,
        reconnect_tag,
        log_entries,
        vcf_path,
    })
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
