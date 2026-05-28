<!--
# Pepper — Repository README

  Top-level project overview, setup, and feature status for the Pepper personal CRM workspace.

INPUT:
  - None (human-facing entry point)

OUTPUT:
  - Setup instructions, architecture summary, links to folder docs

NOTES:
  - See README_*.md files for per-folder detail; DOCUMENTATION_PROGRESS.md tracks doc agent runs.

Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.
-->

# Pepper — Personal CRM Agent

A lightweight personal CRM agent built as a Rust MCP server workspace. It reads contact data from `.vcf` (vCard) files, parses structured tags from notes and categories, manages task state in PostgreSQL, sends a weekly HTML email digest with `.ics` calendar attachments, and surfaces who you should reconnect with based on your travel schedule.

Meet **Pepper** — your friendly personal CRM assistant that helps you stay connected with your network.

## Current Status

### Core library (`pepper-crm`)

Shared business logic for the whole workspace:

| Module | Purpose |
|--------|---------|
| `vcard.rs` | VCF parsing, write-back, geo fields |
| `tags.rs` | `TODO:` / `Reconnect:` extraction and due-date logic |
| `db.rs` | PostgreSQL task state (sqlx) |
| `ical.rs` | `.ics` generation with reminders |
| `calendar.rs` | Google Calendar ICS fetch, next-week trip parsing |
| `geo.rs` | Nominatim geocoding with query cache |
| `contact_geo.rs` | Batch geocode contacts, optional write-back to VCF |
| `travel.rs` | Metro-radius matching for upcoming trips |
| `travel_cache.rs` | Weekly travel snapshot cache |
| `models.rs` | Shared structs |

### MCP servers (stdio transport)

Thin binaries that expose `pepper-crm` as MCP tools for agents and the weekly runner:

| Server | Tools |
|--------|-------|
| `mcp-vcard-server` | `parse_vcards`, `log_interaction` |
| `mcp-scheduler-server` | `upsert_contacts`, `get_due` |
| `mcp-digest-server` | `render_digest` |
| `mcp-cal-server` | `export_ics` |
| `mcp-mailer-server` | `send_email` |
| `mcp-calendar-server` | `get_upcoming_travel` |
| `mcp-travel-server` | `build_travel_week`, `get_travel_week` |

### Weekly orchestrator (`pepper`)

The `pepper` binary spawns MCP servers over stdio and runs the full weekly flow:

1. Parse VCF contacts
2. Sync tasks/reconnects to PostgreSQL
3. Query due items
4. Render HTML digest
5. Generate `.ics` attachments
6. Send email (or dry-run)
7. Build travel match snapshot (once per week, if calendar is configured)

### Web dashboard (`pepper-web`)

Localhost UI at **http://localhost:3000**:

- **Dashboard** (`/`) — Pending tasks, reconnects due, next-week travel matches
- **Digest Preview** (`/preview`) — Live preview of the weekly email

Dashboard features that are live today:

- VCF → PostgreSQL sync on startup
- Pending `TODO:` tasks from the database
- Reconnects due within 7 days (from VCF `CATEGORIES` / `REV` / note anchors)
- **Next Week Travel** — calendar + geocoding + metro-radius matching
- Snooze reconnect intervals (writes back to VCF, removes from travel list)
- On-demand travel refresh with configurable metro radius

Coming soon: Random Person of the Week (see [`DASHBOARD_SECTIONS.md`](DASHBOARD_SECTIONS.md)).

## Project Structure

```
pepper-crm/
├── Cargo.toml                  # workspace root
├── .env.example                # environment variables template
├── contacts/                   # local .vcf files (test + real exports)
├── pepper-crm/                 # core library
│   ├── src/
│   │   ├── vcard.rs
│   │   ├── tags.rs
│   │   ├── db.rs
│   │   ├── ical.rs
│   │   ├── calendar.rs
│   │   ├── geo.rs
│   │   ├── contact_geo.rs
│   │   ├── travel.rs
│   │   └── travel_cache.rs
│   ├── examples/               # list_due_reconnects, test_calendar, etc.
│   └── tests/
├── mcp-vcard-server/
├── mcp-scheduler-server/
├── mcp-digest-server/
├── mcp-cal-server/
├── mcp-mailer-server/
├── mcp-calendar-server/
├── mcp-travel-server/
├── pepper/                     # weekly orchestrator CLI
├── pepper-web/                 # web dashboard
├── assets/brand/               # pepper avatars
├── templates/                  # email digest template
├── migrations/
│   └── 001_initial.sql
└── .cache/                     # geocode + travel snapshots (gitignored)
```

