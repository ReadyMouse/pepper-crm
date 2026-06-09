<p align="center">
  <img src="assets/brand/pepper_avatar_teal.png" alt="Pepper" width="180">
</p>

# Pepper — Personal CRM

**Pepper** helps you stay in touch with your network using the contact app you already have. It reads plain-text tags in your vCards — `TODO:` follow-ups and `Reconnect:` schedules — matches people to trips on your calendar, and sends a weekly email digest with reminders. No separate CRM database.

**Day to day:** open the dashboard at **http://localhost:3000** to see who's due, who's near your next trip, and birthdays coming up.

**Each week:** `pepper` emails you a digest (or run `--dry-run` to preview first).

## How it's meant to run

On a **Raspberry Pi** with **[Radicale](https://radicale.org/)** (CardDAV) and your phone synced via **[DAVx⁵](https://www.davx5.com/)** (Android) or a built-in CardDAV account:

```
Phone Contacts  ↔  CardDAV sync  ↔  Radicale  ↔  Pepper
```

Edit tags on your phone; Pepper reads the same vCards. For trying it out on your laptop, point Pepper at a folder of `.vcf` files instead — no Pi required.

→ Deployment, env vars, Vagrant, and Pi cron: [`README_technical.md`](README_technical.md)

## Quick start

**Requirements:** [Rust](https://rustup.rs/)

```bash
cp .env.example .env    # set CONTACTS_DIR, SMTP_*, DIGEST_RECIPIENT as needed
cargo build --workspace

cargo run --bin pepper-web              # dashboard → http://localhost:3000
./target/debug/pepper --dry-run         # preview weekly email (no send)
./target/debug/pepper                   # send digest
```

**Travel matches:** add `GOOGLE_CALENDAR_ICS_URL` to `.env`, then click **Refresh travel matches** on the dashboard.

**Test data:** `contacts/` has 20 sample vCards. Regenerate with:

```bash
cargo test -p pepper-crm --test generate_test_contacts -- --ignored
```

**CardDAV without a Pi:** A [Vagrant](https://www.vagrantup.com/) VM (Debian + FreedomBox + Radicale) lets you exercise Pepper’s CardDAV read/write against fake contacts before touching your phone or Pi. Requires VirtualBox. Point `CARDDAV_*` in `.env` at `https://localhost:8443/radicale/admin/test-contacts/`, set `CONTACTS_READ_ONLY=false` and `GEO_WRITE_TO_VCF=true`, then run the `carddav_*` examples or `pepper-web`. Full setup, smoke tests, and troubleshooting: [`README_technical.md#local-homelab-vagrant`](README_technical.md#local-homelab-vagrant).

## Tag your contacts

Edit these in any contacts app (Notes and Categories fields).

### Follow-ups — `TODO:` in Notes

```
July 2026: Met at conference.

TODO: send intro email
```

### Reconnect schedule — `Reconnect:` in Categories

```
Reconnect: 1 week
Reconnect: 1 month
Reconnect: 3 months
Reconnect: 6 months
Reconnect: Never          # close contacts — no "due soon" nudge
Do Not Engage             # keep on file, never suggest
```

Timed reconnects use `REV` or a `Month YYYY:` note line (e.g. `May 2026: Had coffee`) as the anchor date.

→ Full tag rules and surface tables: [`README_technical.md#tag-format-full-spec`](README_technical.md#tag-format-full-spec)

## What Pepper does today

| | |
|--|--|
| **Dashboard** | Tasks due, reconnects due (7 days), travel matches, random picks, birthdays, digest preview |
| **Weekly email** | HTML digest + `.ics` attachments for due reconnects |
| **Writes back** | Snooze reconnect, mark task done, geocode addresses (local VCF or CardDAV) |
| **Planned** | Agent-driven interaction log (`--- CRM Log ---`) via MCP / Matrix bot |

## More docs

| Doc | For |
|-----|-----|
| [`README_technical.md`](README_technical.md) | Architecture, env vars, CardDAV, Vagrant, Pi cron |
| [`personal_crm_design.md`](personal_crm_design.md) | Full design doc |

## License

MIT
