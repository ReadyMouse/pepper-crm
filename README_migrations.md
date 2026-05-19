# migrations — Database Schema

## Purpose

SQL migration files for PostgreSQL. Pepper stores **task state only** (tasks, reconnects, digest log) — not a duplicate of vCard contact fields.

## Contents

| File | Description |
|------|-------------|
| `001_initial.sql` | Initial schema: contacts (UID refs), tasks, reconnects, digest_log |

## Apply

```bash
createdb pepper_crm
psql pepper_crm < migrations/001_initial.sql
```

## Open-source candidate

**Yes.** Standard PostgreSQL DDL with no proprietary extensions.
