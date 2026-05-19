<!--
# Pepper Web Dashboard — Summary

  Short snapshot of pepper-web features; prefer pepper-web/README.md for details.

INPUT:
  - pepper-web routes and templates

OUTPUT:
  - Feature list, run instructions, related doc links

NOTES:
  - Superseded in part by README_pepper-web.md.

Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.
-->

# Pepper Web Dashboard — Summary

> **Note:** This file is a snapshot summary. Prefer [`pepper-web/README.md`](pepper-web/README.md) and the root [`README.md`](README.md) for current docs.

## What it is

A localhost dashboard at **http://localhost:3000** for visualizing Pepper CRM data without sending emails.

## Live features

1. **Dashboard** (`/`)
   - Summary stats: pending tasks, reconnects due, travel matches
   - **Reconnects Due** — 7-day window, snooze to VCF
   - **Pending Tasks** — from VCF `TODO:` tags via PostgreSQL
   - **Next Week Travel** — Google Calendar + geocoding + metro-radius matching; refresh on demand

2. **Digest Preview** (`/preview`)
   - Preview of the weekly email (tasks + reconnects)

## Removed / not built

- **Contacts page** (`/contacts`) — removed; VCF files are the people store
- **Random Person of the Week** — coming soon ([`DASHBOARD_SECTIONS.md`](DASHBOARD_SECTIONS.md))

## How to run

```bash
cargo build --workspace
cp .env.example .env   # set DATABASE_URL, optional GOOGLE_CALENDAR_ICS_URL

cargo run --bin pepper-web
```

Sync happens on startup. Alternatively run `./target/debug/pepper --dry-run` first.

## Stack

Axum · Tera · SQLx · pepper-crm · theme.css + snooze.js

## Related docs

- [`pepper-web/README.md`](pepper-web/README.md) — routes, file layout, dev workflow
- [`NEXT_WEEK_TRAVEL_BUILD.md`](NEXT_WEEK_TRAVEL_BUILD.md) — travel matching design
- [`DASHBOARD_SECTIONS.md`](DASHBOARD_SECTIONS.md) — product spec for all dashboard sections
