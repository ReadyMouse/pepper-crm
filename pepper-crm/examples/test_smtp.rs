//! # SMTP Test Example
//!
//!   Sends a short HTML test email using SMTP settings from `.env`.
//!
//! INPUT:
//!   - `DIGEST_RECIPIENT`, `SMTP_*` env vars (via `load_dotenv`).
//!
//! OUTPUT:
//!   - One test email delivered to `DIGEST_RECIPIENT`.
//!
//! NOTES:
//!   - Run: `cargo run -q -p pepper-crm --example test_smtp`
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use anyhow::{Context, Result};
use pepper_crm::{load_dotenv, send_html_email};

fn main() -> Result<()> {
    load_dotenv()?;

    let recipient = std::env::var("DIGEST_RECIPIENT").context("DIGEST_RECIPIENT not set in .env")?;

    let html_body = r#"<!DOCTYPE html>
<html><body style="font-family: sans-serif; color: #2c3333;">
<p><strong>Pepper SMTP test</strong></p>
<p>If you can read this, SMTP is configured correctly.</p>
</body></html>"#;

    eprintln!("Sending test message…");
    send_html_email(
        &recipient,
        "Pepper CRM — SMTP test",
        html_body,
        &[],
    )?;

    println!("Test email sent to {recipient}");
    Ok(())
}
