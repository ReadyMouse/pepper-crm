<!--
# Pepper — Technical README

  Implementation details, deployment, and developer reference for the Pepper CRM workspace.

INPUT:
  - None (developer-facing)

OUTPUT:
  - Architecture, env vars, CardDAV/Vagrant/Pi setup, tag spec, doc index

NOTES:
  - User-facing overview lives in README.md.

Written by Cursor for Ready Mouse and Pepper CRM. June 2026. All rights reserved.
-->

# Pepper — Technical Reference

Developer and deployment guide. For a short overview and quick start, see [`README.md`](README.md).

## Architecture

Rust workspace: shared `pepper-crm` library, MCP server binaries, `pepper` weekly CLI, and `pepper-web` dashboard.

### Production (Raspberry Pi + Radicale)

```
Phone Contacts  ↔  DAVx⁵ / CardDAV client  ↔  Radicale (.vcf files)  ↔  Pepper (pepper + pepper-web)
```

| Piece | Role |
|-------|------|
| **Radicale** | CardDAV address book on the Pi (default port **5232**; often via Tailscale) |
| **DAVx⁵** | Two-way sync between Radicale and the phone Contacts app |
| **Pepper** | CardDAV `addressbook-query` **REPORT** reads; HTTP **PUT** writes (snooze, geocode, task done; CRM log via agent — planned) |
| **`pepper` cron** | Hourly `pepper --send-if-due` → Monday 6:00 weekly digest |

Local dev uses `./contacts/` VCF files instead — same tag format, no server.

### Core library (`pepper-crm`)

| Module | Purpose |
|--------|---------|
| `vcard.rs` | VCF parsing, write-back, geo fields, CardDAV integration |
| `carddav.rs` | CardDAV REPORT/PUT for Radicale |
| `tags.rs` | `TODO:` / `Reconnect:` extraction and due-date logic |
| `tasks.rs` | Pending TODOs from vCard NOTE fields |
| `ical.rs` | `.ics` generation with reminders |
| `calendar.rs` | Google Calendar ICS fetch, next-week trip parsing |
| `digest.rs` | Weekly email HTML via Tera |
| `digest_schedule.rs` | Monday 6:00 send window by trip timezone |
| `mail.rs` | SMTP delivery |
| `weekly.rs` | End-to-end weekly digest pipeline |
| `geo.rs` | Nominatim geocoding with query cache |
| `contact_geo.rs` | Batch geocode contacts, optional write-back to VCF |
| `travel.rs` | Metro-radius matching for upcoming trips |
| `travel_cache.rs` | Weekly travel snapshot cache |
| `birthdays.rs` | Upcoming birthday window |
| `random_pick.rs` | Weekly random contact spotlight |
| `data_enrichment.rs` | Address-fix picks for dashboard enrichment |
| `models.rs` | Shared structs |

### MCP servers (stdio transport)

| Server | Tools |
|--------|-------|
| `mcp-vcard-server` | `parse_vcards` (today); `log_interaction` (agent workflow — planned) |
| `mcp-digest-server` | `render_digest` |
| `mcp-cal-server` | `export_ics` |
| `mcp-mailer-server` | `send_email` |
| `mcp-calendar-server` | `get_upcoming_travel` |
| `mcp-travel-server` | `build_travel_week`, `get_travel_week` |

The weekly `pepper` CLI calls the library directly; MCP servers are for agent workflows.

### Weekly orchestrator (`pepper`)

1. Parse VCF contacts
2. Collect due tasks and reconnects from vCard tags
3. Render HTML digest
4. Generate `.ics` attachments
5. Send email (or dry-run)
6. Build travel match snapshot (once per week, if calendar is configured)

### Web dashboard (`pepper-web`)

Localhost UI at **http://localhost:3000**:

- **Dashboard** (`/`) — tasks, reconnects due, random picks, enrichment, birthdays, travel matches
- **Digest Preview** (`/preview`) — live weekly email preview

## Project structure

