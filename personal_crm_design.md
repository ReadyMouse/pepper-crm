# Personal CRM Agent — Design Document

> **For:** Claude (via Cursor IDE)
> **Author:** Mylo
> **Date:** May 2026
> **Status:** Prototype phase — local VCF files only, no live CardDAV

---

## What We Are Building

A lightweight personal CRM agent built as a Rust MCP server workspace. It reads contact data from `.vcf` (vCard) files, parses structured tags from the notes field, manages task state in PostgreSQL, and sends a weekly HTML email digest with `.ics` calendar attachments for upcoming follow-ups.

The system runs as a weekly cron job today. It is architected as MCP servers from day one so that a Claude agent can call the same tools directly in the future — with no changes to the servers themselves.

**It does not:** run a persistent web server, expose a public API, or touch a live contact book during prototyping.

**It does:** read local VCF files, write interaction logs back to them, manage task state in Postgres, generate `.ics` files, and send one useful email per week.

---

## Core Design Principles

- **VCF is the people store.** Contact data lives in vCards. The database holds only task state and logs — never duplicates contact fields.
- **Notes field is human-readable first.** Tags are plain text that a human can read and edit in any contacts app. No binary formats, no proprietary fields.
- **Write-back is append-only.** The agent never modifies existing note content. It only appends to a clearly delimited log block below a `--- CRM Log ---` separator.
- **Last tag wins.** If `Reconnect:` appears multiple times in a note, the last one is authoritative. Re-scheduling means appending a new tag, not editing the old one.
- **Dry-run always works.** The `--dry-run` flag must be safe at any time. No emails sent, no DB writes, no VCF modifications.
- **Prototype locally, promote to Pi.** The only change between local prototype and Pi production is the contacts source (local files vs. CardDAV) and the cron schedule.
- **stdio now, HTTP/SSE later.** MCP servers use stdio transport during development. Switching to HTTP/SSE transport makes them persistent daemons callable by a live agent — no tool signatures change.

---

## Prototype Scope (Build This First)

- Read `.vcf` files from a local directory (`./contacts/`)
- Parse `TODO:` and `Reconnect:` tags from the `NOTE` field
- Store task state in PostgreSQL (pending / done / snoozed)
- Generate a weekly HTML email digest
- Attach `.ics` files for reconnect reminders
- Send via SMTP
- Write interaction logs back into VCF `NOTE` fields (local files only for now)
- Generate a set of realistic fake VCF contacts for testing

**Not in prototype scope:**
- CardDAV read/write (stubbed with a `// TODO: CardDAV` comment block)
- HTTP/SSE transport (servers use stdio only)
- AI enrichment
- Matrix bot
- Geo radius search

---

## Cargo Workspace Structure

All crates live in a single Cargo workspace. One shared library (`crm-core`) contains all business logic. Each MCP server is a thin binary crate that wraps `crm-core` functionality as MCP tools. The runner is the MCP client that orchestrates the weekly sequence.

```
personal-crm/
├── Cargo.toml                  # workspace root — lists all members
├── .env.example
├── contacts/                   # local .vcf files for prototyping
│   └── (generated test contacts)
│
├── crm-core/                   # shared library crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── vcard.rs            # VCF parsing and write-back
│       ├── tags.rs             # TODO:/Reconnect: tag extraction
│       ├── db.rs               # sqlx Postgres connection + queries
│       ├── models.rs           # shared structs (Contact, Task, Reconnect)
│       └── ical.rs             # .ics generation helpers
│
├── mcp-vcard-server/               # MCP server binary: read/write VCF
│   ├── Cargo.toml
│   └── src/main.rs
│
├── mcp-scheduler-server/           # MCP server binary: what's due this week
│   ├── Cargo.toml
│   └── src/main.rs
│
├── mcp-digest-server/              # MCP server binary: render HTML email
│   ├── Cargo.toml
│   └── src/main.rs
│
├── mcp-cal-server/                 # MCP server binary: generate .ics files
│   ├── Cargo.toml
│   └── src/main.rs
│
├── mcp-mailer-server/              # MCP server binary: send via SMTP
│   ├── Cargo.toml
│   └── src/main.rs
│
├── mcp-crm-runner/                 # MCP client binary: orchestrates weekly run
│   ├── Cargo.toml
│   └── src/main.rs
│
├── migrations/
│   └── 001_initial.sql         # PostgreSQL schema
├── templates/
│   └── digest.html             # Tera email template
└── tests/
    ├── test_vcard_parsing.rs
    ├── test_tag_extraction.rs
    └── generate_test_contacts.rs  # creates fake .vcf files for testing
```

