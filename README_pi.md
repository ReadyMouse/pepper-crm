<!--
# Pepper on the Pi — Owner's Manual

  How Pepper runs on the Raspberry Pi and how to update it after making
  changes on the Mac.

INPUT:
  - None (human-facing runbook)

OUTPUT:
  - Update routine, digest schedule reference, troubleshooting

NOTES:
  - Deployment history and architecture live in README_technical.md.

Written by Cursor for Ready Mouse and Pepper CRM. August 2026. All rights reserved.
-->

# Pepper on the Pi — Owner's Manual

Pepper runs on the Raspberry Pi (`kensington-estate.local`) as user `mylo`,
reading contacts from Radicale on the same machine and emailing a digest
**every 3 days**. The Mac is not involved — it can be off.

## The one-glance mental model

```
You edit code on the Mac ──git push──> GitHub ──git pull──> Pi rebuilds itself
You edit contacts on your phone ──DAVx⁵──> Radicale on the Pi ──> next digest
```

## Updating Pepper after you change code

On the Mac — commit and push like always:

```bash
git add -A && git commit -m "describe the change" && git push
```

On the Pi — one command does everything (pull, rebuild, sanity check):

```bash
ssh pepper-pi
~/pepper-crm/scripts/update-pepper-pi.sh
```

That's the whole routine. The script skips rebuilding when nothing changed,
and never touches your settings (`.env`), caches, logs, or the cron schedule.

**Template-only or script-only changes** still go through the same routine —
the rebuild is quick because Rust only recompiles what changed.

## The digest schedule

- Cron (under `crontab -l` as mylo) runs a check **hourly**; a digest is sent
  when the last successful send is **3+ days old**. Failures retry hourly and
  never advance the clock.
- Last-sent stamp: `~/pepper-crm/.cache/digest-last-sent` (epoch seconds).
  Delete it to make the next hourly check send.
- Send one right now: `DIGEST_FORCE=1 ~/pepper-crm/scripts/run-digest-every-3-days.sh`
- Logs: `~/pepper-crm/logs/digest-3day.log` (only send attempts are logged;
  silence between sends is normal).

## Checking on Pepper

```bash
ssh pepper-pi
cd ~/pepper-crm
./target/release/pepper --dry-run          # full rehearsal, sends nothing
./target/release/pepper --schedule-status  # what the scheduler thinks
tail logs/digest-3day.log                  # recent send attempts
crontab -l                                 # the hourly cron line
```

## When something's wrong

| Symptom | First thing to check |
|---|---|
| No digest for 4+ days | `tail logs/digest-3day.log` on the Pi — failed sends say why |
| Digest missing contacts/edits | Did the phone sync? Force a sync in DAVx⁵, then `--dry-run` |
| Update script fails at `git pull` | Uncommitted edits on the Pi — `git status` there; the Pi should never be edited directly |
| Build fails after an update | Run with `FORCE_BUILD=1`; if it persists, the error is in the pushed code |

## What is deliberately NOT on the Pi

- **Write-back** — `CONTACTS_READ_ONLY=true` in the Pi's `.env`; Pepper only
  reads Radicale. Enable writes only after the backup + write-path testing
  described in `README_technical.md`.
- **The dashboard** — `pepper-web` runs on the Mac (pointed at the Pi via
  CardDAV). To host it on the Pi later: `BUILD_WEB=1` during update, then see
  the systemd section in `README_technical.md`.
