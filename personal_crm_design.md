<!--
# Personal CRM Agent — Design Document

  Architecture and scope for Pepper CRM.

INPUT:
  - Product requirements, vCard/tag conventions, MCP transport decisions

OUTPUT:
  - Design principles, crate layout, data model, upgrade path

NOTES:
  - Operational detail (env vars, Vagrant, Pi cron) lives in README_technical.md.
  - User overview lives in README.md.

Written by Cursor for Ready Mouse and Pepper CRM. June 2026. All rights reserved.
-->

# Personal CRM Agent — Design Document

> **Author:** Mylo  
> **Date:** May–June 2026  
> **Status:** Working prototype — local VCF or CardDAV (Radicale), weekly CLI, web dashboard

---

## What We Are Building

**Pepper** is a lightweight personal CRM built as a Rust workspace. It reads contact data from vCards (local `.vcf` files or CardDAV), parses structured tags, sends a weekly HTML email digest with `.ics` reconnect reminders, and surfaces travel-based reconnect suggestions on a local dashboard.

```
Phone Contacts  ↔  DAVx⁵ / CardDAV  ↔  Radicale (.vcf)  ↔  Pepper (pepper + pepper-web)
```

The weekly `pepper` binary runs on a schedule (cron on a Pi). MCP servers expose the same operations for a future agent workflow. **No separate CRM database** — vCards are the store.

**It does:** parse tags, compute due tasks/reconnects, match contacts to trips, geocode addresses, write snooze/task/geo updates back to vCards, send one useful email per week, and show a dashboard at `http://localhost:3000`.

**Planned:** agent-driven interaction log (`--- CRM Log ---`) via MCP or a Matrix bot; HTTP/SSE MCP transport for live agent calls.

---

## Core Design Principles

- **VCF is the source of truth.** Tasks, reconnect intervals, geo, and (eventually) CRM log entries live in vCard fields.
- **Human-readable tags.** `TODO:` in Notes; `Reconnect:` in Categories — editable in any contacts app.
- **Reconnect snooze writes `CATEGORIES` + `REV`.** Timed snooze sets `Reconnect: …` in Categories and refreshes `REV` as the anchor. It does **not** stamp the Notes field.
- **CRM log is append-only (when enabled).** The agent must not rewrite interaction history above `--- CRM Log ---`; it only appends dated entries below the separator.
- **Last `Reconnect:` wins.** In Categories (or legacy NOTE lines), the last matching tag is authoritative.
- **Dry-run always works.** `pepper --dry-run` previews the digest without sending email.
- **Prototype locally, promote to Pi.** Same code; swap `CONTACTS_DIR` for `CARDDAV_*` and run on the Pi with cron.
- **stdio now, HTTP/SSE later.** MCP servers use stdio today; transport can change without altering tool semantics.

---

## Built Today

| Area | Status |
|------|--------|
| VCF parse/write (2.1/3.0/4.0, line folding) | ✅ |
| `TODO:` tasks in NOTE | ✅ |
| `Reconnect:` in CATEGORIES (+ legacy NOTE) | ✅ |
| Due-date anchoring from `REV` or `Month YYYY:` note | ✅ |
| `Reconnect: Never`, `Do Not Engage` | ✅ |
| Weekly HTML digest + `.ics` attachments | ✅ |
| `pepper-web` dashboard | ✅ |
| Travel matching (calendar ICS + metro radius) | ✅ |
| Nominatim geocoding + `GEO` / `X-PEPPER-GEO-SOURCE` write-back | ✅ |
| CardDAV read (REPORT) + write (PUT) for Radicale | ✅ |
| Vagrant homelab (FreedomBox + Radicale) for CardDAV testing | ✅ |
| Random Person of the Week, birthdays, data enrichment picks | ✅ |
| MCP servers (stdio) | ✅ |
| Agent `log_interaction` / CRM log in production flows | 🔲 planned |
| Matrix bot | 🔲 planned |
| HTTP/SSE MCP transport | 🔲 planned |

---

## Cargo Workspace Structure

