# pepper — Weekly Orchestrator

## Purpose

CLI binary that runs the full weekly CRM flow by spawning MCP servers over stdio and chaining tool calls: parse VCF → sync DB → get due items → render digest → export `.ics` → send email → build travel snapshot.

## Contents

| Path | Role |
|------|------|
| `src/main.rs` | MCP client orchestration, clap CLI |
| `Cargo.toml` | Depends on `pepper-crm`, `rmcp`, `clap` |

## CLI flags

- `--dry-run` — preview without sending email
- `--recipient` — override digest recipient
- `--force-travel` — rebuild travel snapshot
- `--contacts-dir` — VCF directory override

## Scheduled run (cron)

Production schedule: **Monday 06:00** in your **trip timezone** (from `GOOGLE_CALENDAR_ICS_URL`), default **US Eastern**. Hourly cron runs `pepper --send-if-due`.

```bash
./pepper --schedule-status          # monday, timezone, send_window_active
./scripts/install-weekly-cron.sh    # hourly crontab → run-weekly-digest.sh
```

See [README_technical — Weekly digest cron](../README_technical.md#weekly-digest-cron-monday-600-trip-timezone).

## Open-source candidate

**Yes.** Thin orchestrator; logic lives in `pepper-crm`. MCP server binaries are optional for agent workflows; the weekly CLI calls the library directly.
