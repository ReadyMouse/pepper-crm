# Pepper Web Dashboard

A localhost web UI for visualizing and testing your Pepper CRM data.

## Quick Start

```bash
# Make sure database is running and .env is configured
cargo run --bin pepper-web
```

Then open: **http://localhost:3000**

## Pages

- **Dashboard** (`/`) - Overview of pending tasks and due reconnects
- **Contacts** (`/contacts`) - Grid view of all contacts in the database
- **Digest Preview** (`/preview`) - Preview what the weekly email will look like

## Features

- 🎨 Same styling as the email digest
- 📊 Real-time data from PostgreSQL
- 🔄 Refresh to see latest data
- 📱 Responsive design

## Workflow

1. **Parse VCF files** - Run `pepper --dry-run` to sync contacts to DB
2. **View in browser** - Open http://localhost:3000 to see the data
3. **Test digest** - Go to `/preview` to see what the email will contain
4. **Send for real** - Run `pepper` (without --dry-run) to send the actual email

## Development

The web server uses:
- **Axum** - Fast web framework
- **Tera** - Template engine (same as email digest)
- **SQLx** - Direct database queries
- **pepper-crm** - Shared business logic library

All data comes from the same PostgreSQL database that the MCP servers use.