```
pepper-crm/
├── Cargo.toml                  # workspace root
├── .env.example
├── contacts/                   # local .vcf prototyping (gitignored *.vcf)
├── pepper-crm/                 # shared library
│   └── src/
│       ├── vcard.rs            # parse/write VCF, CardDAV I/O
│       ├── carddav.rs          # Radicale REPORT/PUT
│       ├── tags.rs             # TODO/Reconnect extraction, due dates
│       ├── tasks.rs            # pending TODOs, complete → NOTE write-back
│       ├── models.rs
│       ├── ical.rs
│       ├── calendar.rs         # Google Calendar ICS trips
│       ├── digest.rs / weekly.rs / mail.rs
│       ├── geo.rs / contact_geo.rs
│       ├── travel.rs / travel_cache.rs
│       ├── birthdays.rs / random_pick.rs / data_enrichment.rs
│       └── ...
├── pepper/                     # weekly CLI (`pepper --dry-run`)
├── pepper-web/                 # Axum dashboard
├── mcp-vcard-server/
├── mcp-digest-server/
├── mcp-cal-server/
├── mcp-mailer-server/
├── mcp-calendar-server/
├── mcp-travel-server/
├── templates/digest.html
├── tests/data/radicale/        # Vagrant CardDAV fixtures
├── Vagrantfile
└── scripts/                    # Pi cross-compile + cron
```

The weekly CLI calls `pepper-crm` directly. MCP binaries are thin wrappers for future agent orchestration.

---

## Contacts Source: Local Files vs CardDAV

| Mode | Config | Use |
|------|--------|-----|
| **Local** | `CONTACTS_DIR=./contacts` (no `CARDDAV_*`) | Laptop dev, read/write `.vcf` on disk |
| **CardDAV** | `CARDDAV_URL`, `CARDDAV_USER`, `CARDDAV_PASS` | Pi or Vagrant Radicale; phone sync via DAVx⁵ |
| **Safe prod** | `CONTACTS_READ_ONLY=true`, `GEO_WRITE_TO_VCF=false` | Probe Pi book before enabling writes |

CardDAV implementation (`carddav.rs`): `addressbook-query` REPORT for reads; HTTP PUT per contact href for snooze, task complete, location, and geo write-back. FreedomBox may return HTTP 500 on PUT even when the vCard was stored — Pepper re-GETs and verifies content.

