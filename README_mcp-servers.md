# MCP Servers — Agent Tool Layer

## Purpose

Seven thin Rust binaries exposing `pepper-crm` functionality as MCP tools over stdio transport. Designed for the `pepper` weekly runner and future AI agents (Claude, etc.).

## Servers

| Crate | Tools | Role |
|-------|-------|------|
| `mcp-vcard-server` | `parse_vcards`, `log_interaction` | Read/write local VCF |
| `mcp-scheduler-server` | `upsert_contacts`, `get_due` | PostgreSQL sync + due queries |
| `mcp-digest-server` | `render_digest` | HTML email from Tera template |
| `mcp-cal-server` | `export_ics` | Calendar attachments for reconnects |
| `mcp-mailer-server` | `send_email` | SMTP delivery via lettre |
| `mcp-calendar-server` | `get_upcoming_travel` | Fetch next-week trips from ICS |
| `mcp-travel-server` | `build_travel_week`, `get_travel_week` | Travel match snapshot |

Each crate: `Cargo.toml` + `src/main.rs` only.

## Architecture notes

- **stdio only** today — one process per tool call, no HTTP port
- `pepper` spawns vcard, scheduler, digest, cal, mailer directly; travel/calendar can also be called by agents independently
- CardDAV stub planned for production Pi deployment

## Open-source candidate

**Yes.** Standard `rmcp` MCP pattern with no proprietary dependencies. SMTP and DB credentials come from consumer `.env`.