```
pepper-crm/
├── Cargo.toml
├── .env.example
├── contacts/                   # local .vcf files
├── pepper-crm/                 # core library (README_pepper-crm.md)
├── mcp-vcard-server/
├── mcp-digest-server/
├── mcp-cal-server/
├── mcp-mailer-server/
├── mcp-calendar-server/
├── mcp-travel-server/
├── mcp-servers/                # MCP overview (README_mcp-servers.md)
├── pepper/                     # weekly CLI
├── pepper-web/                 # dashboard
├── scripts/                    # Pi cross-compile + cron
├── assets/brand/
├── templates/
├── Vagrantfile
├── provision/
├── tests/data/radicale/
└── .cache/                     # geocode + travel snapshots (gitignored)
```

## Setup

### Dependencies

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
cp .env.example .env
cargo build --workspace
```

### Environment variables

| Variable | Purpose |
|----------|---------|
| `CONTACTS_DIR` | Path to `.vcf` files (default `./contacts`) |
| `CACHE_DIR` | Geocode + travel cache (default `.cache`) |
| `SMTP_*` / `DIGEST_RECIPIENT` | Weekly email delivery |
| `GOOGLE_CALENDAR_ICS_URL` | Secret ICS link for travel matching |
| `NOMINATIM_USER_AGENT` | Required for geocoding (include your email) |
| `GEO_WRITE_TO_VCF` | Write lat/lng back to vCards after geocoding (default on) |
| `PEPPER_DASHBOARD_URL` | Link in weekly digest email (default `http://127.0.0.1:3000`) |
| `CARDDAV_*` | Optional — Radicale instead of `CONTACTS_DIR` |

### `pepper` CLI flags

- `--dry-run` — preview digest in logs; no email sent
- `--recipient` — override `DIGEST_RECIPIENT`
- `--force-travel` — rebuild travel snapshot even if one exists for next week
- `--contacts-dir` — override `CONTACTS_DIR`
- `--send-if-due` — only send when Monday 6:00 trip-timezone window is active (cron)
- `--schedule-status` — print digest timezone / send window and exit

## Tag format (full spec)

Tags live in human-readable vCard fields, editable in any contacts app.

### `TODO:` (in `NOTE`)

One per line:

```
July 2026: Met at conference.

TODO: send intro email
TODO: share grant template
```

### `Reconnect:` (in `CATEGORIES`)

```
CATEGORIES:Reconnect: 3 months
```

Supported values:

- **Timed intervals** — `1 week`, `3 months`, `1 year`, etc.
- **Trip triggers** — `before Chicago trip`
- **No timed reconnect** — `Reconnect: Never` (see below)

Legacy `Reconnect:` lines in `NOTE` are still read as a fallback.

Due dates anchor from vCard `REV` or the latest `Month YYYY:` note line (e.g. `May 2026: Had coffee`).

### Engagement categories

#### `Reconnect: Never`

Close contacts without interval nudges (family, partners, daily colleagues).

| Surface | Included? |
|---------|-----------|
| Reconnects Due | No |
| Next Week Travel | No |
| Random Person of the Week | Yes |
| Birthday reminders | Yes |

#### `Do Not Engage`

Keep the vCard on file; omit from all proactive surfaces.

| Surface | Included? |
|---------|-----------|
| Reconnects Due | No |
| Next Week Travel | No |
| Random Person of the Week | No |
| Birthday reminders | No |
| Any Pepper suggestion list | No |

### CRM log (planned — agent-driven)

> **Not wired into the app yet.** `log_interaction` exists in the library and `mcp-vcard-server`, but the weekly CLI and dashboard do not call it. Intended for agents or a future Matrix bot.

When enabled, entries append below `--- CRM Log ---`; the code prefixes today's date:

```
--- CRM Log ---
2026-05-14: Sent follow-up email.
```

Fixtures: `contact_15`–`16` in `contacts/`.

## Test contacts

| Files | Scenario |
|-------|----------|
| `contact_01`–`03` | No tags |
| `contact_04`–`06` | TODO only |
| `contact_07`–`09` | Reconnect due this week |
| `contact_10`–`12` | Multiple TODOs + Reconnect |
| `contact_13`–`14` | City/trip triggers |
| `contact_15`–`16` | CRM log blocks |
| `contact_17`–`18` | Overdue reconnects |
| `contact_19`–`20` | Incomplete records |

```bash
cargo test -p pepper-crm --test generate_test_contacts -- --ignored
```

## CardDAV (Radicale on Pi)

When `CARDDAV_URL`, `CARDDAV_USER`, and `CARDDAV_PASS` are set, Pepper loads contacts via CardDAV `addressbook-query` REPORT and writes with HTTP PUT. `CONTACTS_DIR` is ignored for reads.

