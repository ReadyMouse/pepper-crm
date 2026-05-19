//! # PostgreSQL Persistence
//!
//!   Upserts contacts, tasks, and reconnect reminders into Postgres and queries due items
//!   for digest emails and dashboard views.
//!
//! INPUT:
//!   - `PgPool`, parsed `Contact` slices, window days, reconnect/task UUIDs.
//!
//! OUTPUT:
//!   - `UpsertResult`, `TaskRow`, `ReconnectRow`, and digest log entries.
//!
//! NOTES:
//!   - Reconnect due dates come from vCard tags via `tags::reconnect_due_date`, not stored on contacts.
//!   - City-trigger reconnects are stored as `deferred` with a far-future due date.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use crate::models::*;
use crate::tags::{
    is_city_trigger, is_reconnect_never, reconnect_due_date, resolve_reconnect_tag,
};
use anyhow::{Context, Result};
use chrono::NaiveDate;
use sqlx::PgPool;
use sqlx::Row;
use tracing::debug;
use uuid::Uuid;

/// Upsert a contact into the database
/// Returns the contact's UUID
pub async fn upsert_contact(pool: &PgPool, contact: &Contact) -> Result<Uuid> {
    let row = sqlx::query(
        r#"
        INSERT INTO contacts (vcard_uid, full_name, email, last_synced_at)
        VALUES ($1, $2, $3, NOW())
        ON CONFLICT (vcard_uid) DO UPDATE
        SET full_name = EXCLUDED.full_name,
            email = EXCLUDED.email,
            last_synced_at = NOW()
        RETURNING id
        "#,
    )
    .bind(&contact.uid)
    .bind(&contact.full_name)
    .bind(&contact.email)
    .fetch_one(pool)
    .await
    .context("Failed to upsert contact")?;
    
    Ok(row.get("id"))
}

/// Upsert a task for a contact
/// Returns the task's UUID
pub async fn upsert_task(pool: &PgPool, contact_id: Uuid, body: &str) -> Result<Uuid> {
    // Check if this exact task already exists for this contact
    let existing = sqlx::query(
        r#"
        SELECT id FROM tasks
        WHERE contact_id = $1 AND body = $2 AND status = 'pending'
        "#,
    )
    .bind(contact_id)
    .bind(body)
    .fetch_optional(pool)
    .await?;
    
    if let Some(row) = existing {
        Ok(row.get("id"))
    } else {
        let row = sqlx::query(
            r#"
            INSERT INTO tasks (contact_id, body, status, created_at, updated_at)
            VALUES ($1, $2, 'pending', NOW(), NOW())
            RETURNING id
            "#,
        )
        .bind(contact_id)
        .bind(body)
        .fetch_one(pool)
        .await
        .context("Failed to insert task")?;
        
        Ok(row.get("id"))
    }
}

/// Upsert a reconnect reminder
/// Returns the reconnect's UUID
pub async fn upsert_reconnect(
    pool: &PgPool,
    contact_id: Uuid,
    due_date: NaiveDate,
    original_tag: &str,
) -> Result<Uuid> {
    // Check if there's already a pending reconnect for this contact
    let existing = sqlx::query(
        r#"
        SELECT id FROM reconnects
        WHERE contact_id = $1 AND status = 'pending'
        ORDER BY due_date DESC
        LIMIT 1
        "#,
    )
    .bind(contact_id)
    .fetch_optional(pool)
    .await?;
    
    if let Some(row) = existing {
        let id: Uuid = row.get("id");
        // Update the existing one
        sqlx::query(
            r#"
            UPDATE reconnects
            SET due_date = $1, original_tag = $2, created_at = NOW()
            WHERE id = $3
            "#,
        )
        .bind(due_date)
        .bind(original_tag)
        .bind(id)
        .execute(pool)
        .await?;
        
        Ok(id)
    } else {
        let row = sqlx::query(
            r#"
            INSERT INTO reconnects (contact_id, due_date, status, original_tag, created_at)
            VALUES ($1, $2, 'pending', $3, NOW())
            RETURNING id
            "#,
        )
        .bind(contact_id)
        .bind(due_date)
        .bind(original_tag)
        .fetch_one(pool)
        .await
        .context("Failed to insert reconnect")?;
        
        Ok(row.get("id"))
    }
}

