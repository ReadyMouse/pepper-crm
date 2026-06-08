<!--
  Pepper Web Dashboard Documentation

    Developer guide for the localhost web UI crate.

  INPUT: None (markdown docs).
  OUTPUT: Setup, routes, file layout, and workflow for pepper-web.
  NOTES: See root README.md for full project setup; brand assets live under assets/brand/.

  Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.
-->

# Pepper Web Dashboard

Localhost web UI for visualizing and testing Pepper CRM data. See the root [`README.md`](../README.md) for full project setup and [`README_pepper-web.md`](README_pepper-web.md) for folder-level crate overview.

## Quick Start

```bash
# From repo root
cargo run --bin pepper-web
```

Open **http://localhost:3000**

The server loads contacts from VCF (or CardDAV) on startup. For travel matches, set `GOOGLE_CALENDAR_ICS_URL` in `.env`, then click **Refresh travel matches** on the dashboard.

## Pages

| Route | Description |
|-------|-------------|
| `/` | Dashboard — tasks, reconnects due, next-week travel |
| `/preview` | Live preview of the weekly email digest |

## Dashboard sections

### Live

- **Stats** — counts for pending tasks, reconnects due, travel matches
- **Reconnects Due** — timed `Reconnect:` intervals due within 7 days; snooze writes back to VCF
- **Pending Tasks** — open `TODO:` items from vCard NOTE fields
- **Next Week Travel** — metro-radius matching from Google Calendar + contact addresses; refresh on demand with configurable radius

### Coming soon

- **Random Person of the Week** — see [`DASHBOARD_SECTIONS.md`](../DASHBOARD_SECTIONS.md)

## Routes (API)

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/` | Render dashboard |
| `GET` | `/preview` | Render digest preview |
| `POST` | `/travel/refresh` | Rebuild travel snapshot (calendar + geocode + match) |
| `POST` | `/travel/snooze` | Update reconnect interval on a contact VCF |

Snooze accepts `Accept: application/json` for in-page updates via `snooze.js`.

## Static files

| URL path | Source |
|----------|--------|
| `/static/theme.css` | `pepper-web/static/` |
| `/static/snooze.js` | `pepper-web/static/` |
| `/assets/brand/*` | `assets/brand/` (shared avatars) |

The header uses `pepper_avatar_teal.png`. The white avatar is for the email digest in `templates/`, not the web app.

## File structure

```
pepper-web/
├── Cargo.toml
├── README.md
├── src/
│   └── main.rs              # Axum server
├── static/
│   ├── theme.css
│   └── snooze.js
└── templates/
    ├── dashboard.html
    ├── preview.html
    └── partials/
        └── header.html
```

## Development workflow

```
1. Edit VCF files in contacts/
2. Restart pepper-web (syncs on startup) or run pepper --dry-run
3. Refresh browser
4. Check /preview for digest content
5. For travel: edit calendar or VCF, then Refresh travel matches
```

## Stack

- **Axum** — web framework
- **Tera** — templates
- **Tower-HTTP** — tracing, static file serving
- **pepper-crm** — VCF parsing, due items, travel matching

Brand images live under `assets/brand/` at the repo root — do not duplicate into `pepper-web/static/`.
