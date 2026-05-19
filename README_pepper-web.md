# pepper-web — Local Dashboard

## Purpose

Axum web server providing a localhost UI (`127.0.0.1:3000`) to visualize pending tasks, due reconnects, next-week travel matches, and a weekly digest preview.

## Contents

| Path | Role |
|------|------|
| `src/main.rs` | Routes, VCF sync, travel refresh/snooze handlers |
| `templates/dashboard.html` | Main dashboard layout |
| `templates/preview.html` | Email digest preview |
| `templates/partials/header.html` | Shared nav + branding |
| `static/theme.css` | Dashboard styles |
| `static/snooze.js` | In-page snooze without full reload |
| `README.md` | Quick start for this crate |

## Dependencies

Requires `DATABASE_URL`. Optional `GOOGLE_CALENDAR_ICS_URL`, `CONTACTS_DIR`, `CACHE_DIR`, geocoding env vars (via `pepper-crm`).

## Open-source candidate

**Yes**, with caveats: useful as a generic CRM dashboard for local VCF + Postgres workflows. Brand assets live in `assets/brand/` at repo root.

## Related docs

- [`DASHBOARD_SECTIONS.md`](DASHBOARD_SECTIONS.md) — product spec
- [`NEXT_WEEK_TRAVEL_BUILD.md`](NEXT_WEEK_TRAVEL_BUILD.md) — travel matching
