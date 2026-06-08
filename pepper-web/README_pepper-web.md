<!--
# pepper-web — Local Dashboard

  Folder README for the Axum web dashboard crate.

INPUT:
  - None (human-facing folder overview)

OUTPUT:
  - Route and template map, dependency notes, open-source assessment

NOTES:
  - Integration tests live in `tests/dashboard_render.rs`.

Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.
-->

# pepper-web — Local Dashboard

## Purpose

Axum web server providing a localhost UI (`127.0.0.1:3000`) to visualize pending tasks, due reconnects, next-week travel matches, random people of the week, data enrichment picks, upcoming birthdays, and a weekly digest preview.

## Contents

| Path | Role |
|------|------|
| `src/main.rs` | Routes, VCF sync, travel refresh/snooze, random pick and enrichment handlers |
| `templates/dashboard.html` | Main dashboard layout |
| `templates/preview.html` | Email digest preview |
| `templates/partials/header.html` | Shared nav + branding |
| `static/theme.css` | Dashboard styles |
| `static/snooze.js` | In-page snooze without full reload |
| `tests/dashboard_render.rs` | Regression: dashboard template renders without stack overflow |
| `README.md` | Quick start for this crate |

## Dependencies

Requires `CONTACTS_DIR` (or CardDAV). Optional `GOOGLE_CALENDAR_ICS_URL`, `CACHE_DIR`, geocoding env vars (via `pepper-crm`).

## Open-source candidate

**Yes**, with caveats: useful as a generic CRM dashboard for local VCF workflows. Brand assets live in [`assets/brand/`](../assets/brand/).

## Related docs

- [`DASHBOARD_SECTIONS.md`](../DASHBOARD_SECTIONS.md) — product spec
- [`NEXT_WEEK_TRAVEL_BUILD.md`](../NEXT_WEEK_TRAVEL_BUILD.md) — travel matching