Local homelab: Debian + FreedomBox in VirtualBox (`vagrant up`). See [`README_technical.md`](README_technical.md#local-homelab-vagrant).

---

## Tag Format

### `TODO:` — in `NOTE`

One line per task, inside the Notes field (not a separate vCard property):

```
July 2026: Met at conference.

TODO: send intro email
```

Completing a task removes that `TODO:` line from NOTE via write-back.

### `Reconnect:` — in `CATEGORIES`

```
CATEGORIES:Pool Player: BOS,Reconnect: 3 months
```

Supported values: timed intervals (`1 week`, `3 months`, …), trip triggers (`before Chicago trip`), `Reconnect: Never`, and `Do Not Engage`.

Legacy `Reconnect:` lines in NOTE are still read as a fallback.

**Snooze / random pick:** updates `CATEGORIES` (`Reconnect: …`) and `REV` only — leaves interaction notes unchanged.

**Due-date anchor:** `REV` timestamp or the latest `Month YYYY:` line in NOTE (e.g. `May 2026: Had coffee`).

### Engagement categories

| Category | Meaning |
|----------|---------|
| `Reconnect: Never` | No timed nudges; still eligible for random pick + birthdays |
| `Do Not Engage` | Omit from all Pepper surfaces |

### CRM log (planned)

Append-only block below `--- CRM Log ---`:

```
--- CRM Log ---
2026-05-14: Sent follow-up email.
```

Library support exists (`log_interaction`); weekly CLI and dashboard do not call it yet.

---

## Write-Back Summary

| Action | Fields written |
|--------|----------------|
| Snooze reconnect | `CATEGORIES`, `REV` |
| Mark task done | `NOTE` (remove `TODO:` line) |
| Save location / geocode | `ADR`, `GEO`, `X-PEPPER-GEO-SOURCE` |
| Do Not Engage | `CATEGORIES`, `NOTE` (stamp), `REV` |
| CRM log (planned) | `NOTE` (append below separator) |

---

## `pepper-crm` — Key Types

```rust
pub struct Contact {
    pub uid: String,
    pub full_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub org: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
    pub geo: Option<GeoPoint>,
    pub geo_source: Option<String>,
    pub note_raw: String,
    pub todos: Vec<String>,
    pub categories: Vec<String>,
    pub reconnect_tag: Option<String>,
    pub log_entries: Vec<String>,
    pub vcf_path: PathBuf,
    pub carddav_href: Option<String>,
}
```

Parsing notes:
- Strip `--- CRM Log ---` before extracting `TODO:` / month-year anchors
- Unfold vCard continuation lines
- `Reconnect:` resolved from `CATEGORIES` first, then legacy NOTE

---

## MCP Tool Surface (stdio today)

| Server | Tools |
|--------|-------|
| `mcp-vcard-server` | `parse_vcards`; `log_interaction` (planned in flows) |
| `mcp-digest-server` | `render_digest` |
| `mcp-cal-server` | `export_ics` |
| `mcp-mailer-server` | `send_email` |
| `mcp-calendar-server` | `get_upcoming_travel` |
| `mcp-travel-server` | `build_travel_week`, `get_travel_week` |

**Historical note:** An early prototype used PostgreSQL and `mcp-scheduler-server`. That layer was removed; vCards are the sole store.

---

## Weekly Run Sequence (`pepper`)

1. Parse contacts (VCF dir or CardDAV)
2. Collect due tasks and reconnects from tags
3. Render HTML digest (Tera template)
4. Generate `.ics` attachments for due reconnects
5. Send email via SMTP (skip on `--dry-run`)
6. Build travel snapshot when calendar URL is configured (`--force-travel` or weekly cache miss)

Cron on Pi: hourly `pepper --send-if-due` → Monday 6:00 in the trip timezone.

---

## Test Data

```bash
cargo test -p pepper-crm --test generate_test_contacts -- --ignored
```

Creates 20 scenario vCards in `./contacts/`. CardDAV fixtures for Vagrant live in `tests/data/radicale/`.

CardDAV smoke tests (with `CARDDAV_*` in `.env`):

```bash
cargo run -p pepper-crm --example carddav_list
cargo run -p pepper-crm --example carddav_snooze -- test-contact "1 week"
cargo run -p pepper-crm --example carddav_write_location -- test-contact "Chicago" IL
```

---

## Environment Variables

See [`.env.example`](.env.example) and [`README_technical.md`](README_technical.md#environment-variables). Key groups:

- `CONTACTS_DIR` / `CARDDAV_*` — people store
- `CONTACTS_READ_ONLY`, `GEO_WRITE_TO_VCF` — write safety
- `SMTP_*`, `DIGEST_RECIPIENT` — weekly email
- `GOOGLE_CALENDAR_ICS_URL` — travel trips
- `NOMINATIM_USER_AGENT` — geocoding

---

## Local Dev Setup

```bash
cp .env.example .env
cargo build --workspace

cargo run --bin pepper-web              # dashboard → http://localhost:3000
cargo run --bin pepper -- --dry-run     # preview weekly email
cargo run --bin pepper                  # send digest
```

---

## Upgrade Path: Agent-Callable MCP

When ready for a live Claude agent:

1. Add HTTP/SSE transport to MCP server binaries
2. Run servers as persistent daemons on the Pi (systemd)
3. Agent calls `log_interaction`, travel, and digest tools directly
4. Cron becomes a scheduled agent invocation or keeps triggering `pepper --send-if-due`

Tool signatures and vCard tag format stay stable across the transport change.

---

## Related Docs

| Doc | Purpose |
|-----|---------|
| [`README.md`](README.md) | User overview and quick start |
| [`README_technical.md`](README_technical.md) | Env vars, CardDAV, Vagrant, Pi deployment, full tag spec |
