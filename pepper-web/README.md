# Pepper Web Dashboard

A localhost web UI for visualizing and testing your Pepper CRM data.

## Quick Start

```bash
cargo run --bin pepper-web
```

Then open: **http://localhost:3000**

## Pages

- **Dashboard** (`/`) — Product layout (sections marked Coming Soon)
- **Digest Preview** (`/preview`) — Live preview of the weekly email

## Static files

| Path | Source |
|------|--------|
| `/static/theme.css` | `pepper-web/static/` (web-only styles) |
| `/assets/brand/*` | `assets/brand/` (shared avatars — see `assets/README.md`) |

The header uses `pepper_avatar_teal.png`. The white avatar is for the email digest in `templates/`, not the web app.

## Development

- **Axum** + **Tera** + **SQLx** + **pepper-crm**
- Brand images: edit under `assets/brand/` at the repo root — do not duplicate into `pepper-web/static/`
