pub mod db;
pub mod ical;
pub mod models;
pub mod tags;
pub mod vcard;

// Re-export commonly used types
pub use models::{
    Contact, DigestCounts, DueItems, IcsFile, Reconnect, ReconnectRow, ReconnectStatus,
    Task, TaskRow, TaskStatus, UpsertResult,
};

// Re-export commonly used functions
pub use db::{get_due_reconnects, get_due_tasks, upsert_contacts_batch};
pub use vcard::{log_interaction, parse_vcard, parse_vcards_from_dir};
