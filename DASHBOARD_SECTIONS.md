<!--
# Pepper — Dashboard Feature Spec

  Product spec for dashboard and digest sections; not web implementation detail.

INPUT:
  - pepper-crm data sources (VCF tags, calendar, geo)

OUTPUT:
  - Feature definitions, display order, implementation priority

NOTES:
  - Status lines reflect pepper-web as of May 2026. See IMPLEMENTATION_STATUS.md for crate-level checklist.

Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.
-->

# Pepper — Dashboard Feature Spec

Product feature spec for what Pepper surfaces on the dashboard and in the weekly digest. This is a **design document**, not web implementation detail.

**Dashboard (`pepper-web`):** all four sections are live. **Weekly digest email:** Pending Tasks + Reconnects Due only (no travel or random picks yet).

### Implementation summary

| Section | Dashboard | Digest email | Notes |
|---------|-----------|--------------|-------|
| Random People of the Week | ✅ Live (partial) | 🔜 | Three picks/week, not one; manual discovery links, not API enrichment |
| Reconnects Due | ✅ Live (partial) | ✅ | Snooze → VCF write-back; no “log interaction” or `.ics` from UI |
| Pending Tasks | ✅ Live (partial) | ✅ | Read-only list; no mark-done/snooze in UI |
| Next Week Travel | ✅ Live (partial) | 🔜 | Stricter eligibility than original geo-only sketch |

---

### Engagement categories

