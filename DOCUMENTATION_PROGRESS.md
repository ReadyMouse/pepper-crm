# Documentation Progress

Checklist for the documentation agent run. **Last updated:** May 2026 (re-run).

Legend: `[x]` done · `[-]` folder-level only (no safe inline header)

## Root

- [x] `Cargo.toml`
- [x] `.env.example`
- [x] `README.md`
- [x] `templates/digest.html`

## pepper-crm (library crate)

- [x] `pepper-crm/Cargo.toml`
- [x] `pepper-crm/src/lib.rs`
- [x] `pepper-crm/src/models.rs`
- [x] `pepper-crm/src/vcard.rs`
- [x] `pepper-crm/src/carddav.rs`
- [x] `pepper-crm/src/tags.rs`
- [x] `pepper-crm/src/tasks.rs`
- [x] `pepper-crm/src/ical.rs`
- [x] `pepper-crm/src/calendar.rs`
- [x] `pepper-crm/src/digest.rs`
- [x] `pepper-crm/src/digest_schedule.rs`
- [x] `pepper-crm/src/mail.rs`
- [x] `pepper-crm/src/weekly.rs`
- [x] `pepper-crm/src/geo.rs`
- [x] `pepper-crm/src/contact_geo.rs`
- [x] `pepper-crm/src/travel.rs`
- [x] `pepper-crm/src/travel_cache.rs`
- [x] `pepper-crm/src/birthdays.rs`
- [x] `pepper-crm/src/random_pick.rs`
- [x] `pepper-crm/src/data_enrichment.rs`
- [x] `pepper-crm/examples/list_due_reconnects.rs`
- [x] `pepper-crm/examples/parse_contacts.rs`
- [x] `pepper-crm/examples/test_calendar.rs`
- [x] `pepper-crm/examples/carddav_list.rs`
- [x] `pepper-crm/examples/geocode_contacts.rs`
- [x] `pepper-crm/examples/random_pick_smoke.rs`
- [x] `pepper-crm/examples/test_smtp.rs`
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
- [x] `pepper-web/tests/dashboard_render.rs`

## MCP servers

- [x] `mcp-vcard-server/Cargo.toml`
- [x] `mcp-vcard-server/src/main.rs`
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

## scripts

- [x] `scripts/build-linux-arm64.sh`
- [x] `scripts/install-weekly-cron.sh`
- [x] `scripts/run-weekly-digest.sh`
- [x] `scripts/crontab.pepper.example`

## assets

- [-] `assets/brand/pepper_avatar_teal.png` — documented in `assets/README_assets.md`
- [-] `assets/brand/pepper_avatar_white.png` — documented in `assets/README_assets.md`
- [x] `assets/README.md`
- [x] `assets/brand/README.md`

## contacts (test VCF fixtures)

- [-] `contacts/*.vcf` — folder-level only (`contacts/README_contacts.md`); inline headers break vCard parsers
- [-] `pepper-crm/contacts/contact_*.vcf` — duplicate test fixtures; same rule

## Project docs

- [x] `personal_crm_design.md`
- [x] `DASHBOARD_SECTIONS.md`
- [x] `IMPLEMENTATION_STATUS.md`
- [x] `PEPPER_WEB_SUMMARY.md`
- [x] `NEXT_WEEK_TRAVEL_BUILD.md`
- [x] `rust-agent.md`

## Folder READMEs (step 4)

- [x] `pepper-crm/README_pepper-crm.md`
- [x] `pepper-web/README_pepper-web.md`
- [x] `pepper/README_pepper.md`
- [x] `mcp-servers/README_mcp-servers.md`
- [x] `contacts/README_contacts.md`
- [x] `assets/README_assets.md`
- [x] `templates/README_templates.md`
- [x] `scripts/README_scripts.md`

## Excluded (generated / duplicate / meta)

- `target/` — build output
- `.cache/` — geocode/travel cache
- `.env` — secrets
- `documentation_agent.md`, `documentation_subagent.md` — agent instructions (gitignored)
- `Cargo.lock` — generated lockfile
- `LICENSE` — standard MIT text
- `.gitignore` — ignore rules

## Summary

| Category | Files documented |
|----------|------------------|
| Rust source + examples + tests | 36 |
| MCP + pepper binaries | 14 |
| pepper-web (rs, html, css, js) | 9 |
| scripts | 4 |
| Config (Cargo, .env) | 2 |
| Project markdown | 6 |
| Folder READMEs | 8 |
| **Total with inline headers** | **79** |
| Folder-level only (VCF, PNG) | `contacts/*.vcf`, 2 PNGs |

Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.
