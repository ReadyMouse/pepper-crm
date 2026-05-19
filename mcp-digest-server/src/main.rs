use anyhow::Result;
use chrono::Local;
use rmcp::*;
use serde::{Deserialize, Serialize};
use tera::{Context, Tera};
use tracing::info;

#[derive(Debug, Deserialize)]
struct RenderDigestArgs {
    tasks: Vec<TaskItem>,
    reconnects: Vec<ReconnectItem>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TaskItem {
    contact_name: String,
    description: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ReconnectItem {
    contact_name: String,
    due_date: String,
    tag: String,
}

#[derive(Debug, Serialize)]
struct DigestOutput {
    html: String,
    subject: String,
}

async fn handle_render_digest(args: RenderDigestArgs) -> Result<DigestOutput> {
    info!("Rendering digest with {} tasks and {} reconnects", 
          args.tasks.len(), args.reconnects.len());
    
    let mut tera = Tera::new("templates/**/*.html")?;
    tera.autoescape_on(vec!["html"]);
    
    let mut context = Context::new();
    context.insert("tasks", &args.tasks);
    context.insert("reconnects", &args.reconnects);
    context.insert("date", &Local::now().format("%B %d, %Y").to_string());
    context.insert("task_count", &args.tasks.len());
    context.insert("reconnect_count", &args.reconnects.len());
    
    let html = tera.render("digest.html", &context)?;
    
    let subject = if args.tasks.is_empty() && args.reconnects.is_empty() {
        "Pepper CRM: No items this week".to_string()
    } else {
        format!(
            "Pepper CRM: {} task{}, {} reconnect{}",
            args.tasks.len(),
            if args.tasks.len() == 1 { "" } else { "s" },
            args.reconnects.len(),
            if args.reconnects.len() == 1 { "" } else { "s" }
        )
    };
    
    Ok(DigestOutput { html, subject })
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    info!("Starting mcp-digest-server");
    
    let server = Server::new("mcp-digest-server")
        .with_tool(
            "render_digest",
            "Render HTML email digest from tasks and reconnects",
            |args: RenderDigestArgs| async move {
                handle_render_digest(args).await
            },
        );
    
    server.run_stdio().await?;
    
    Ok(())
}
