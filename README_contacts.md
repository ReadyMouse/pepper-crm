# contacts — Local VCF Fixtures

## Purpose

Directory of vCard (`.vcf`) files used as the people store during local prototyping. Pepper reads these at runtime; PostgreSQL holds task state only, not contact profile duplicates.

## Contents

| Pattern | Description |
|---------|-------------|
| `contact_01.vcf`–`contact_20.vcf` | Generated test scenarios (TODO, reconnect, trip triggers, CRM logs) |
| `reconnect_due_*.vcf` | Focused fixtures for reconnect due-date edge cases |
| `contacts.vcf` | Real export (gitignored pattern in `.gitignore`) |
| `pepper_test.vcf` | Manual test contact |

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

## Open-source candidate

**N/A** — sample data only; real exports should stay gitignored.
