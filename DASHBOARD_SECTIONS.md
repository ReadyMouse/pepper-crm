<!--
# Pepper — Dashboard Feature Spec

  Product spec for dashboard and digest sections; not web implementation detail.

INPUT:
  - pepper-crm data sources (VCF tags, calendar, geo)

OUTPUT:
  - Feature definitions, display order, implementation priority

NOTES:
  - Sections marked Coming Soon in spec may now be live; see IMPLEMENTATION_STATUS.md.

Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.
-->

# Pepper — Dashboard Feature Spec

Product feature spec for what Pepper surfaces on the dashboard and in the weekly digest. This is a **design document**, not web implementation detail.

Every feature below is marked **Coming Soon** in the UI until built.

---

## 1. Pending Tasks

**Status:** Coming Soon  
**Data source:** VCF `TODO:` tags → PostgreSQL `tasks` table  
**Digest parity:** Yes — same list as the weekly email

### What it shows
All open tasks across contacts: who, what TODO text, and (eventually) how long it's been pending.

### Future behavior
- Pull from `get_due_tasks()` (already in `pepper-crm`)
- Mark done / snooze from the UI
- Write-back optional notes to the contact's VCF

---

## 2. Reconnects Due

**Status:** Coming Soon  
**Data source:** VCF `Reconnect:` tags → PostgreSQL `reconnects` table  
**Digest parity:** Yes — same list as the weekly email

### What it shows
Contacts whose reconnect reminder falls within the current window (default: next 7 days).

### Future behavior
- Pull from `get_due_reconnects()`
- One-click "log interaction" → append CRM log to VCF, reset reconnect tag
- Attach `.ics` calendar invite (same as digest)

---

## 3. Random Person of the Week

**Status:** Coming Soon  
**Data source:** Random selection from contacts DB + external enrichment

### What it shows
One contact chosen at random each week — someone you might have drifted away from. The goal is **serendipitous reconnection**, not just due-date reminders.

### Enrichment pipeline (planned)

1. **Pick** a contact (weighted toward: no recent CRM log, long time since sync, or explicit "reconnect someday" tags with no due date). Do not pick anyone with "Reconnect: Never" tag.
2. **Search** the public web using name + org + city from the VCF:
   - Google / web search API
   - LinkedIn profile URL discovery
   - Twitter/X, GitHub, personal site, company page
3. **Present** findings as suggested fields to add to the contact card:
   - LinkedIn URL → `URL` or custom field in VCF
   - Social handles → `NOTE` or structured tags
   - Recent headline / role change → summary blurb
4. **Suggest actions:**
   - "Add LinkedIn to contact card"
   - "Send a check-in message"
   - "Schedule reconnect: 1 month"

### Constraints
- Enrichment is **assistive, not automatic** — you approve before anything is written to the VCF.
- Respect rate limits and ToS for search APIs.
- Cache enrichment results (TTL ~7 days) to avoid re-querying every page load.

### Example output (future)
```
Random Person of the Week: Alice Smith (Acme Corp)
  Found: linkedin.com/in/alicesmith · github.com/alice
  Suggested: Add LinkedIn to contact card  [Apply]  [Dismiss]
```

---

## 4. Next Week Travel

**Status:** Coming Soon  
**Data source:** Your calendar (future) + contact geo data from VCF `ADR` / city fields

### What it shows
Where you'll be next week, and **who you could meet while there** — dinners, coffee, quick catch-ups.

### Calendar integration (planned)
- Read upcoming events from Google Calendar / Apple Calendar / `.ics` feed
- Extract **destination city** from event location or title (e.g. "Chicago trip", "ETHDenver")
- Show: "Next week you're in **Chicago** (Mon–Thu)"

### Soft geo matching (planned)

Addresses in VCFs rarely match where you actually travel. Matching must be **metro-area aware**, not exact city string equality.

| You are in | Contact address | Match? |
|------------|-----------------|--------|
| Chicago, IL | Chicago, IL | ✅ exact |
| Chicago, IL | Evanston, IL | ✅ same metro (~20 mi) |
| Chicago, IL | Littleton, CO | ❌ different metro |
| Denver, CO | Littleton, CO | ✅ Denver metro (~30 min) |
| Denver, CO | Boulder, CO | ✅ Denver metro |

### Algorithm sketch
1. Geocode your travel city → lat/lng + metro radius (default ~50 km / ~30 mi, configurable).
2. Geocode each contact's city from `ADR` (or parse from `NOTE` if missing).
3. Haversine distance ≤ threshold → **suggest reach-out**.
4. Rank by: existing `Reconnect: before [city] trip` tags first, then proximity, then time since last log.

### Suggested actions (future)
- "Email Alice — she's in the Denver area, you're there Tue–Wed"
- "Schedule dinner with Bob" → draft `.ics` invite
- Contacts with `Reconnect: before Chicago trip` auto-surface when Chicago is detected

### Example output (future)
```
Next Week Travel: Chicago, IL (May 26–29)
  3 people in the Chicago metro you could meet:
    · James Martinez — Evanston (~15 mi)
    · Sofia Chen — Chicago
    · Liam Anderson — tagged "before Chicago trip"
  [Draft outreach email]  [View on map]
```

---

## Display order

1. Random Person of the Week  
2. Reconnects Due  
3. Pending Tasks  
4. Next Week Travel  

Summary stats at the top: pending task count, reconnects due, travel matches.

---

## Implementation priority (suggested)

1. **Pending Tasks + Reconnects Due** — wire up existing `pepper-crm` queries (data already syncs on startup)
2. **Weekly digest** — email send path (MCP servers + `pepper` runner)
3. **Next Week Travel** — calendar read + geocoding service
4. **Random Person of the Week** — random pick + search enrichment API
