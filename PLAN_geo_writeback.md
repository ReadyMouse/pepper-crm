# Plan: Enable GEO Write-Back to Contact Cards

*Drafted Aug 9, 2026 — not yet executed. Goal: Pepper writes geocoded lat/lng
back to the vCards in Radicale so travel matching stops re-geocoding contacts
every few days.*

## Why

Today every digest run geocodes into a local cache (`.cache`, 7-day TTL), so
the Pi keeps re-asking Nominatim for the same addresses forever. Once
coordinates live on the cards themselves (`GEO:` field), future runs see them
and skip geocoding entirely — and the coordinates sync everywhere with the
card.

## What the code already supports (verified Aug 9)

- `write_contact_geo` PUTs the updated card back to Radicale, adding
  `GEO:lat;lng` plus `X-PEPPER-GEO-SOURCE:` (records which address produced
  the pin, so an address change triggers a re-geocode of just that card).
- The single master gate is `CONTACTS_READ_ONLY` in the Pi's `.env` — it
  blocks **all** writes. `GEO_WRITE_TO_VCF` is already on by default.
- Geo writes do **not** bump the card's `REV`, so reconnect schedules
  (anchored on `REV`) and the sync-staleness warning stay honest.

## Steps (in order, all on the Pi unless noted)

1. **Back up Radicale first** — no writes without a backup:

   ```bash
   ~/pepper-crm/scripts/backup-radicale.sh
   ```

2. **Sync the phone once** (DAVx⁵, with Orbot running) to flush pending phone
   edits *before* Pepper starts writing — minimizes same-card conflict risk.

3. **Update the Pi's code** (also delivers the digest crash fix if not yet
   deployed):

   ```bash
   ~/pepper-crm/scripts/update-pepper-pi.sh
   ```

4. **Flip the switch**: in `~/pepper-crm/.env`, remove or comment out
   `CONTACTS_READ_ONLY=1`.

   ⚠️ This opens **all** writes, not just geo — dashboard snooze/category
   buttons become live too. There is no narrower geo-only flag today.
   *Open decision: add a geo-only write flag first if we want to open the
   door one inch at a time.*

5. **Run the first write pass**:

   ```bash
   cd ~/pepper-crm && ./target/release/pepper --force-travel --dry-run
   ```

   Geocodes every contact missing coordinates and writes them back to
   Radicale. Nominatim allows ~1 lookup/second, so the first pass may take
   several minutes; every run after that is fast.

6. **Verify**:
   - `scripts/check-pi-contact.sh` — pull one card straight from Radicale and
     confirm it has `GEO:` and `X-PEPPER-GEO-SOURCE:` lines.
   - Sync the phone and spot-check that a contact still looks right in the
     contacts app; confirm the contact count is unchanged.

## Follow-up once trusted

- Put `backup-radicale.sh` on a nightly cron — Pepper will now be writing to
  the source of truth regularly.
