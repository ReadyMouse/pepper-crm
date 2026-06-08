//! # MCP Mailer Server
//!
//!   MCP server (stdio) that sends HTML emails with optional file attachments via SMTP.
//!
//! INPUT:
//!   - Env: `SMTP_HOST`, `SMTP_PORT`, `SMTP_USERNAME`, `SMTP_PASSWORD`, `SMTP_FROM`
//!   - MCP tool `send_email`: `{ "to", "subject", "html_body", "attachments": [...] }`
//!
//! OUTPUT:
//!   - `send_email` → confirmation string (e.g. `"Email sent to <recipient>"`)
//!
//! NOTES:
//!   - Server name: `mcp-mailer-server`
//!   - Loads `.env` via pepper_crm::load_dotenv on startup
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use lettre::{
    message::{header::ContentType, Attachment, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    Message, SmtpTransport, Transport,
};
use pepper_crm::load_dotenv;
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::stdio,
};
use schemars::JsonSchema;
use serde::Deserialize;
use tracing::info;

#[derive(Debug, Deserialize, JsonSchema)]
struct SendEmailArgs {
    to: String,
    subject: String,
    html_body: String,
    attachments: Vec<EmailAttachment>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct EmailAttachment {
    filename: String,
    content: String,
    content_type: String,
}

#[derive(Clone)]
struct MailerServer {
    tool_router: ToolRouter<Self>,
}

impl MailerServer {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl MailerServer {
    #[tool(description = "Send HTML email with optional .ics attachments via SMTP")]
    fn send_email(
        &self,
        Parameters(args): Parameters<SendEmailArgs>,
    ) -> Result<String, String> {
        info!("Sending email to: {}", args.to);

        let smtp_host = std::env::var("SMTP_HOST").map_err(|e| e.to_string())?;
        let smtp_port: u16 = std::env::var("SMTP_PORT")
            .map_err(|e| e.to_string())?
            .parse::<u16>()
            .map_err(|e| e.to_string())?;
        let smtp_username = std::env::var("SMTP_USERNAME").map_err(|e| e.to_string())?;
        let smtp_password = std::env::var("SMTP_PASSWORD").map_err(|e| e.to_string())?;
        let smtp_from = std::env::var("SMTP_FROM").map_err(|e| e.to_string())?;

        let mut multipart = MultiPart::mixed().singlepart(
            SinglePart::builder()
                .header(ContentType::TEXT_HTML)
                .body(args.html_body),
        );

        for attachment in args.attachments {
            let content_type: ContentType = attachment
                .content_type
                .parse()
                .map_err(|e| format!("Invalid content type: {e}"))?;
            multipart = multipart.singlepart(
                Attachment::new(attachment.filename).body(attachment.content, content_type),
            );
        }

        let email = Message::builder()
            .from(smtp_from.parse().map_err(|e| format!("Invalid from: {e}"))?)
            .to(args.to.parse().map_err(|e| format!("Invalid to: {e}"))?)
            .subject(&args.subject)
            .multipart(multipart)
            .map_err(|e| e.to_string())?;

        let creds = Credentials::new(smtp_username, smtp_password);
        let mailer = SmtpTransport::relay(&smtp_host)
            .map_err(|e| e.to_string())?
            .port(smtp_port)
            .credentials(creds)
            .build();

        mailer.send(&email).map_err(|e| e.to_string())?;

        info!("Email sent successfully: {}", args.subject);
        Ok(format!("Email sent to {}", args.to))
    }
}

#[tool_handler]
impl ServerHandler for MailerServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Send Pepper CRM digest emails via SMTP with optional calendar attachments.".into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    load_dotenv()?;

    info!("Starting mcp-mailer-server");
    let service = MailerServer::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
