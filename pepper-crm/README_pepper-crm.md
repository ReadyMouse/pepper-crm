<!--
# pepper-crm — Core Library

  Folder README for the shared Pepper CRM Rust library crate.

INPUT:
  - None (human-facing folder overview)

OUTPUT:
  - Module map, dependency notes, open-source assessment

NOTES:
  - See inline `//!` headers on each source file for INPUT/OUTPUT detail.

Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.
-->

# pepper-crm — Core Library

## Purpose

Shared Rust library containing all Pepper CRM business logic: vCard parsing, tag extraction, iCalendar generation, calendar travel parsing, geocoding, digest rendering, and dashboard feature modules (birthdays, random picks, data enrichment).

## Contents

| Path | Role |
|------|------|
| `src/lib.rs` | Crate root, public re-exports |
| `src/models.rs` | Contact, pending task/reconnect, travel, digest structs |
| `src/vcard.rs` | VCF read/write, geo fields, snooze, task completion |
| `src/carddav.rs` | CardDAV REPORT/PUT when `CARDDAV_*` env is set |
| `src/tags.rs` | `TODO:` / `Reconnect:` parsing and due logic |
| `src/tasks.rs` | Pending TODOs from vCard NOTE fields |
| `src/ical.rs` | `.ics` file generation |
| `src/calendar.rs` | Google Calendar ICS fetch |
| `src/digest.rs` | Weekly digest Tera context and HTML render |
| `src/digest_schedule.rs` | Monday 6:00 send window by trip timezone |
| `src/mail.rs` | SMTP delivery via lettre |
| `src/weekly.rs` | End-to-end weekly digest pipeline |
| `src/geo.rs` | Nominatim geocoding + cache |
| `src/contact_geo.rs` | Batch geocode contacts |
| `src/travel.rs` | Travel week matching |
| `src/travel_cache.rs` | Weekly snapshot persistence |
| `src/birthdays.rs` | Upcoming birthday window for dashboard/digest |
| `src/random_pick.rs` | Weekly random contact spotlight |
| `src/data_enrichment.rs` | Address-fix picks for dashboard enrichment |
| `examples/` | CLI demos (CardDAV, geocode, SMTP, random pick, etc.) |
| `tests/` | Test contact generator, fixtures |

## Dependencies

chrono, reqwest (calendar/geocode/CardDAV), regex, serde, tera, lettre. Standard OSS dependencies — reads local VCF files and public Nominatim.

## Open-source candidate

**Yes.** Self-contained CRM library with standard OSS dependencies. CardDAV and SMTP are consumer-configured, not vendor-locked.

## Consumers

- `pepper` weekly runner
- `pepper-web` dashboard
- All `mcp-*-server` binaries
