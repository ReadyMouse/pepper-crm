use chrono::{NaiveDate, DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Represents a parsed contact from a VCF file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub uid: String,
    pub full_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub org: Option<String>,
    pub city: Option<String>,       // parsed from ADR field
    pub country: Option<String>,
    pub note_raw: String,           // full raw NOTE field
    pub todos: Vec<String>,         // TODO: texts above CRM Log separator
    pub reconnect_tag: Option<String>,  // last Reconnect: value found
    pub log_entries: Vec<String>,   // lines from CRM Log block
    pub vcf_path: PathBuf,          // needed for write-back
}

/// Task data structure for database operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub contact_id: Uuid,
    pub body: String,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum TaskStatus {
    #[sqlx(rename = "pending")]
    Pending,
    #[sqlx(rename = "done")]
    Done,
    #[sqlx(rename = "snoozed")]
    Snoozed,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::Done => write!(f, "done"),
            TaskStatus::Snoozed => write!(f, "snoozed"),
        }
    }
}

/// Reconnect reminder data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reconnect {
    pub id: Uuid,
    pub contact_id: Uuid,
    pub due_date: NaiveDate,
    pub status: ReconnectStatus,
    pub original_tag: String,
    pub created_at: DateTime<Utc>,
    pub sent_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum ReconnectStatus {
    #[sqlx(rename = "pending")]
    Pending,
    #[sqlx(rename = "sent")]
    Sent,
    #[sqlx(rename = "dismissed")]
    Dismissed,
    #[sqlx(rename = "deferred")]
    Deferred,
}

impl std::fmt::Display for ReconnectStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReconnectStatus::Pending => write!(f, "pending"),
            ReconnectStatus::Sent => write!(f, "sent"),
            ReconnectStatus::Dismissed => write!(f, "dismissed"),
            ReconnectStatus::Deferred => write!(f, "deferred"),
        }
    }
}

/// Database row returned from task queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRow {
    pub task_id: Uuid,
    pub contact_id: Uuid,
    pub vcard_uid: String,
    pub full_name: String,
    pub email: Option<String>,
    pub body: String,
    pub status: TaskStatus,
}

/// Database row returned from reconnect queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectRow {
    pub reconnect_id: Uuid,
    pub contact_id: Uuid,
    pub vcard_uid: String,
    pub full_name: String,
    pub email: Option<String>,
    pub due_date: NaiveDate,
    pub original_tag: String,
    pub status: ReconnectStatus,
}

/// Items due for the current digest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DueItems {
    pub tasks: Vec<TaskRow>,
    pub reconnects: Vec<ReconnectRow>,
}

/// Result of upserting contacts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertResult {
    pub contacts_upserted: usize,
    pub tasks_created: usize,
    pub reconnects_created: usize,
}

/// Counts for logging digest runs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestCounts {
    pub contact_count: i32,
    pub task_count: i32,
    pub reconnect_count: i32,
}

/// ICS file attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IcsFile {
    pub filename: String,
    pub content: String,
}
