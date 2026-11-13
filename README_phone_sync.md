<!--
# Pepper Phone Sync: Setting Up a New Android Phone

  How to connect a new Android phone to Radicale on the Pi so contact edits
  flow into Pepper.

INPUT:
  - None (human-facing runbook)

OUTPUT:
  - Step-by-step setup for F-Droid, Orbot, and DAVx5 on a new phone

NOTES:
  - First written while moving from an old phone to a Pixel 11 (November 2026).
  - Secrets (onion address, password) are deliberately NOT stored here.

Written by Claude for Ready Mouse and Pepper CRM. November 2026. All rights reserved.
-->

# Pepper Phone Sync: Setting Up a New Android Phone

How the pieces fit:

```
Phone Contacts ── DAVx5 ── Orbot (Tor) ── onion service ── FreedomBox ── Radicale ── Pepper
```

Orbot is the tunnel and DAVx5 is the courier. The courier can only reach the
Pi while the tunnel is open, so when sync fails, check Orbot first.

## What you need before starting

| Item | Where to find it |
|---|---|
| Onion address of the Pi | FreedomBox web UI (`https://kensington-estate.local/freedombox`, or `/plinth` on older versions) → Apps → Tor. Or over SSH: `sudo find /var/lib/tor* -name hostname -exec cat {} \;` |
| Radicale username | The `mylo` user, NOT the FreedomBox admin account. Radicale address books belong to the user in the URL path (`/radicale/mylo/...`) |
| Radicale password | The `mylo` user's password (or `CARDDAV_PASS` in `~/pepper-crm/.env` on the Pi) |
| Address book URL (best option) | Old phone: DAVx5 → account settings. Format: `http://<onion>.onion/radicale/mylo/<collection-id>/`. Copy and paste it to the new phone (Signal note-to-self, Quick Share); do not retype a 56-character onion address by hand |

Keep these in your password manager, not in this repo.

## 1. Back up Radicale

A new phone can push duplicates or stale cards. Take a backup first so any
damage is reversible:

```bash
ssh pepper-pi
sudo PEPPER_HOME=/home/mylo/pepper-crm ~/pepper-crm/scripts/backup-radicale.sh
```

Archives land in `~/pepper-crm/backups/radicale/`.

## 2. Install F-Droid

DAVx5 costs money on Google Play but is free on F-Droid (same app, same
developer). F-Droid itself is not in the Play Store, so install it from the web:

1. On the phone, open Chrome and go to **https://f-droid.org**.
2. Tap **Download F-Droid** and open the downloaded `.apk`.
3. Android asks to allow Chrome to install unknown apps. Tap **Settings**,
   turn on **Allow from this source**, then go back.
4. Tap **Install**. If Play Protect warns about an unknown app, choose
   **Install anyway** (F-Droid is open source and widely used).
5. Optional: turn the Chrome permission back off afterwards
   (Settings → Apps → Special app access → Install unknown apps → Chrome).
6. Open F-Droid and let it finish updating its app list.

> Note: Google's Android developer verification starts restricting apps
> from unregistered developers globally in 2027. If F-Droid installs get
> blocked on a future phone, check F-Droid's site for the current workaround.

## 3. Install DAVx5 and Orbot

- **DAVx⁵** (CardDAV/CalDAV sync): install from **F-Droid** (free there,
  paid on Play).
- **Orbot** (Tor for Android): install from **Google Play** (free). Orbot is
  not in F-Droid's main repo; it lives in the Guardian Project repo, which
  F-Droid no longer enables by default and which often lags behind.

Switching an app between Play and F-Droid later means uninstalling first,
because the two builds are signed with different keys.

## 4. Set up Orbot and point DAVx5 at it

DAVx5 talks to Orbot through Orbot's local SOCKS proxy. This setting lives in
DAVx5 and does **not** carry over to a new phone, so it is the step most
likely to be missed. Without it, login fails with "can't find CalDAV/CardDAV
service".

1. Open Orbot. Leave **VPN mode off** (the SOCKS proxy does the routing).
2. Tap **Connect** and wait for the connected status.
3. In DAVx5: **Settings → Connection → Override proxy settings** → on.
   - Proxy type: **SOCKS**
   - Host: `127.0.0.1`
   - Port: `9050`

