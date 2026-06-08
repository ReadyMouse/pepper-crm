//! # Pending Tasks from vCards
//!
//!   Lists and completes `TODO:` items stored in contact NOTE fields.
//!
//! INPUT:
//!   - Parsed `Contact` slice with `todos` extracted from NOTE lines.
//!   - Contact UID, task text, and NOTE body for completion write-back.
//!
//! OUTPUT:
//!   - `PendingTaskInfo` rows for dashboard and digest.
//!   - Updated NOTE with a single TODO line removed on completion.
//!
//! NOTES:
//!   - Excludes `Do Not Engage` contacts.
//!   - Task state lives entirely in vCard NOTE fields.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use crate::models::{Contact, PendingTaskInfo};
use crate::tags::is_do_not_engage;
use regex::Regex;
use std::sync::OnceLock;

static TODO_LINE: OnceLock<Regex> = OnceLock::new();

fn todo_line_re() -> &'static Regex {
    TODO_LINE.get_or_init(|| Regex::new(r"(?im)^\s*TODO:\s*(.+)$").expect("todo line regex"))
}

/// All open TODOs across contacts, excluding `Do Not Engage`.
pub fn pending_tasks_from_contacts(contacts: &[Contact]) -> Vec<PendingTaskInfo> {
    contacts
        .iter()
        .filter(|c| !is_do_not_engage(&c.categories))
        .flat_map(|c| {
            c.todos.iter().map(|todo| PendingTaskInfo {
                uid: c.uid.clone(),
                full_name: c.full_name.clone(),
                description: todo.clone(),
            })
        })
        .collect()
}

/// Remove the first matching `TODO:` line from note text (case-insensitive prefix).
pub fn remove_todo_from_note(note: &str, todo_body: &str) -> String {
    let target = todo_body.trim();
    let re = todo_line_re();
    let mut removed = false;
    let mut lines = Vec::new();

    for line in note.lines() {
        if !removed {
            if let Some(cap) = re.captures(line) {
                if cap.get(1).is_some_and(|m| m.as_str().trim() == target) {
                    removed = true;
                    continue;
                }
            }
        }
        lines.push(line);
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_todo_drops_one_matching_line() {
        let note = "Met at conference.\nTODO: send intro email\nTODO: share deck\nReconnect: 3 months";
        let updated = remove_todo_from_note(note, "send intro email");
        assert!(!updated.contains("send intro email"));
        assert!(updated.contains("share deck"));
        assert!(updated.contains("Met at conference."));
    }

    #[test]
    fn remove_last_todo_can_leave_empty_note() {
        let updated = remove_todo_from_note("TODO: only task", "only task");
        assert!(updated.trim().is_empty());
    }

    #[test]
    fn pending_tasks_skips_do_not_engage() {
        let contacts = vec![Contact {
            uid: "a".into(),
            full_name: "Alice".into(),
            email: None,
            phone: None,
            urls: vec![],
            org: None,
            street: None,
            city: None,
            state: None,
            country: None,
            geo: None,
            geo_source: None,
            categories: vec!["Do Not Engage".into()],
            note_raw: "TODO: hidden".into(),
            todos: vec!["hidden".into()],
            reconnect_tag: None,
            birthday: None,
            rev: None,
            log_entries: vec![],
            vcf_path: std::path::PathBuf::from("x.vcf"),
            carddav_href: None,
        }];
        assert!(pending_tasks_from_contacts(&contacts).is_empty());
    }
}
