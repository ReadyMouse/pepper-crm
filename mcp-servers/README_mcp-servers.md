# MCP Servers — Agent Tool Layer

## Purpose

Six thin Rust binaries exposing `pepper-crm` functionality as MCP tools over stdio transport. Designed for AI agents (Claude, etc.) and optional composition outside the main `pepper` CLI.

Each server is a sibling crate at the repo root (e.g. `../mcp-vcard-server/`). This folder holds the shared overview only.

## Servers

| Crate | Tools | Role |
|-------|-------|------|
| `mcp-vcard-server` | `parse_vcards`, `log_interaction` | Read/write VCF or CardDAV (via `CARDDAV_*`) |
| `mcp-digest-server` | `render_digest` | HTML email from Tera template |
| `mcp-cal-server` | `export_ics` | Calendar attachments for reconnects |
| `mcp-mailer-server` | `send_email` | SMTP delivery via lettre |
| `mcp-calendar-server` | `get_upcoming_travel` | Fetch next-week trips from ICS |
| `mcp-travel-server` | `build_travel_week`, `get_travel_week` | Travel match snapshot |

Each crate: `Cargo.toml` + `src/main.rs` only.

## Architecture notes

- **stdio only** today — one process per tool call, no HTTP port
- Task and reconnect state lives in vCards; no database layer
- `mcp-vcard-server` uses `parse_contacts` / `find_contact_by_uid` — same CardDAV path as `pepper-web` when `CARDDAV_*` is set

## Open-source candidate

**Yes.** Standard `rmcp` MCP pattern with no proprietary dependencies. SMTP credentials come from consumer `.env`.