Orbot must be connected whenever DAVx5 syncs.

### Onion client authorization (required for this Pi)

The Pi's onion service uses client authorization. Tor refuses to connect
without the private key, and DAVx5 logs `SOCKS server general failure` within
about a second. Check on the Pi:

```bash
sudo find /var/lib/tor* -type d -name authorized_clients -exec ls -la {} \;
```

Any `.auth` files listed mean client auth is on. The phone then needs the
matching private key in **Orbot → More → Client Auth (v3 Onion Services)**.
Copy it from the old phone's Orbot (same menu), or generate a new key pair on
the Pi and add the public half to `authorized_clients/`. Keep the private key
in a password manager, never in this repo.

To move the key from the old phone: in Orbot's Client Auth list, tap the entry
and choose **Backup/Share**. The backup is one line:
`<onion-without-.onion>:descriptor:x25519:<52-char private key>`. On the new
phone, import that file in Client Auth, or paste the onion address and the
part after `x25519:`. Restart Orbot afterwards. This is a Tor key, not an SSH
key.

## 5. Add the account in DAVx5

1. Open DAVx5 and tap **+**.
2. Under the generic login types, choose **URL and user name**
   (not Email or Advanced login).
3. Base URL: `http://<onion-address>.onion/radicale/`
   (or paste the full address book URL from the old phone).
4. Enter the username and password, then tap **Login**.
   Over Tor this can take 30 to 60 seconds.
5. **Account name**: DAVx5 suggests an email address, but any label works
   (for example `Pepper Pi`). It is only the name shown in Android's account
   list and Contacts app; email only matters for calendar invites.
6. Contact group method: **Groups are per-contact categories**. This is
   required: Pepper reads `Reconnect:` and engagement tags from the vCard
   `CATEGORIES` field, and this option is what maps phone labels to
   `CATEGORIES`. The other option stores groups as separate group vCards that
   Pepper never sees. It cannot be changed later without deleting and
   re-adding the account.
7. Tick the contacts address book and tap the sync button.

**First sync is slow.** Every contact comes down through Tor one by one
(about 1,900 contacts at the November 2026 setup). Keep the phone plugged in,
Orbot connected, and let it finish before testing anything.

## 6. Make the Pi the default for new contacts

In the Contacts app: **Settings → Default account for new contacts** → pick
the DAVx5 account. Otherwise new people are saved to Google and never reach
Pepper.

## 7. Stop Android from killing the sync

Settings → Apps → **DAVx⁵** → Battery → **Unrestricted**. Repeat for **Orbot**.

Pepper's stale-sync warning will eventually notice a dead sync, but this
prevents it.

## 8. Verify end to end

1. Create a test contact under the DAVx5 account and add a `TODO:` line in
   its notes.
2. Force a sync in DAVx5 (Orbot connected).
3. On the Pi:

```bash
ssh pepper-pi
cd ~/pepper-crm
./target/release/pepper --dry-run
```

The test contact's TODO should appear. For a direct server check, use
`scripts/check-pi-contact.sh` with the contact's UID (see
[`scripts/README_scripts.md`](scripts/README_scripts.md)).

## Troubleshooting

| Symptom | First thing to check |
|---|---|
| "Can't find CalDAV/CardDAV service" | DAVx5 proxy override missing: SOCKS, `127.0.0.1`, `9050` (step 4) |
| Log shows `SOCKS server general failure` or `SOCKS: Host unreachable` | DAVx5 reached Orbot fine, but Tor could not reach the onion service. Check Orbot is fully connected, then check the old phone still syncs; if not, the Pi's onion service is down (FreedomBox → Apps → Tor) |
| Log shows `Connection refused` on 9050 | Orbot is not running or uses a different SOCKS port (Orbot → More) |
| DAVx5 login times out | Is Orbot connected? Is another always-on VPN (Tailscale, Proton) taking over? |
| Certificate warning | FreedomBox uses a self-signed cert; accept it in DAVx5 |
| 401 Unauthorized | Wrong username/password; test by logging in to the FreedomBox web UI |
| Syncs once, then stops | Battery optimization; set DAVx5 and Orbot to Unrestricted |
| Duplicate contacts | Restore the backup from step 1, delete the DAVx5 account, re-add it |
