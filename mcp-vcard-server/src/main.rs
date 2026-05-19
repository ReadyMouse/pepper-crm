use anyhow::Result;
use pepper_crm::{parse_vcard, parse_vcards_from_dir, log_interaction, Contact};
use rmcp::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

#[derive(Debug, Serialize)]
struct ContactSummary {
    uid: String,
    full_name: String,
    email: Option<String>,
    phone: Option<String>,
    org: Option<String>,
    city: Option<String>,
    country: Option<String>,
    note_raw: String,
    todos: Vec<String>,
    reconnect_tag: Option<String>,
    vcf_path: String,
}

impl From<Contact> for ContactSummary {
    fn from(c: Contact) -> Self {
        Self {
            uid: c.uid,
            full_name: c.full_name,
            email: c.email,
            phone: c.phone,
            org: c.org,
            city: c.city,
            country: c.country,
            note_raw: c.note_raw,
            todos: c.todos,
            reconnect_tag: c.reconnect_tag,
            vcf_path: c.vcf_path.display().to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ParseVcardsArgs {
    directory: String,
}

#[derive(Debug, Deserialize)]
struct LogInteractionArgs {
    vcf_path: String,
    note: String,
    new_reconnect_tag: Option<String>,
}

async fn handle_parse_vcards(args: ParseVcardsArgs) -> Result<Vec<ContactSummary>> {
    info!("Parsing VCF files from: {}", args.directory);
    
    let dir = PathBuf::from(args.directory);
    let contacts = parse_vcards_from_dir(&dir)?;
    
    info!("Parsed {} contacts", contacts.len());
    
    Ok(contacts.into_iter().map(ContactSummary::from).collect())
}

async fn handle_log_interaction(args: LogInteractionArgs) -> Result<String> {
    info!("Logging interaction to: {}", args.vcf_path);
    
    let vcf_path = PathBuf::from(&args.vcf_path);
    let contact = parse_vcard(&vcf_path)?;
    
    log_interaction(
        &contact,
        &args.note,
        args.new_reconnect_tag.as_deref(),
    )?;
    
    Ok(format!("Logged interaction to {}", contact.full_name))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    info!("Starting mcp-vcard-server");
    
    let server = Server::new("mcp-vcard-server")
        .with_tool(
            "parse_vcards",
            "Parse all VCF files from a directory",
            |args: ParseVcardsArgs| async move {
                handle_parse_vcards(args).await
            },
        )
        .with_tool(
            "log_interaction",
            "Log an interaction to a contact's VCF file (append-only CRM log)",
            |args: LogInteractionArgs| async move {
                handle_log_interaction(args).await
            },
        );
    
    server.run_stdio().await?;
    
    Ok(())
}