/// Upsert a deferred reconnect (for city triggers like "before NY trip")
pub async fn upsert_deferred_reconnect(
    pool: &PgPool,
    contact_id: Uuid,
    original_tag: &str,
) -> Result<Uuid> {
    // Use a far-future date for deferred reconnects
    let far_future = NaiveDate::from_ymd_opt(2099, 12, 31).unwrap();
    
    let existing = sqlx::query(
        r#"
        SELECT id FROM reconnects
        WHERE contact_id = $1 AND status = 'deferred'
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(contact_id)
    .fetch_optional(pool)
    .await?;
    
    if let Some(row) = existing {
        let id: Uuid = row.get("id");
        sqlx::query(
            r#"
            UPDATE reconnects
            SET original_tag = $1, created_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(original_tag)
        .bind(id)
        .execute(pool)
        .await?;
        
        Ok(id)
    } else {
        let row = sqlx::query(
            r#"
            INSERT INTO reconnects (contact_id, due_date, status, original_tag, created_at)
            VALUES ($1, $2, 'deferred', $3, NOW())
            RETURNING id
            "#,
        )
        .bind(contact_id)
        .bind(far_future)
        .bind(original_tag)
        .fetch_one(pool)
        .await
        .context("Failed to insert deferred reconnect")?;
        
        Ok(row.get("id"))
    }
}

/// Sync contacts and their tags to the database
pub async fn upsert_contacts_batch(pool: &PgPool, contacts: &[Contact]) -> Result<UpsertResult> {
    let mut contacts_upserted = 0;
    let mut tasks_created = 0;
    let mut reconnects_created = 0;
    
    for contact in contacts {
        let contact_id = upsert_contact(pool, contact).await?;
        contacts_upserted += 1;
        
        // Upsert tasks
        for todo_body in &contact.todos {
            upsert_task(pool, contact_id, todo_body).await?;
            tasks_created += 1;
        }
        
        // Upsert reconnect (anchor from REV or latest Month YYYY note)
        if let Some(tag) = resolve_reconnect_tag(&contact.categories, &contact.note_raw) {
            if is_reconnect_never(&contact.categories, Some(&tag)) {
                // no row
            } else if is_city_trigger(&tag) {
                upsert_deferred_reconnect(pool, contact_id, &tag).await?;
                reconnects_created += 1;
            } else if let Some(due_date) = reconnect_due_date(
                &contact.categories,
                &contact.note_raw,
                Some(&tag),
                contact.rev,
                chrono::Local::now().date_naive(),
            ) {
                upsert_reconnect(pool, contact_id, due_date, &tag).await?;
                reconnects_created += 1;
            }
        }
    }
    
    debug!(
        "Upserted {} contacts, {} tasks, {} reconnects",
        contacts_upserted, tasks_created, reconnects_created
    );
    
    Ok(UpsertResult {
        contacts_upserted,
        tasks_created,
        reconnects_created,
    })
}

/// Get tasks that are pending
pub async fn get_due_tasks(pool: &PgPool, _as_of: NaiveDate) -> Result<Vec<TaskRow>> {
    let rows = sqlx::query(
        r#"
        SELECT 
            t.id as task_id,
            t.contact_id,
            c.vcard_uid,
            c.full_name,
            c.email,
            t.body,
            t.status
        FROM tasks t
        JOIN contacts c ON t.contact_id = c.id
        WHERE t.status = 'pending'
        ORDER BY t.created_at
        "#
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch due tasks")?;
    
    let tasks = rows
        .into_iter()
        .map(|row| {
            let status: String = row.get("status");
            TaskRow {
                task_id: row.get("task_id"),
                contact_id: row.get("contact_id"),
                vcard_uid: row.get("vcard_uid"),
                full_name: row.get("full_name"),
                email: row.get("email"),
                body: row.get("body"),
                status: match status.as_str() {
                    "pending" => TaskStatus::Pending,
                    "done" => TaskStatus::Done,
                    "snoozed" => TaskStatus::Snoozed,
                    _ => TaskStatus::Pending,
                },
            }
        })
        .collect();
    
    Ok(tasks)
}

/// Get reconnects that are due within the given window
pub async fn get_due_reconnects(pool: &PgPool, window_days: u32) -> Result<Vec<ReconnectRow>> {
    let window_end = chrono::Local::now().date_naive() + chrono::Duration::days(window_days as i64);
    
    let rows = sqlx::query(
        r#"
        SELECT 
            r.id as reconnect_id,
            r.contact_id,
            c.vcard_uid,
            c.full_name,
            c.email,
            r.due_date,
            r.original_tag,
            r.status
        FROM reconnects r
        JOIN contacts c ON r.contact_id = c.id
        WHERE r.status = 'pending' AND r.due_date <= $1
        ORDER BY r.due_date
        "#,
    )
    .bind(window_end)
    .fetch_all(pool)
    .await
    .context("Failed to fetch due reconnects")?;
    
    let reconnects = rows
        .into_iter()
        .map(|row| {
            let status: String = row.get("status");
            ReconnectRow {
                reconnect_id: row.get("reconnect_id"),
                contact_id: row.get("contact_id"),
                vcard_uid: row.get("vcard_uid"),
                full_name: row.get("full_name"),
                email: row.get("email"),
                due_date: row.get("due_date"),
                original_tag: row.get("original_tag"),
                status: match status.as_str() {
                    "pending" => ReconnectStatus::Pending,
                    "sent" => ReconnectStatus::Sent,
                    "dismissed" => ReconnectStatus::Dismissed,
                    "deferred" => ReconnectStatus::Deferred,
                    _ => ReconnectStatus::Pending,
                },
            }
        })
        .collect();
    
    Ok(reconnects)
}

/// Mark a reconnect as sent
pub async fn mark_reconnect_sent(pool: &PgPool, id: Uuid) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE reconnects
        SET status = 'sent', sent_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to mark reconnect as sent")?;
    
    Ok(())
}

/// Mark multiple reconnects as sent
pub async fn mark_reconnects_sent_batch(pool: &PgPool, ids: &[Uuid]) -> Result<()> {
    for id in ids {
        mark_reconnect_sent(pool, *id).await?;
    }
    Ok(())
}

/// Log a digest run
pub async fn log_digest(pool: &PgPool, recipient: &str, counts: DigestCounts) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO digest_log (sent_at, recipient, contact_count, task_count, reconnect_count)
        VALUES (NOW(), $1, $2, $3, $4)
        "#,
    )
    .bind(recipient)
    .bind(counts.contact_count)
    .bind(counts.task_count)
    .bind(counts.reconnect_count)
    .execute(pool)
    .await
    .context("Failed to log digest")?;
    
    Ok(())
}