## Setup

### 1. Install dependencies

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# PostgreSQL (macOS)
brew install postgresql@16
brew services start postgresql@16
```

### 2. Database

```bash
createdb pepper_crm
psql pepper_crm < migrations/001_initial.sql
```

### 3. Environment

```bash
cp .env.example .env
```

Edit `.env`:

| Variable | Purpose |
|----------|---------|
| `DATABASE_URL` | PostgreSQL connection (use your macOS username from `whoami`) |
| `CONTACTS_DIR` | Path to `.vcf` files (default `./contacts`) |
| `CACHE_DIR` | Geocode + travel cache (default `.cache`) |
| `SMTP_*` / `DIGEST_RECIPIENT` | Weekly email delivery |
| `GOOGLE_CALENDAR_ICS_URL` | Secret ICS link for travel matching |
| `NOMINATIM_USER_AGENT` | Required for geocoding (include your email) |
| `GEO_WRITE_TO_VCF` | Write lat/lng back to vCards after geocoding (default on) |

### 4. Build

```bash
cargo build --workspace
```

### 5. Sync contacts and preview

```bash
# Sync VCF → DB and preview digest (no email sent)
./target/debug/pepper --dry-run

# Start the dashboard
cargo run --bin pepper-web
```

Open **http://localhost:3000**. For travel matches, set `GOOGLE_CALENDAR_ICS_URL`, then click **Refresh travel matches** on the dashboard.

### 6. Send the weekly digest

```bash
./target/debug/pepper
# or dry-run first:
./target/debug/pepper --dry-run --recipient you@example.com
```

Flags:

- `--dry-run` — no email, no side effects beyond sync
- `--recipient` — override `DIGEST_RECIPIENT`
- `--force-travel` — rebuild travel snapshot even if one exists for next week
- `--contacts-dir` — override `CONTACTS_DIR`

## Tag Format

Tags live in human-readable vCard fields. You can edit them in any contacts app.

### `TODO:` (in `NOTE`)

One per line, above the CRM log separator:

```
July 2026: Met at conference. Works on crypto.

