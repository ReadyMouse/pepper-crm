# contacts — Local VCF Fixtures

## Purpose

Directory of vCard (`.vcf`) files used as the people store during local prototyping. Pepper reads these at runtime; tasks and reconnect state also live in vCard fields.

## Contents

| Pattern | Description |
|---------|-------------|
| `contact_01.vcf`–`contact_20.vcf` | Generated test scenarios (TODO, reconnect, trip triggers, CRM logs) |
| `reconnect_due_*.vcf` | Focused fixtures for reconnect due-date edge cases |
| `contacts.vcf` | Real export (gitignored pattern in `.gitignore`) |

## Why no per-file doc headers

vCard parsers expect `BEGIN:VCARD` as the first meaningful line. Inline comment headers would break import into Contacts.app and other tools. This folder README documents the collection instead.

## Regenerating test contacts

```bash
cargo test -p pepper-crm --test generate_test_contacts -- --ignored
```

## Tag conventions

- `TODO:` lines in `NOTE`
- `Reconnect:` in `CATEGORIES` (e.g. `CATEGORIES:Reconnect: 3 months`)
- CRM log appended below `--- CRM Log ---`

### Engagement categories

Set on vCard `CATEGORIES` (same field as `Reconnect:` tags).

| Category | Meaning | Still in VCF? |
|----------|---------|---------------|
| `Reconnect: Never` | No timed reconnect nudges; close contacts (e.g. mom). May appear in Random Person of the Week and birthday reminders. **Never** in Reconnects Due or Next Week Travel. | Yes |
| `Do Not Engage` | Never surface anywhere — no suggestions, travel, random pick, or search. Keep the card for your records only. | Yes |

Examples:

```vcf
CATEGORIES:Reconnect: Never
```

```vcf
CATEGORIES:Do Not Engage
```

Full rules: [`README_technical.md` — Engagement categories](../README_technical.md#engagement-categories).

## Open-source candidate

**N/A** — sample data only; real exports should stay gitignored.
