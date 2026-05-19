# pepper-crm — Core Library

## Purpose

Shared Rust library containing all Pepper CRM business logic: vCard parsing, tag extraction, PostgreSQL task state, iCalendar generation, calendar travel parsing, geocoding, and metro-radius contact matching.

## Contents

| Path | Role |
|------|------|
| `src/lib.rs` | Crate root, public re-exports |
| `src/models.rs` | Contact, Task, Reconnect, travel structs |
| `src/vcard.rs` | VCF read/write, geo fields, snooze |
| `src/tags.rs` | `TODO:` / `Reconnect:` parsing and due logic |
| `src/db.rs` | sqlx PostgreSQL queries |
| `src/ical.rs` | `.ics` file generation |
| `src/calendar.rs` | Google Calendar ICS fetch |
| `src/geo.rs` | Nominatim geocoding + cache |
| `src/contact_geo.rs` | Batch geocode contacts |
| `src/travel.rs` | Travel week matching |
| `src/travel_cache.rs` | Weekly snapshot persistence |
| `examples/` | CLI demos (`list_due_reconnects`, etc.) |
| `tests/` | Test contact generator, fixtures |

## Dependencies

PostgreSQL (sqlx), chrono, reqwest (calendar/geocode), regex, serde. Standard OSS dependencies — reads local VCF files and public Nominatim.

## Open-source candidate

**Yes.** Self-contained CRM library with standard OSS dependencies. CardDAV and SMTP are consumer-configured, not vendor-locked.

## Consumers

- `pepper` weekly runner
- `pepper-web` dashboard
- All `mcp-*-server` binaries
