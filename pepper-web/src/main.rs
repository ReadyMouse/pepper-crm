use anyhow::{Context, Result};
use axum::{
    extract::State,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use chrono::Local;
use pepper_crm::{get_due_reconnects, get_due_tasks, parse_vcards_from_dir, upsert_contacts_batch};
use serde::Serialize;
use sqlx::{PgPool, Row};
use std::{path::PathBuf, sync::Arc};
use tera::{Context as TeraContext, Tera};
use tower_http::trace::TraceLayer;
use tracing::info;

const RECONNECT_WINDOW_DAYS: u32 = 7;

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    tera: Tera,
}

#[derive(Serialize)]
struct TaskView {
    contact_name: String,
    description: String,
}

#[derive(Serialize)]
struct ReconnectView {
    contact_name: String,
    due_date: String,
    tag: String,
}

#[derive(Serialize)]
struct ContactView {
    uid: String,
    full_name: String,
    email: Option<String>,
}

fn task_views(tasks: Vec<pepper_crm::TaskRow>) -> Vec<TaskView> {
    tasks
        .into_iter()
        .map(|t| TaskView {
            contact_name: t.full_name,
            description: t.body,
        })
        .collect()
}

fn reconnect_views(reconnects: Vec<pepper_crm::ReconnectRow>) -> Vec<ReconnectView> {
    reconnects
        .into_iter()
        .map(|r| ReconnectView {
            contact_name: r.full_name,
            due_date: r.due_date.to_string(),
            tag: r.original_tag,
        })
        .collect()
}

async fn sync_contacts_from_vcf(pool: &PgPool) -> Result<()> {
    let contacts_dir = std::env::var("CONTACTS_DIR").unwrap_or_else(|_| "./contacts".to_string());
    let path = PathBuf::from(&contacts_dir);

    if !path.exists() {
        info!("Contacts directory {} not found, skipping VCF sync", contacts_dir);
        return Ok(());
    }

    let contacts = parse_vcards_from_dir(&path)
        .with_context(|| format!("Failed to parse VCF files in {}", contacts_dir))?;

    if contacts.is_empty() {
        info!("No VCF contacts found in {}", contacts_dir);
        return Ok(());
    }

    let result = upsert_contacts_batch(pool, &contacts).await?;
    info!(
        "Synced {} contacts ({} tasks, {} reconnects) from {}",
        result.contacts_upserted, result.tasks_created, result.reconnects_created, contacts_dir
    );

    Ok(())
}

async fn fetch_due(pool: &PgPool) -> Result<(Vec<TaskView>, Vec<ReconnectView>)> {
    let today = Local::now().date_naive();
    let tasks = get_due_tasks(pool, today).await?;
    let reconnects = get_due_reconnects(pool, RECONNECT_WINDOW_DAYS).await?;
    Ok((task_views(tasks), reconnect_views(reconnects)))
}

async fn fetch_all_contacts(pool: &PgPool) -> Result<Vec<ContactView>> {
    let rows = sqlx::query(
        r#"
        SELECT vcard_uid, full_name, email
        FROM contacts
        ORDER BY full_name
        "#,
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch contacts")?;

    Ok(rows
        .into_iter()
        .map(|row| ContactView {
            uid: row.get("vcard_uid"),
            full_name: row.get("full_name"),
            email: row.get("email"),
        })
        .collect())
}

async fn index(State(state): State<Arc<AppState>>) -> Response {
    match render_dashboard(&state).await {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("Error rendering dashboard: {}", e);
            Html(format!("<h1>Error</h1><p>{}</p>", e)).into_response()
        }
    }
}

async fn render_dashboard(state: &AppState) -> Result<String> {
    let (tasks, reconnects) = fetch_due(&state.pool).await?;

    let mut context = TeraContext::new();
    context.insert("tasks_count", &tasks.len());
    context.insert("reconnects_count", &reconnects.len());
    context.insert("tasks", &tasks);
    context.insert("reconnects", &reconnects);
    context.insert("date", &Local::now().format("%B %d, %Y").to_string());

    Ok(state.tera.render("dashboard.html", &context)?)
}

async fn contacts_page(State(state): State<Arc<AppState>>) -> Response {
    match render_contacts(&state).await {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("Error rendering contacts: {}", e);
            Html(format!("<h1>Error</h1><p>{}</p>", e)).into_response()
        }
    }
}

async fn render_contacts(state: &AppState) -> Result<String> {
    let contacts = fetch_all_contacts(&state.pool).await?;

    let mut context = TeraContext::new();
    context.insert("contacts", &contacts);
    context.insert("date", &Local::now().format("%B %d, %Y").to_string());

    Ok(state.tera.render("contacts.html", &context)?)
}

async fn digest_preview(State(state): State<Arc<AppState>>) -> Response {
    match render_digest_preview(&state).await {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("Error rendering digest preview: {}", e);
            Html(format!("<h1>Error</h1><p>{}</p>", e)).into_response()
        }
    }
}

async fn render_digest_preview(state: &AppState) -> Result<String> {
    let (tasks, reconnects) = fetch_due(&state.pool).await?;

    let mut context = TeraContext::new();
    context.insert("tasks", &tasks);
    context.insert("reconnects", &reconnects);
    context.insert("date", &Local::now().format("%B %d, %Y").to_string());
    context.insert("task_count", &tasks.len());
    context.insert("reconnect_count", &reconnects.len());

    Ok(state.tera.render("digest.html", &context)?)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    info!("Starting Pepper Web Dashboard...");

    let database_url =
        std::env::var("DATABASE_URL").context("DATABASE_URL must be set in .env")?;

    let pool = PgPool::connect(&database_url).await?;
    info!("Connected to database");

    sync_contacts_from_vcf(&pool).await?;

    let mut tera = Tera::new("pepper-web/templates/**/*.html")?;
    tera.autoescape_on(vec!["html"]);
    info!("Loaded templates");

    let state = Arc::new(AppState { pool, tera });

    let app = Router::new()
        .route("/", get(index))
        .route("/contacts", get(contacts_page))
        .route("/preview", get(digest_preview))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = "127.0.0.1:3000";
    info!("Server running at http://{}", addr);
    info!("   Dashboard:       http://{}/", addr);
    info!("   Contacts:        http://{}/contacts", addr);
    info!("   Digest Preview:  http://{}/preview", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