### Workspace `Cargo.toml`

```toml
[workspace]
members = [
    "crm-core",
    "vcard-server",
    "scheduler-server",
    "digest-server",
    "cal-server",
    "mailer-server",
    "crm-runner",
]
resolver = "2"

[workspace.dependencies]
rmcp        = { version = "0.16", features = ["server", "transport-io"] }
sqlx        = { version = "0.7", features = ["postgres", "runtime-tokio", "chrono", "uuid"] }
tokio       = { version = "1", features = ["full"] }
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
anyhow      = "1"
chrono      = { version = "0.4", features = ["serde"] }
uuid        = { version = "1", features = ["v4", "serde"] }
dotenvy     = "0.15"
tracing     = "0.1"
tracing-subscriber = "0.3"
```

---

## Transport: stdio (prototype) → HTTP/SSE (production)

During prototyping, `crm-runner` spawns each server binary as a **child process** and communicates over **stdin/stdout** using JSON-RPC (the rmcp stdio transport). Each server lives only for the duration of its tool call, then exits. Nothing listens on a port. Nothing needs to be kept alive between runs.

```
cron (Sunday night)
  └── spawns crm-runner
        ├── spawns vcard-server     → parse_vcards() → exits
        ├── spawns scheduler-server → get_due()      → exits
        ├── spawns digest-server    → render_digest() → exits
        ├── spawns cal-server       → export_ics()   → exits
        └── spawns mailer-server    → send_email()   → exits
```

**Upgrading to agent-callable (future):** change the transport in each server's `main.rs` from `stdio()` to `SseServerTransport`. The server becomes a persistent daemon. Tool signatures, business logic, and `crm-core` are all unchanged. `crm-runner` is retired in favour of a Claude agent calling the tools directly.

---

## MCP Tool Surface

Each server exposes one or more `#[tool]`-annotated functions via `rmcp`. These are the callable interface for both the runner today and an agent in the future.

### `vcard-server`

```rust
parse_vcards(dir: String) -> Vec<Contact>
// Scans a directory of .vcf files, returns parsed contacts with tags extracted

log_interaction(uid: String, note: String, new_reconnect_tag: Option<String>) -> bool
// Appends a dated log entry to the contact's NOTE field
// Optionally appends a new Reconnect: tag
// Writes back to the local .vcf file
// TODO: CardDAV — replace fs::write with PUT to Radicale endpoint
```

### `scheduler-server`

```rust
upsert_contacts(contacts: Vec<Contact>) -> UpsertResult
// Syncs parsed contact + tag data into Postgres

get_due(window_days: u32) -> DueItems
// Returns tasks and reconnects due within window_days from today
// Cross-references Postgres status to avoid re-sending already-sent items
```

### `digest-server`

```rust
render_digest(due_items: DueItems, week_of: String) -> String
// Renders the HTML email body using the Tera template
// Returns the HTML string — does not send
```

### `cal-server`

```rust
export_ics(reconnects: Vec<ReconnectItem>) -> Vec<IcsFile>
// Generates one VCALENDAR/VEVENT per reconnect
// Returns (filename, ics_bytes) pairs
```

### `mailer-server`

```rust
send_email(
    recipient: String,
    subject: String,
    html_body: String,
    attachments: Vec<IcsFile>
) -> bool
// Sends via SMTP using credentials from .env
// Attaches each .ics as text/calendar
```

---

## `crm-runner` Sequence