TODO: send intro email
TODO: share grant template
```

### `Reconnect:` (in `CATEGORIES`)

Reconnect scheduling lives in **vCard categories**, not in notes:

```
CATEGORIES:Reconnect: 3 months
```

Supported values:

- **Timed intervals** — `1 week`, `3 months`, `1 year`, etc.
- **Trip triggers** — `before Chicago trip` (matched when you travel there)
- **No timed reconnect** — `Reconnect: Never` (see [Engagement categories](#engagement-categories-in-categories) below)

Legacy `Reconnect:` lines in `NOTE` are still read as a fallback.

Due dates anchor from vCard `REV` or the latest `Month YYYY:` note line (e.g. `May 2026: Had coffee`).

### Engagement categories (in `CATEGORIES`)

Two category values control **where** a contact may surface. They are separate from timed `Reconnect:` tags (a card may have both).

#### `Reconnect: Never`

People you stay close to without needing a “text them soon” nudge — e.g. family, partners, daily colleagues. You do **not** want interval-based reconnect reminders for them.

```
CATEGORIES:Reconnect: Never
```

| Surface | Included? |
|---------|-----------|
| Reconnects Due | No |
| Next Week Travel | No |
| Random Person of the Week | Yes |
| Birthday reminders (planned) | Yes |
| General contact search / browse | Yes |

#### `Do Not Engage`

People you never want suggested or surfaced, but you still want the vCard on file (not deleted).

```
CATEGORIES:Do Not Engage
```

| Surface | Included? |
|---------|-----------|
| Reconnects Due | No |
| Next Week Travel | No |
| Random Person of the Week | No |
| Birthday reminders | No |
| Any Pepper search or suggestion list | No |

The contact remains in your VCF export; Pepper simply omits them from all proactive surfaces.

### CRM log (append-only)

After interactions, Pepper appends below a separator — existing note content is never modified:

```
--- CRM Log ---
2026-05-14: Sent follow-up email.
```

## Test Contacts

The `contacts/` directory includes generated test VCFs covering common scenarios:

| Files | Scenario |
|-------|----------|
| `contact_01`–`03` | No tags (baseline) |
| `contact_04`–`06` | TODO only |
| `contact_07`–`09` | Reconnect due this week |
| `contact_10`–`12` | Multiple TODOs + Reconnect |
| `contact_13`–`14` | City/trip triggers |
| `contact_15`–`16` | Existing CRM log blocks |
| `contact_17`–`18` | Overdue reconnects |
| `contact_19`–`20` | Incomplete records |

Regenerate test contacts:

```bash
cargo test -p pepper-crm --test generate_test_contacts -- --ignored
```

## Design Principles

- **VCF is the people store** — contact data lives in vCards; PostgreSQL holds task state only
- **Notes field is human-readable first** — plain text tags, no binary formats
- **Write-back is append-only** — never modifies existing content, only appends
- **Last tag wins** — most recent `Reconnect:` is authoritative
- **Dry-run always works** — safe testing without side effects
- **Prototype locally, promote to Pi** — local VCF files by default; set `CARDDAV_*` for Radicale on Pi
- **stdio now, HTTP/SSE later** — easy development, smooth upgrade path for agents

## Documentation

| Doc | Description |
|-----|-------------|
| [`DOCUMENTATION_PROGRESS.md`](DOCUMENTATION_PROGRESS.md) | Doc-agent checklist (file headers + progress) |
| [`README_pepper-crm.md`](README_pepper-crm.md) | Core library crate |
| [`README_pepper-web.md`](README_pepper-web.md) | Web dashboard |
| [`README_pepper.md`](README_pepper.md) | Weekly orchestrator CLI |
| [`README_mcp-servers.md`](README_mcp-servers.md) | MCP server binaries |
| [`README_contacts.md`](README_contacts.md) | VCF fixtures (no inline headers — parser-safe) |
| [`README_assets.md`](README_assets.md) | Brand images |
| [`README_migrations.md`](README_migrations.md) | PostgreSQL schema |
| [`README_templates.md`](README_templates.md) | Email templates |
| [`personal_crm_design.md`](personal_crm_design.md) | Full design document |
| [`IMPLEMENTATION_STATUS.md`](IMPLEMENTATION_STATUS.md) | Built vs. planned |
| [`DASHBOARD_SECTIONS.md`](DASHBOARD_SECTIONS.md) | Dashboard product spec |
| [`NEXT_WEEK_TRAVEL_BUILD.md`](NEXT_WEEK_TRAVEL_BUILD.md) | Travel matching implementation |

Source files include a standard header block (Rust `//!`, HTML/CSS/JS comments, etc.) describing purpose, inputs, outputs, and notes.

## CardDAV (Radicale on Pi)

When `CARDDAV_URL`, `CARDDAV_USER`, and `CARDDAV_PASS` are set, Pepper loads contacts with a CardDAV `addressbook-query` REPORT and writes changes with HTTP PUT (Done buttons, snooze, geocode writeback, weekly pipeline). Local `CONTACTS_DIR` is ignored for reads in that mode.

```bash
# .env
CARDDAV_URL=https://your-pi.tailnet:5232/alice/contacts/
CARDDAV_USER=alice
CARDDAV_PASS=secret
# CARDDAV_INSECURE=true   # self-signed TLS on homelab

cargo run -p pepper-crm --example carddav_list
cargo run -p pepper-web
```

Pepper PUT → Radicale stores `.vcf` → DAVx5 syncs to phone.

## Raspberry Pi binaries (cross-compile from M3 Mac)

Mac and Pi 5 are both ARM64, but you need **Linux** binaries for the Pi — `cargo build` on macOS produces macOS binaries only.

```bash
brew install messense/macos-cross-toolchains/aarch64-unknown-linux-gnu
rustup target add aarch64-unknown-linux-gnu
./scripts/build-linux-arm64.sh
```

Copy `target/aarch64-unknown-linux-gnu/release/pepper` and `pepper-web` to the Pi. Alternatively, clone the repo on the Pi and `cargo build --release` there.

## What's Next

- Random Person of the Week + web enrichment
- Matrix bot ("chat with Pepper")
- HTTP/SSE transport for persistent MCP daemons
- Digest includes travel section

See [`personal_crm_design.md`](personal_crm_design.md) for the full design doc and [`NEXT_WEEK_TRAVEL_BUILD.md`](NEXT_WEEK_TRAVEL_BUILD.md) for travel matching details.

## License

MIT
