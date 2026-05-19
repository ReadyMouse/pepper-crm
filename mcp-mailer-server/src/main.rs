use anyhow::Result;
use lettre::{
    message::{header::ContentType, Attachment, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    Message, SmtpTransport, Transport,
};
use rmcp::*;
use serde::Deserialize;
use tracing::info;

#[derive(Debug, Deserialize)]
struct SendEmailArgs {
    to: String,
    subject: String,
    html_body: String,
    attachments: Vec<EmailAttachment>,
}

#[derive(Debug, Deserialize)]
struct EmailAttachment {
    filename: String,
    content: String,
    content_type: String,
}

async fn handle_send_email(args: SendEmailArgs) -> Result<String> {
    info!("Sending email to: {}", args.to);
    
    // Load SMTP config from environment
    let smtp_host = std::env::var("SMTP_HOST")?;
    let smtp_port: u16 = std::env::var("SMTP_PORT")?.parse()?;
    let smtp_username = std::env::var("SMTP_USERNAME")?;
    let smtp_password = std::env::var("SMTP_PASSWORD")?;
    let smtp_from = std::env::var("SMTP_FROM")?;
    
    // Build multipart message
    let mut multipart = MultiPart::mixed().singlepart(
        SinglePart::builder()
            .header(ContentType::TEXT_HTML)
            .body(args.html_body),
    );
    
    // Add attachments
    for attachment in args.attachments {
        let content_type: ContentType = attachment.content_type.parse()?;
        multipart = multipart.singlepart(
            Attachment::new(attachment.filename)
                .body(attachment.content, content_type)
        );
    }
    
    // Build message
    let email = Message::builder()
        .from(smtp_from.parse()?)
        .to(args.to.parse()?)
        .subject(&args.subject)
        .multipart(multipart)?;
    
    // Send email
    let creds = Credentials::new(smtp_username, smtp_password);
    let mailer = SmtpTransport::relay(&smtp_host)?
        .port(smtp_port)
        .credentials(creds)
        .build();
    
    mailer.send(&email)?;
    
    info!("Email sent successfully: {}", args.subject);
    
    Ok(format!("Email sent to {}", args.to))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();
    
    info!("Starting mcp-mailer-server");
    
    let server = Server::new("mcp-mailer-server")
        .with_tool(
            "send_email",
            "Send HTML email with optional .ics attachments via SMTP",
            |args: SendEmailArgs| async move {
                handle_send_email(args).await
            },
        );
    
    server.run_stdio().await?;
    
    Ok(())
}
