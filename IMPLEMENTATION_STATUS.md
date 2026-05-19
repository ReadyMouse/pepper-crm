<!--
# Pepper — Implementation Status

  Living checklist of completed vs. planned features across all crates.

INPUT:
  - Current codebase state

OUTPUT:
  - Phase completion tables, quick commands, tag format summary

NOTES:
  - Kept in sync with root README.md.

Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.
-->

# Pepper — Implementation Status

Living summary of what's built vs. planned. For setup and usage, see [`README.md`](README.md).

---

## Completed

### Phase 1 — Core library (`pepper-crm`)

| Module | Status | Notes |
|--------|--------|-------|
| `models.rs` | ✅ | Contact, Task, Reconnect, travel structs |
| `vcard.rs` | ✅ | Parse/write VCF, geo fields, snooze write-back |
| `tags.rs` | ✅ | `TODO:` in NOTE; `Reconnect:` in CATEGORIES (+ legacy NOTE) |
| `db.rs` | ✅ | PostgreSQL via sqlx — task state only |
| `ical.rs` | ✅ | `.ics` with VALARM |
| `calendar.rs` | ✅ | Google Calendar ICS fetch, next-week trips |
| `geo.rs` | ✅ | Nominatim geocoding + query cache |
| `contact_geo.rs` | ✅ | Batch geocode, optional VCF write-back |
| `travel.rs` | ✅ | Metro-radius matching, ranking |
| `travel_cache.rs` | ✅ | Weekly snapshot files in `.cache/travel/` |

**Capabilities:**

- Parses vCard 2.1/3.0/4.0 with line unfolding
- Time-based reconnects (`N days/weeks/months/years`)
- Trip triggers (`before Chicago trip`)
- `Reconnect: Never` exclusion
- Due-date anchoring from `REV` or `Month YYYY:` notes
- Append-only CRM log (`--- CRM Log ---`)
- Last `Reconnect:` tag wins

### Phase 2 — MCP servers + weekly runner

All servers use stdio transport via `rmcp`:

| Crate | Tools | Status |
|-------|-------|--------|
| `mcp-vcard-server` | `parse_vcards`, `log_interaction` | ✅ |
| `mcp-scheduler-server` | `upsert_contacts`, `get_due` | ✅ |
| `mcp-digest-server` | `render_digest` | ✅ |
| `mcp-cal-server` | `export_ics` | ✅ |
| `mcp-mailer-server` | `send_email` | ✅ |
| `mcp-calendar-server` | `get_upcoming_travel` | ✅ |
| `mcp-travel-server` | `build_travel_week`, `get_travel_week` | ✅ |
| `pepper` | Orchestrates full weekly flow | ✅ |

`pepper` flags: `--dry-run`, `--recipient`, `--force-travel`, `--contacts-dir`.

### Phase 3 — Web dashboard (`pepper-web`)

| Feature | Status |
|---------|--------|
| VCF → PostgreSQL sync on startup | ✅ |
| Pending tasks list | ✅ |
| Reconnects due (7-day window) | ✅ |
| Digest preview (`/preview`) | ✅ |
| Next Week Travel (calendar + geo + metro match) | ✅ |
| Travel refresh on demand | ✅ |
| Reconnect snooze (VCF write-back) | ✅ |
| Random Person of the Week | 🔜 |
| Contacts browse page | — removed (VCF is source of truth) |

### Infrastructure

- ✅ Cargo workspace (all members build)
- ✅ PostgreSQL migration (`migrations/001_initial.sql`)
- ✅ `.env.example` (DB, SMTP, calendar, geocoding)
- ✅ 20 generated test VCFs in `contacts/`
- ✅ Email template (`templates/digest.html`)
- ✅ Brand assets (`assets/brand/`)

---

## Not yet built

| Item | Notes |
|------|-------|
| CardDAV read/write | Stubbed for Pi production |
| HTTP/SSE MCP transport | stdio only today |
| Matrix bot | Future chat interface |
| Random Person + web enrichment | Spec in `DASHBOARD_SECTIONS.md` |
| Travel section in weekly digest email | Dashboard only for now |
| Mark tasks done from UI | Tasks sync from VCF only |
| AI contact enrichment | Out of prototype scope |

---

## Project structure (current)

```
pepper-crm/
├── pepper-crm/              ✅ core library
├── mcp-vcard-server/        ✅
├── mcp-scheduler-server/    ✅
├── mcp-digest-server/       ✅
├── mcp-cal-server/          ✅
├── mcp-mailer-server/       ✅
├── mcp-calendar-server/     ✅
├── mcp-travel-server/       ✅
├── pepper/                  ✅ weekly runner
├── pepper-web/              ✅ dashboard
├── contacts/                ✅ test VCFs
├── migrations/              ✅
├── templates/               ✅ digest email
├── assets/brand/            ✅
└── .cache/                  geocode + travel snapshots (gitignored)
```

---

## Quick commands

```bash
cargo build --workspace
cargo test -p pepper-crm

createdb pepper_crm
psql pepper_crm < migrations/001_initial.sql
cp .env.example .env

./target/debug/pepper --dry-run          # sync + preview digest
cargo run --bin pepper-web               # dashboard
./target/debug/pepper                    # send weekly digest
```

---

## Tag format (current)

**TODO:** in `NOTE` (one per line):

```
TODO: send intro email
```

**Reconnect:** in `CATEGORIES`:

```
CATEGORIES:Reconnect: 3 months
```

Also supports trip triggers (`before Chicago trip`), `Reconnect: Never`, and legacy `Reconnect:` lines in `NOTE`.

Due dates anchor from vCard `REV` or latest `Month YYYY:` note line.