The runner is the MCP client. It calls tools in order, passing output from one as input to the next.

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Spawn vcard-server, call parse_vcards("./contacts")
    // 2. Spawn scheduler-server, call upsert_contacts(contacts)
    // 3. Call get_due(window_days=7) -> due_items
    // 4. Spawn digest-server, call render_digest(due_items, week_of)
    // 5. Spawn cal-server, call export_ics(due_items.reconnects)
    // 6. Spawn mailer-server, call send_email(recipient, subject, html, ics_files)
    // 7. Spawn scheduler-server, call mark_sent(reconnect_ids)
    // 8. Log digest run to Postgres digest_log table

    // --dry-run: skip steps 6, 7, 8 — print HTML to stdout instead
    Ok(())
}
```

---

## VCF Note Field — Tag Format

The `NOTE` field is the only field the agent reads structured data from or writes to. All other vCard fields (name, email, phone, address, org) are read-only from the agent's perspective.

### Tag Syntax

Tags are written one per line. The agent reads the **last occurrence** of a tag.

```
NOTE: Met at Consensus Miami. Works on ZK proofs at Aztec.
TODO: intro to the Zcash core team
TODO: send link to ZCG grant template
Reconnect: 3 months
```

### Supported Tags

| Tag | Format | Behaviour |
|---|---|---|
| `TODO:` | `TODO: free text` | Creates a task linked to this contact |
| `Reconnect:` | `Reconnect: N days/weeks/months` | Schedule a timed follow-up |
| `Reconnect:` | `Reconnect: before [city] trip` | Parse but set status `deferred` — do not schedule yet |

### Interaction Log Write-Back

```
NOTE: Met at Consensus Miami. Works on ZK proofs at Aztec.
TODO: intro to the Zcash core team
Reconnect: 3 months
--- CRM Log ---
2026-05-14: Reconnect email sent re Zcash Kitchen grant. Reset to 6 months.
Reconnect: 6 months
```

Rules:
- `--- CRM Log ---` separator added once, on first write
- Each entry format: `YYYY-MM-DD: [note text]`
- Agent always reads the **last** `Reconnect:` tag in the file
- Agent never modifies content above the separator

---

## PostgreSQL Schema

```sql
-- migrations/001_initial.sql

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE contacts (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    vcard_uid       TEXT UNIQUE NOT NULL,
    full_name       TEXT NOT NULL,
    email           TEXT,
    last_synced_at  TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE tasks (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    contact_id      UUID REFERENCES contacts(id) ON DELETE CASCADE,
    body            TEXT NOT NULL,
    status          TEXT DEFAULT 'pending' CHECK (status IN ('pending','done','snoozed')),
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE reconnects (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    contact_id      UUID REFERENCES contacts(id) ON DELETE CASCADE,
    due_date        DATE NOT NULL,
    status          TEXT DEFAULT 'pending' CHECK (status IN ('pending','sent','dismissed','deferred')),
    original_tag    TEXT,
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    sent_at         TIMESTAMPTZ
);

CREATE TABLE digest_log (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    sent_at         TIMESTAMPTZ DEFAULT NOW(),
    recipient       TEXT,
    contact_count   INTEGER,
    task_count      INTEGER,
    reconnect_count INTEGER
);
```

---

## `crm-core` Module Responsibilities

### `vcard.rs`

Parses `.vcf` files using the `ical` crate (handles vCard 2.1 / 3.0 / 4.0).

```rust
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
```

Notes:
- Strip `--- CRM Log ---` block before parsing tags
- Handle vCard line folding (lines wrapped with leading whitespace)
- Gracefully skip malformed vCards with `tracing::warn!`
- `// TODO: CardDAV` comment marks where `fs::write` becomes a `PUT` request

### `tags.rs`

Pure functions, no I/O.

```rust
pub fn parse_todos(note: &str) -> Vec<String>
pub fn parse_reconnect_tag(note: &str) -> Option<String>
pub fn tag_to_due_date(tag: &str, from: NaiveDate) -> Option<NaiveDate>
pub fn is_city_trigger(tag: &str) -> bool
pub fn append_log_entry(note: &str, entry: &str, new_tag: Option<&str>) -> String
```

`tag_to_due_date` handles:
- `"2 weeks"` → from + 14 days
- `"3 months"` → from + 3 months (chrono month arithmetic)
- `"before NY trip"` → `None` (city trigger, caller sets status to `deferred`)

### `db.rs`

Async wrapper around `sqlx`. Connection pool passed as parameter — no global state.

```rust
pub async fn upsert_contact(pool: &PgPool, contact: &Contact) -> Result<Uuid>
pub async fn upsert_task(pool: &PgPool, contact_id: Uuid, body: &str) -> Result<Uuid>
pub async fn upsert_reconnect(pool: &PgPool, contact_id: Uuid, due_date: NaiveDate, tag: &str) -> Result<Uuid>
pub async fn get_due_tasks(pool: &PgPool, as_of: NaiveDate) -> Result<Vec<TaskRow>>
pub async fn get_due_reconnects(pool: &PgPool, window_days: u32) -> Result<Vec<ReconnectRow>>
pub async fn mark_reconnect_sent(pool: &PgPool, id: Uuid) -> Result<()>
pub async fn log_digest(pool: &PgPool, recipient: &str, counts: DigestCounts) -> Result<()>
```

### `ical.rs`

```rust
pub fn build_ics(reconnect: &ReconnectRow) -> String
// Returns a complete VCALENDAR string for one reconnect event
// SUMMARY: "Follow up: [Full Name]"
// DTSTART: due_date as all-day event
// VALARM: 1 day before
```

---

## Test Contact Generator

`tests/generate_test_contacts.rs` — run with:

```bash
cargo test --test generate_test_contacts -- --ignored
```

Generates 20 `.vcf` files in `./contacts/` using the `fake` crate.

| Scenario | Count |
|---|---|
| No tags at all | 3 |
| TODO: only, no reconnect | 3 |
| Reconnect: only, due this week | 3 |
| Multiple TODOs + Reconnect | 3 |
| `Reconnect: before [city] trip` (deferred) | 2 |
| Already has CRM Log block with prior entries | 2 |
| Reconnect: overdue by 2 weeks | 2 |
| Missing email, phone, org (incomplete record) | 2 |

---

## Key Dependencies

```toml
# crm-core/Cargo.toml (beyond workspace deps)
ical        = "0.10"        # vCard 2.1/3.0/4.0 parser
icalendar   = "0.16"        # .ics generation
tera        = "1"           # email templating
lettre      = "0.11"        # SMTP sending

# dev-dependencies
fake        = "2"           # test data generation
```

---

## Environment Variables

```bash
# .env.example

DATABASE_URL=postgres://crm_user:yourpassword@localhost:5432/personal_crm

SMTP_HOST=smtp.gmail.com
SMTP_PORT=587
SMTP_USER=you@gmail.com
SMTP_PASS=your-app-password

DIGEST_RECIPIENT=you@youremail.com
CONTACTS_DIR=./contacts
RECONNECT_WINDOW_DAYS=7
```

---

## Local Dev Setup

```bash
# 1. Clone and build
git clone [repo]
cd personal-crm
cargo build

# 2. Set up Postgres
createdb personal_crm
psql personal_crm < migrations/001_initial.sql

# 3. Configure
cp .env.example .env
# edit .env with your SMTP credentials and DB password

# 4. Generate test contacts
cargo test --test generate_test_contacts -- --ignored

# 5. Dry run (no email sent, prints digest to stdout)
cargo run -p crm-runner -- --dry-run

# 6. Live run
cargo run -p crm-runner
```

---

## CardDAV Integration (Production — Not Prototype)

When ready to connect to the live Pi CardDAV server (Radicale):

- `vcard.rs`: replace directory scan with HTTP `REPORT` request to Radicale, fetching vCards from `card:address-data` XML elements
- Write-back: replace `fs::write(&vcf_path, updated)` with HTTP `PUT` to `https://[pi-ip]/[user]/contacts/[uid].vcf`
- Auth: HTTP Basic — add `CARDDAV_URL`, `CARDDAV_USER`, `CARDDAV_PASS` to `.env`
- All other crates unchanged

The `// TODO: CardDAV` comment block in `vcard.rs` marks the exact lines to replace.

---

## Upgrade Path: stdio → Agent-Callable

When ready to make the servers callable by a live Claude agent:

1. Add `transport-sse` feature to `rmcp` in each server's `Cargo.toml`
2. In each server `main.rs`, swap:

```rust
// before (stdio — prototype)
let server = MyServer::new().serve(stdio()).await?;

// after (HTTP/SSE — production)
let server = MyServer::new().serve(SseServerTransport::new("0.0.0.0:8080")).await?;
```

3. Add a systemd service for each server on the Pi so they start on boot
4. Retire `crm-runner` — Claude calls the tools directly via MCP
5. Cron becomes a scheduled Claude agent call, or stays as a simple trigger

Tool signatures, `crm-core`, database schema, and VCF tag format are all unchanged.
