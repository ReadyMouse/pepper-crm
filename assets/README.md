<!--
# Assets — Shared Media Index

  Describes brand PNG layout and which Pepper surfaces use which avatar.

INPUT:
  - Static files under assets/brand/

OUTPUT:
  - Usage table for web vs. email

NOTES:
  - Do not duplicate brand files into pepper-web/static/

Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.
-->

# Assets

Shared media for Pepper — not tied to a single binary. Web, email, and future clients all reference files here.

## Layout

```
assets/
├── brand/          # Pepper mascot & logos
│   ├── pepper_avatar_teal.png   # Teal background — web dashboard header
│   └── pepper_avatar_white.png  # White/transparent — email digest, dark headers
└── README.md
```

## Usage by surface

| File | Where it's used |
|------|-----------------|
| `brand/pepper_avatar_teal.png` | Web dashboard header (`pepper-web`, URL `/assets/brand/...`) |
| `brand/pepper_avatar_white.png` | Weekly HTML email digest (`templates/digest.html`, via MCP digest server) |

## Web (`pepper-web`)

- **CSS only** lives in `pepper-web/static/` (e.g. `theme.css`).
- **Images** are served from this folder at `/assets/brand/<filename>` — do not copy avatars into `pepper-web/static/`.

## Email digest

When you wire the white avatar into the digest template, prefer either:

1. **CID attachment** (best for email clients) — embed `assets/brand/pepper_avatar_white.png` when sending via `mcp-mailer-server`
2. **Hosted URL** — if the app is deployed with static assets public

## Adding files

- Put new brand images under `assets/brand/`.
- Put web-only assets (favicons, etc.) under `assets/web/` if you add that folder later.
- Keep filenames lowercase with underscores.
