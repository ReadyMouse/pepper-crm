<!--
# scripts — Deployment and Ops Helpers

  Shell scripts for Pi deployment, cross-compilation, and weekly digest cron.

INPUT:
  - None (human-facing folder overview)

OUTPUT:
  - Pointers to build, cron install, and digest runner scripts

NOTES:
  - Scripts expect `PEPPER_HOME` to point at the repo root on the Pi.

Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.
-->

# scripts — Deployment and Ops Helpers

## Purpose

Operational shell scripts for cross-compiling Pepper binaries for Raspberry Pi, installing the weekly digest cron job, and running the calendar-aware send window from cron.

## Contents

| Path | Role |
|------|------|
| `build-linux-arm64.sh` | Cross-compile `pepper` and `pepper-web` for aarch64 Linux from macOS |
| `install-weekly-cron.sh` | Add/replace hourly cron entry for the digest runner |
| `run-weekly-digest.sh` | Wrapper that invokes `pepper --send-if-due` and logs to `logs/weekly-digest.log` |
| `crontab.pepper.example` | Sample crontab line (prefer `install-weekly-cron.sh`) |

## Usage

```bash
# Cross-compile for Pi 5
./scripts/build-linux-arm64.sh

# On the Pi — install hourly cron
PEPPER_HOME=~/pepper-crm ./scripts/install-weekly-cron.sh

# Force send now (ignore schedule)
DIGEST_FORCE=1 ./scripts/run-weekly-digest.sh
```

## Open-source candidate

**Yes.** Generic bash helpers with no proprietary logic; paths and env vars are configurable.

## Related docs

- [`README_technical.md`](../README_technical.md) — Raspberry Pi deployment and cron sections
- [`README_pepper.md`](../pepper/README_pepper.md) — `pepper` CLI flags (`--send-if-due`, `--schedule-status`)
