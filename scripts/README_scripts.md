<!--
# scripts — Deployment and Ops Helpers

  Shell scripts for running Pepper on the Pi: updates, digest cron, backups,
  and CardDAV debugging.

INPUT:
  - None (human-facing folder overview)

OUTPUT:
  - Pointers to update, digest scheduler, backup, and debug scripts

NOTES:
  - Scripts expect `PEPPER_HOME` to point at the repo root on the Pi.

Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.
-->

# scripts — Deployment and Ops Helpers

## Purpose

Operational scripts for the live Pi deployment (updating, digest scheduling, backups) plus debugging helpers and alternative schedulers.

## Contents

**In production:**

| Path | Role |
|------|------|
| `update-pepper-pi.sh` | Run on the Pi: `git pull` + rebuild `pepper` + sanity check; installs Rust on first run |
| `run-digest-every-3-days.sh` | Digest sender (Pi cron or Mac launchd): hourly stamp check, sends when last success was 3+ days ago |

**Ready when needed:**

| Path | Role |
|------|------|
| `backup-radicale.sh` | Nightly tar of Radicale collections with retention pruning — required before enabling write-back |
| `pepper-web.service` | systemd unit keeping the dashboard running on the Pi (binds `0.0.0.0:3000` for tailnet access) |
| `check-pi-contact.sh` | Read-only CardDAV probe: fetch one contact straight from Radicale to debug phone-sync issues |
| `com.pepper.digest.plist` | macOS LaunchAgent for the 3-day digest — fallback if the Pi is ever out of service |

**Alternative scheduler (Monday 6:00, trip timezone):**

| Path | Role |
|------|------|
| `install-weekly-cron.sh` | Add/replace hourly cron entry for the weekly digest runner |
| `run-weekly-digest.sh` | Wrapper that invokes `pepper --send-if-due` and logs to `logs/weekly-digest.log` |
| `crontab.pepper.example` | Sample crontab line (prefer `install-weekly-cron.sh`) |

## Usage

```bash
# On the Pi — update Pepper after pushing changes from the laptop
~/pepper-crm/scripts/update-pepper-pi.sh   # pull + rebuild + sanity check

# On the Pi — the every-3-days digest cron (current production schedule)
(crontab -l 2>/dev/null; echo "0 * * * * PEPPER_HOME=$HOME/pepper-crm $HOME/pepper-crm/scripts/run-digest-every-3-days.sh") | crontab -
DIGEST_FORCE=1 ./scripts/run-digest-every-3-days.sh   # force a send now
# Logs: logs/digest-3day.log — stamp: .cache/digest-last-sent (delete to force next check to send)

# On the Pi — nightly Radicale backup (root cron; PUT writes become reversible)
sudo crontab -e   # 15 3 * * * PEPPER_HOME=/home/mylo/pepper-crm /home/mylo/pepper-crm/scripts/backup-radicale.sh

# On the Pi — dashboard as a service (when the dashboard moves to the Pi)
sudo cp scripts/pepper-web.service /etc/systemd/system/
sudo systemctl enable --now pepper-web

# On a Mac — 3-day digest via launchd (fallback when the Pi is down)
cp scripts/com.pepper.digest.plist ~/Library/LaunchAgents/
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.pepper.digest.plist

# Alternative: Monday-morning weekly scheduler instead of every-3-days
PEPPER_HOME=~/pepper-crm ./scripts/install-weekly-cron.sh
DIGEST_FORCE=1 ./scripts/run-weekly-digest.sh
```

## Open-source candidate

**Yes.** Generic bash helpers with no proprietary logic; paths and env vars are configurable.

## Related docs

- [`README_technical.md`](../README_technical.md) — Raspberry Pi deployment and cron sections
- [`README_pepper.md`](../pepper/README_pepper.md) — `pepper` CLI flags (`--send-if-due`, `--schedule-status`)
