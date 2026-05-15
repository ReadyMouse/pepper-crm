# Pepper - Implementation Summary

## ✅ Completed (Phase 1)

### Core Library: `pepper-crm`

A complete Rust library implementing all CRM business logic:

**Modules:**
- `models.rs` - Data structures (Contact, Task, Reconnect, status enums)
- `vcard.rs` - VCF parsing with line unfolding and write-back support
- `tags.rs` - Tag extraction (TODO:, Reconnect:) with regex parsing
- `db.rs` - PostgreSQL async operations using sqlx
- `ical.rs` - iCalendar file generation with alarms
- `lib.rs` - Public API exports

**Features:**
- ✅ Parses vCard 2.1/3.0/4.0 files
- ✅ Handles line folding/continuation
- ✅ Extracts TODO and Reconnect tags from NOTE field
- ✅ Supports time-based reconnects (N days/weeks/months)
- ✅ Supports city-trigger reconnects (e.g., "before NY trip")
- ✅ Append-only CRM log with `--- CRM Log ---` separator
- ✅ Last tag wins (multiple Reconnect: tags)
- ✅ PostgreSQL schema with contacts/tasks/reconnects/digest_log
- ✅ Generates .ics files with VALARM reminders

### Test Data: 20 Realistic VCF Contacts

Generated in `contacts/` directory covering all scenarios:
- 3 contacts with no tags (baseline)
- 3 with TODO only
- 3 with Reconnect due this week
- 3 with multiple TODOs + Reconnect
- 2 with city triggers (deferred status)
- 2 with existing CRM Log blocks
- 2 with overdue reconnects
- 2 with incomplete records (missing email/phone)

### Infrastructure

- ✅ Cargo workspace structure
- ✅ PostgreSQL migration file (001_initial.sql)
- ✅ .env.example with all config options
- ✅ README with setup instructions
- ✅ Test contact generator

## 📋 Next Steps (Phase 2)

### MCP Servers to Build

Each server is a thin binary that wraps `pepper-crm` and exposes MCP tools:

1. **mcp-vcard-server**
   - Tools: `parse_vcards()`, `log_interaction()`
   - Reads local VCF directory (prototype)
   - TODO: CardDAV integration (production)

2. **mcp-scheduler-server**
   - Tools: `upsert_contacts()`, `get_due()`
   - Syncs contacts to PostgreSQL
   - Returns due tasks/reconnects

3. **mcp-digest-server**
   - Tool: `render_digest()`
   - Renders HTML email using Tera template
   - Creates weekly digest

4. **mcp-cal-server**
   - Tool: `export_ics()`
   - Generates .ics files for reconnects
   - Returns array of IcsFile structs

5. **mcp-mailer-server**
   - Tool: `send_email()`
   - Sends via SMTP with attachments
   - Uses lettre crate

6. **pepper** (the runner binary)
   - MCP client that orchestrates the weekly flow
   - Spawns servers via stdio transport
   - Chains tool calls in sequence
   - Supports `--dry-run` flag

### Phase 3 (Future)

- Matrix bot integration ("Pepper" as chat assistant)
- HTTP/SSE transport (persistent daemon mode)
- CardDAV read/write (Radicale)
- AI enrichment (contact insights)
- Geo radius search

## Project Structure

```
pepper/
├── Cargo.toml                  # Workspace root
├── pepper-crm/                 # Core library ✅
├── mcp-vcard-server/           # TODO
├── mcp-scheduler-server/       # TODO
├── mcp-digest-server/          # TODO
├── mcp-cal-server/             # TODO
├── mcp-mailer-server/          # TODO
├── pepper/                     # Runner binary TODO
├── contacts/                   # Test VCFs ✅
├── migrations/                 # DB schema ✅
├── templates/                  # Email templates TODO
└── .env.example                # Config ✅
```

## Quick Start

```bash
# Build the core library
cargo build -p pepper-crm

# Run all tests
cargo test -p pepper-crm

# Generate test contacts
cargo test -p pepper-crm --test generate_test_contacts -- --ignored

# Set up database
createdb pepper_crm
psql pepper_crm < migrations/001_initial.sql
cp .env.example .env
# Edit .env with your settings
```

## Design Principles

✅ VCF is the people store (DB holds only task state)  
✅ Notes field is human-readable first  
✅ Write-back is append-only  
✅ Last tag wins  
✅ Dry-run always works  
✅ Prototype locally (local VCFs) → promote to Pi (CardDAV)  
✅ stdio now (easy dev) → HTTP/SSE later (agent-callable)

## Tag Format

```
NOTE: Met at conference. Works on crypto.
TODO: send intro email
TODO: share grant template
Reconnect: 3 months
```

After CRM log entry:
```
NOTE: Met at conference.
TODO: send intro email
Reconnect: 3 months
--- CRM Log ---
2026-05-14: Sent follow-up email. Reset to 6 months.
Reconnect: 6 months
```
