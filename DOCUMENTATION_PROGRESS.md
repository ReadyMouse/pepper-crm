# Documentation Progress

Checklist for the documentation agent run. **Completed:** May 2026.

Legend: `[x]` done · `[-]` folder-level only (no safe inline header)

## Root

- [x] `Cargo.toml`
- [x] `.env.example`
- [x] `README.md`
- [x] `migrations/001_initial.sql`
- [x] `templates/digest.html`

## pepper-crm (library crate)

- [x] `pepper-crm/Cargo.toml`
- [x] `pepper-crm/src/lib.rs`
- [x] `pepper-crm/src/models.rs`
- [x] `pepper-crm/src/vcard.rs`
- [x] `pepper-crm/src/tags.rs`
- [x] `pepper-crm/src/db.rs`
- [x] `pepper-crm/src/ical.rs`
- [x] `pepper-crm/src/calendar.rs`
- [x] `pepper-crm/src/geo.rs`
- [x] `pepper-crm/src/contact_geo.rs`
- [x] `pepper-crm/src/travel.rs`
- [x] `pepper-crm/src/travel_cache.rs`
- [x] `pepper-crm/examples/list_due_reconnects.rs`
- [x] `pepper-crm/examples/parse_contacts.rs`
- [x] `pepper-crm/examples/test_calendar.rs`
- [x] `pepper-crm/tests/generate_test_contacts.rs`
- [x] `pepper-crm/tests/fixtures/travel_calendar.ics`

## pepper (orchestrator)

- [x] `pepper/Cargo.toml`
- [x] `pepper/src/main.rs`

## pepper-web (dashboard)

- [x] `pepper-web/Cargo.toml`
- [x] `pepper-web/README.md`
- [x] `pepper-web/src/main.rs`
- [x] `pepper-web/templates/dashboard.html`
- [x] `pepper-web/templates/preview.html`
- [x] `pepper-web/templates/partials/header.html`
- [x] `pepper-web/static/theme.css`
- [x] `pepper-web/static/snooze.js`

## MCP servers

- [x] `mcp-vcard-server/Cargo.toml`
- [x] `mcp-vcard-server/src/main.rs`
- [x] `mcp-scheduler-server/Cargo.toml`
- [x] `mcp-scheduler-server/src/main.rs`
- [x] `mcp-digest-server/Cargo.toml`
- [x] `mcp-digest-server/src/main.rs`
- [x] `mcp-cal-server/Cargo.toml`
- [x] `mcp-cal-server/src/main.rs`
- [x] `mcp-mailer-server/Cargo.toml`
- [x] `mcp-mailer-server/src/main.rs`
- [x] `mcp-calendar-server/Cargo.toml`
- [x] `mcp-calendar-server/src/main.rs`
- [x] `mcp-travel-server/Cargo.toml`
- [x] `mcp-travel-server/src/main.rs`

## assets

- [-] `assets/brand/pepper_avatar_teal.png` — documented in `README_assets.md`
- [-] `assets/brand/pepper_avatar_white.png` — documented in `README_assets.md`
- [x] `assets/README.md`
- [x] `assets/brand/README.md`

## contacts (test VCF fixtures)

- [-] `contacts/*.vcf` — folder-level only (`README_contacts.md`); inline headers break vCard parsers

## Project docs

- [x] `personal_crm_design.md`
- [x] `DASHBOARD_SECTIONS.md`
- [x] `IMPLEMENTATION_STATUS.md`
- [x] `PEPPER_WEB_SUMMARY.md`
- [x] `NEXT_WEEK_TRAVEL_BUILD.md`
- [x] `rust-agent.md`

## Folder READMEs (step 4)

- [x] `README_pepper-crm.md`
- [x] `README_pepper-web.md`
- [x] `README_pepper.md`
- [x] `README_mcp-servers.md`
- [x] `README_contacts.md`
- [x] `README_assets.md`
- [x] `README_migrations.md`
- [x] `README_templates.md`

## Excluded (generated / duplicate / meta)

- `target/` — build output
- `.cache/` — geocode/travel cache
- `.env` — secrets
- `pepper-crm/contacts/` — duplicate of root `contacts/`
- `documentation_agent.md`, `documentation_subagent.md` — agent instructions (gitignored)
- `Cargo.lock` — generated lockfile

## Summary

| Category | Files documented |
|----------|------------------|
| Rust source + examples + tests | 28 |
| MCP + pepper binaries | 16 |
| pepper-web (rs, html, css, js) | 8 |
| Config (Cargo, .env, sql) | 3 |
| Project markdown | 10 |
| Folder READMEs | 8 |
| **Total with inline headers** | **73** |
| Folder-level only (VCF, PNG) | `contacts/*.vcf`, 2 PNGs |

Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.