```bash
CARDDAV_URL=https://your-pi.tailnet:5232/alice/contacts/
CARDDAV_USER=alice
CARDDAV_PASS=secret
# CARDDAV_INSECURE=true   # self-signed TLS

cargo run -p pepper-crm --example carddav_list
cargo run --bin pepper-web
```

Pepper PUT → Radicale stores `.vcf` → DAVx⁵ syncs to phone.

## Local homelab (Vagrant)

Debian 13 + FreedomBox (Radicale) in VirtualBox for CardDAV testing without a Pi.

**Prerequisites:** VirtualBox, Vagrant 2.4+. First `vagrant up` may install `vagrant-vbguest` — re-run once if prompted.

```bash
vagrant up          # note first-boot secret in terminal output
# https://localhost:8443 — finish FreedomBox wizard (try admin / freedombox)
vagrant ssh
vagrant provision --provision-with restart-radicale
vagrant halt / vagrant destroy
```

`tests/data/radicale/` syncs to `/var/lib/radicale` in the guest (`admin/test-contacts` sample book). Point `CARDDAV_*` at the VM once Radicale is configured.

## Raspberry Pi deployment

Cross-compile from Apple Silicon Mac (Pi 5 is aarch64 Linux, not macOS):

```bash
brew install messense/macos-cross-toolchains/aarch64-unknown-linux-gnu
rustup target add aarch64-unknown-linux-gnu
./scripts/build-linux-arm64.sh
```

Copy `target/aarch64-unknown-linux-gnu/release/pepper` (and optionally `pepper-web`) to the Pi, or `cargo build --release` on the Pi directly.

### Weekly digest cron (Monday 6:00, trip timezone)

Sends Monday at **6:00** in the IANA timezone for that Monday's calendar trip (`SUMMARY` = destination). Falls back to **US Eastern** if not traveling or lookup fails.

Hourly cron runs `pepper --send-if-due` so 6am works in any offset.

```bash
chmod +x ~/pepper-crm/scripts/run-weekly-digest.sh
cd ~/pepper-crm && ./target/release/pepper --schedule-status
PEPPER_HOME=~/pepper-crm ./scripts/install-weekly-cron.sh
```

Force send: `DIGEST_FORCE=1 ./scripts/run-weekly-digest.sh`

Logs: `~/pepper-crm/logs/weekly-digest.log`. Override binary path with `PEPPER_BIN`.

## Design principles

- **VCF is the source of truth** — no duplicate contact database
- **Notes field is human-readable first** — plain text tags
- **CRM log is append-only** (planned) — below `--- CRM Log ---`
- **Last tag wins** — most recent `Reconnect:` is authoritative
- **Dry-run always works** — safe testing without email
- **Prototype locally, promote to Pi** — VCF files → CardDAV
- **stdio now, HTTP/SSE later** — MCP transport upgrade path

## Documentation index

| Doc | Description |
|-----|-------------|
| [`README.md`](README.md) | User overview and quick start |
| [`IMPLEMENTATION_STATUS.md`](IMPLEMENTATION_STATUS.md) | Built vs. planned |
| [`personal_crm_design.md`](personal_crm_design.md) | Full design document |
| [`pepper-crm/README_pepper-crm.md`](pepper-crm/README_pepper-crm.md) | Core library |
| [`pepper-web/README_pepper-web.md`](pepper-web/README_pepper-web.md) | Web dashboard |
| [`pepper/README_pepper.md`](pepper/README_pepper.md) | Weekly CLI |
| [`mcp-servers/README_mcp-servers.md`](mcp-servers/README_mcp-servers.md) | MCP servers |
| [`scripts/README_scripts.md`](scripts/README_scripts.md) | Pi build + cron |
| [`DASHBOARD_SECTIONS.md`](DASHBOARD_SECTIONS.md) | Dashboard product spec |
| [`NEXT_WEEK_TRAVEL_BUILD.md`](NEXT_WEEK_TRAVEL_BUILD.md) | Travel matching |

## What's next

- Agent-driven interaction logging (`log_interaction` via MCP or Matrix bot)
- Matrix bot ("chat with Pepper")
- HTTP/SSE transport for persistent MCP daemons
- Digest travel section polish