Contacts can carry vCard **CATEGORIES** values that limit where they appear. See [`README.md`](README.md#engagement-categories-in-categories) for full definitions.

| Category | Reconnects Due | Next Week Travel | Random Person | Birthdays (planned) | Any search |
|----------|----------------|------------------|---------------|---------------------|------------|
| *(none)* | Per `Reconnect:` tag | Yes (if due) | Yes | Yes | Yes |
| `Reconnect: Never` | No | No | Yes | Yes | Yes |
| `Do Not Engage` | No | No | No | No | No |

**Built behavior matches this table** (`pepper-crm/src/tags.rs`, wired in `pepper-web`).

---

## 1. Pending Tasks

**Status:** ✅ Live (read-only)  
**Data source:** VCF `TODO:` tags → PostgreSQL `tasks` table (`get_due_tasks`)  
**Digest parity:** Yes — same list as the weekly email

### What it shows
All open `pending` tasks across contacts: who and TODO text.

### Built (`pepper-web/templates/dashboard.html`, `fetch_due` in `main.rs`)
- Lists tasks from PostgreSQL after VCF sync on startup
- Excludes contacts with `Do Not Engage`
- Summary stat: pending task count

### Not built yet
- How long each task has been pending
- Mark done / snooze from the UI
- Write-back optional notes to the contact's VCF

---

## 2. Reconnects Due

**Status:** ✅ Live (partial)  
**Data source:** VCF `Reconnect:` tags — computed from synced contacts via `due_reconnects_from_contacts()` (7-day window)  
**Digest parity:** Yes — same list as the weekly email (digest adds `.ics` attachments on send)

### What it shows
Contacts whose timed reconnect interval is due on or before the next **7 days** (anchor: vCard `REV`, or latest past `Month YYYY:` note).

### Eligibility (built)
- Exclude `CATEGORIES:Reconnect: Never`
- Exclude `CATEGORIES:Do Not Engage`
- Exclude venue/business cards and city-trip triggers (`before Chicago trip`)

### Built
- List with due date and reconnect tag
- Snooze dropdown → `POST /travel/snooze` writes new `Reconnect:` interval to VCF and removes from list
- Summary stat: reconnects due count

### Not built yet
- One-click “log interaction” → append CRM log to VCF, reset reconnect tag
- Attach `.ics` calendar invite from dashboard (digest send path has this)

---

## 3. Random People of the Week

**Status:** ✅ Live (partial) — spec originally described **one** person; dashboard ships **three** per ISO week  
**Data source:** Random selection from contacts in memory (VCF sync) + assistive action links

### What it shows
Up to **three** contacts chosen at random for the ISO week containing today — serendipitous reconnection, not interval reminders. **Shuffle 3 new** picks a fresh trio on demand (cached under `.cache/random_pick/`).

### Built (`pepper-crm/src/random_pick.rs`, dashboard section)
- Weekly stable seed (same trio until Monday) + manual shuffle
- Excludes `Do Not Engage` and `Venue/Business`; **`Reconnect: Never` is eligible**
- Card shows name, org, location, reconnect tag, note preview, email/phone, LinkedIn if already on vCard `URL:`
- **Suggested actions** (assistive only — nothing auto-written except Schedule reconnect):
  - Search the web (Google query from name + org + city)
  - Open LinkedIn profile (only if URL already on card)
  - Search GitHub
  - Send a check-in message (`mailto:` draft when email present)
  - Schedule reconnect: 1 month (snooze → VCF, same as travel snooze)

### Spec vs built (gaps)

| Spec idea | Built? |
|-----------|--------|
| One person per week | **No** — three picks (`RANDOM_PICK_COUNT = 3`) |
| Weighted toward no recent CRM log / long sync | **No** — uniform random among eligible contacts |
| Web/LinkedIn API enrichment with “Apply to vCard” | **No** — manual search links; LinkedIn discovery deferred until LLM disambiguation |
| Enrichment result cache (~7 days) | **No** |
| Auto-suggest social URLs to add to card | **No** |

### Future behavior (still planned)
- Optional single-person mode or smarter weighting
- API-backed enrichment with approve-before-write
- Rate limits / ToS-safe search integration

### Example output (current)
```
Random People of the Week: May 19 – May 25, 2026
  Alice Smith (Acme Corp · Chicago, IL)
  Suggested: [Search the web] [Search GitHub] [Send a check-in] [Schedule reconnect: 1 month]
```

---

## 4. Next Week Travel

**Status:** ✅ Live (partial)  
**Data source:** Google Calendar ICS (`GOOGLE_CALENDAR_ICS_URL`) + contact geo from VCF `ADR` / geocoded `GEO`  
**Digest parity:** No — dashboard only; digest template has no travel section yet

### What it shows
Trips for the target calendar week and contacts you could meet in each trip’s metro area. Rebuild **on demand** (`POST /travel/refresh`) or via `pepper` weekly run — **not** on every page load. Snapshot: `.cache/travel/{iso_week}.json`.

### Eligibility (built — stricter than original geo-only sketch)
- Exclude `Reconnect: Never` and `Do Not Engage`
- Exclude venues/business cards
- Contact must have a **recent interaction anchor** (vCard `REV` or `Month YYYY:` note within ~18 months)
- Contact’s timed `Reconnect:` interval must be **due** (or due within travel window) — not every person in the metro

### Built
- Calendar trip list with date ranges; trip location geocoded from event title/location
- Metro-radius matching (default ~50 mi, configurable on refresh form)
- Haversine distance; ranking favors `before [city] trip` tags, then proximity
- Per-match snooze (VCF write-back)
- Summary stat: travel match count (when snapshot exists)
- Optional GEO write-back to VCF files during build

### Spec vs built (gaps)

| Spec idea | Built? |
|-----------|--------|
| Soft geo / metro matching | **Yes** (`geo.rs`, `travel.rs`) |
| Show all contacts in metro while traveling | **Partial** — only reconnect-due + recent-anchor contacts |
| “Draft outreach email” / map view | **No** |
| Travel section in weekly digest email | **No** |

### Example output (current)
```
Next Week Travel · Updated May 19, 2026 14:32
  Chicago, IL · May 26–29
    James Martinez — Evanston (~15 mi) · Reconnect: 3 months
    [Snooze ▼]
```

---

## Display order

**Built order** (`pepper-web/templates/dashboard.html`):

1. Summary stats (travel matches, pending tasks, reconnects due, upcoming birthdays)  
2. Next Week Travel  
3. Pending Tasks  
4. Reconnects Due  
5. Random People of the Week  
6. Upcoming Birthdays (next 14 days, vCard `BDAY`)

---

## Implementation priority (original → current)

| Priority | Item | State |
|----------|------|-------|
| 1 | Pending Tasks + Reconnects Due | ✅ Done |
| 2 | Weekly digest (email) | ✅ Done (tasks + reconnects) |
| 3 | Next Week Travel | ✅ Done (dashboard); digest section 🔜 |
| 4 | Random Person + enrichment API | 🔶 Dashboard picks + manual links; API enrichment 🔜 |

---

## Code map (verification)

| Spec section | Primary implementation |
|--------------|------------------------|
| Pending Tasks | `pepper-crm/src/db.rs` (`get_due_tasks`), `pepper-web/src/main.rs` (`fetch_due`) |
| Reconnects Due | `pepper-crm/src/tags.rs` (`due_reconnects_from_contacts`) |
| Random picks | `pepper-crm/src/random_pick.rs`, `POST /random/shuffle` |
| Next Week Travel | `pepper-crm/src/travel.rs`, `travel_cache.rs`, `POST /travel/refresh` |
| Digest | `templates/digest.html`, `mcp-digest-server` |
