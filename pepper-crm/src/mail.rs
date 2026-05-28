//! # Digest Email Delivery
//!
//!   Sends HTML digest emails with optional `.ics` attachments via SMTP (env-configured).
//!
//! INPUT: recipient, subject, HTML body, optional `IcsFile` attachments; `SMTP_*` env vars.
//! OUTPUT: Sends multipart email through Gmail or other SMTP relay.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use crate::models::IcsFile;
use anyhow::{Context, Result};
use lettre::{
    message::{header::ContentType, Attachment, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    Message, SmtpTransport, Transport,
};
use std::time::Duration;

/// Load `.env` from the current directory or by walking up to find one.
pub fn load_dotenv() -> Result<()> {
    if dotenvy::dotenv().is_ok() {
        return Ok(());
    }
    let mut dir = std::env::current_dir().context("Could not read current directory")?;
    for _ in 0..6 {
        let candidate = dir.join(".env");
        if candidate.is_file() {
            if dotenvy::from_path(&candidate).is_ok() {
                return Ok(());
            }
            anyhow::bail!(
                "Could not parse {}. App passwords with spaces must be quoted: SMTP_PASSWORD=\"xxxx xxxx xxxx xxxx\"",
                candidate.display()
            );
        }
        if !dir.pop() {
            break;
        }
    }
    anyhow::bail!("Could not find .env in current directory or parents")
}

/// Send an HTML email with optional calendar attachments.
pub fn send_html_email(
    to: &str,
    subject: &str,
    html_body: &str,
    attachments: &[IcsFile],
) -> Result<()> {
    let smtp_host = std::env::var("SMTP_HOST").context("SMTP_HOST not set")?;
    let smtp_port: u16 = std::env::var("SMTP_PORT")
        .context("SMTP_PORT not set")?
        .parse()
        .context("SMTP_PORT must be a number")?;
    let smtp_username = std::env::var("SMTP_USERNAME").context("SMTP_USERNAME not set")?;
    let smtp_password = std::env::var("SMTP_PASSWORD").context("SMTP_PASSWORD not set")?;
    let smtp_from = std::env::var("SMTP_FROM").context("SMTP_FROM not set")?;

    let mut multipart = MultiPart::mixed().singlepart(
        SinglePart::builder()
            .header(ContentType::TEXT_HTML)
            .body(html_body.to_string()),
    );

    for attachment in attachments {
        multipart = multipart.singlepart(
            Attachment::new(attachment.filename.clone())
                .body(attachment.content.clone(), ContentType::parse("text/calendar").unwrap()),
        );
    }

    let email = Message::builder()
        .from(smtp_from.parse().context("Invalid SMTP_FROM")?)
        .to(to.parse().context("Invalid recipient address")?)
        .subject(subject)
        .multipart(multipart)?;

    let creds = Credentials::new(smtp_username, smtp_password);
    let mailer = SmtpTransport::starttls_relay(&smtp_host)
        .with_context(|| format!("Could not connect to SMTP relay {smtp_host}"))?
        .port(smtp_port)
        .timeout(Some(Duration::from_secs(30)))
        .credentials(creds)
        .build();

    mailer
        .send(&email)
        .context("SMTP send failed — check username, app password, and host/port")?;

    Ok(())
}
