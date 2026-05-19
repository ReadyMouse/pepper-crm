# Pepper - Personal CRM Agent

A lightweight personal CRM agent built as a Rust MCP server workspace. It reads contact data from `.vcf` (vCard) files, parses structured tags from the notes field, manages task state in PostgreSQL, and sends a weekly HTML email digest with `.ics` calendar attachments for upcoming follow-ups.

Meet **Pepper** - your friendly personal CRM assistant that helps you stay connected with your network.

## Current Status

✅ **Phase 1 Complete: pepper-crm and Test Data**

The core library and test contact generator have been implemented:

- **pepper-crm**: Shared library containing all business logic
  - VCF parsing and write-back (`vcard.rs`)
  - Tag extraction (`tags.rs`)
  - PostgreSQL database operations (`db.rs`)
  - iCalendar generation (`ical.rs`)
  - Data models (`models.rs`)
  
- **Test Contact Generator**: 20 realistic fake VCF contacts with various scenarios
  - 3 contacts with no tags
  - 3 with TODO only
  - 3 with Reconnect due this week
  - 3 with multiple TODOs + Reconnect
  - 2 with city triggers (deferred)
  - 2 with existing CRM Log blocks
  - 2 with overdue reconnects
  - 2 with incomplete records

## Project Structure

```
pepper/
├── Cargo.toml              # workspace root
├── .env.example            # environment variables template
├── contacts/               # 20 test .vcf files
│   └── contact_*.vcf
├── pepper-crm/             # core library
│   ├── src/
│   │   ├── lib.rs
│   │   ├── vcard.rs       # VCF parsing and write-back
│   │   ├── tags.rs        # TODO:/Reconnect: tag extraction
│   │   ├── db.rs          # PostgreSQL queries
│   │   ├── models.rs      # shared structs
│   │   └── ical.rs        # .ics generation
│   └── tests/
│       └── generate_test_contacts.rs
├── mcp-vcard-server/       # VCF read/write MCP server
├── mcp-scheduler-server/   # Database sync MCP server
├── mcp-digest-server/      # Email rendering MCP server
├── mcp-cal-server/         # iCalendar export MCP server
├── mcp-mailer-server/      # SMTP email MCP server
├── pepper/                 # CLI orchestrator
├── assets/                 # Shared media (brand avatars, etc.)
│   └── brand/              # pepper_avatar_teal.png, pepper_avatar_white.png
├── pepper-web/             # Web dashboard (localhost:3000)
├── templates/              # Email templates
└── migrations/
    └── 001_initial.sql    # PostgreSQL schema
```

## Next Steps

The following MCP server binaries need to be implemented:

1. **mcp-vcard-server** - VCF read/write tools
2. **mcp-scheduler-server** - Due items scheduler
3. **mcp-digest-server** - HTML email renderer
4. **mcp-cal-server** - iCalendar file generator
5. **mcp-mailer-server** - SMTP email sender
6. **pepper** - MCP client orchestrator (the runner binary)

Each server is a thin wrapper around `pepper-crm` functionality, exposing tools via the `rmcp` framework using stdio transport.

Eventually, **Pepper** will also be your friendly Matrix bot that you can chat with to manage your contacts!

## Setup Instructions

### 1. Install Dependencies

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install PostgreSQL (macOS)
brew install postgresql
```

### 2. Set Up Database

```bash
# Create database
createdb pepper_crm

# Run migrations
psql pepper_crm < migrations/001_initial.sql
```

### 3. Configure Environment

```bash
# Copy example environment file
cp .env.example .env

# Edit .env with your settings
# - DATABASE_URL: PostgreSQL connection string
# - SMTP_*: Email server credentials
# - DIGEST_RECIPIENT: Your email address
```

### 4. Build and Test

```bash
# Build the core library
cargo build -p pepper-crm

# Run tests (including VCF parsing tests)
cargo test -p pepper-crm

# Regenerate test contacts (if needed)
cargo test -p pepper-crm --test generate_test_contacts -- --ignored
```

## Web Dashboard 🌶️

For testing and visualization, run the web dashboard:

```bash
cargo run --bin pepper-web
```

Then open: **http://localhost:3000**

### Pages
- **Dashboard** (`/`) — Layout blueprint with all product sections (Coming Soon)
- **Digest Preview** (`/preview`) — Live preview of the weekly email digest

Feature specs: [`DASHBOARD_SECTIONS.md`](DASHBOARD_SECTIONS.md)

## Test Contacts

The `contacts/` directory contains 20 generated test contacts covering various scenarios:

- **contact_01.vcf - contact_03.vcf**: No tags (baseline contacts)
- **contact_04.vcf - contact_06.vcf**: TODO items only
- **contact_07.vcf - contact_09.vcf**: Reconnect reminders due this week
- **contact_10.vcf - contact_12.vcf**: Multiple TODOs + Reconnect
- **contact_13.vcf - contact_14.vcf**: City triggers (deferred)
- **contact_15.vcf - contact_16.vcf**: Existing CRM Log entries
- **contact_17.vcf - contact_18.vcf**: Overdue reconnects
- **contact_19.vcf - contact_20.vcf**: Incomplete contact records

## Design Principles

- **VCF is the people store**: Contact data lives in vCards, DB holds only task state
- **Notes field is human-readable first**: Plain text tags, no binary formats
- **Write-back is append-only**: Never modifies existing content, only appends
- **Last tag wins**: Most recent tag is authoritative
- **Dry-run always works**: Safe testing without side effects
- **Prototype locally, promote to Pi**: Local files now, CardDAV later
- **stdio now, HTTP/SSE later**: Easy development, smooth upgrade path

## Tag Format

Tags are written one per line in the `NOTE` field:

```
July 2026: Met at conference. Works on crypto.

TODO: send intro email
TODO: share grant template

```

"Reconnect: 3 months" is a tag that can be added. 

Supported tags:
- `TODO: [description]` - Create a task
- `Reconnect: N days/weeks/months` - Schedule timed follow-up

## License

MIT
