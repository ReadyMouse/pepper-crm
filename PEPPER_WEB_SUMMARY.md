# Pepper Web Dashboard - Implementation Summary

## ✅ What Was Built

A localhost web dashboard for visualizing and testing your Pepper CRM data.

### Features

1. **Dashboard Page** (`http://localhost:3000/`)
   - Stats cards showing count of pending tasks and reconnects
   - List of all pending tasks
   - List of all due reconnects
   - Clean, modern UI with the same styling as email digest

2. **Contacts Page** (`http://localhost:3000/contacts`)
   - Grid view of all contacts from database
   - Shows name, email, phone, org, city
   - Responsive card layout
   - Hover effects

3. **Digest Preview** (`http://localhost:3000/preview`)
   - Exact preview of what the weekly email will look like
   - Uses the same Tera template as the actual digest
   - Perfect for testing before sending emails

### Technology Stack

- **Axum** - Web framework
- **Tera** - Template engine (reuses email templates)
- **SQLx** - Direct PostgreSQL queries
- **Tower-HTTP** - Middleware (tracing, static files)
- **pepper-crm** - Shared business logic

### File Structure

```
pepper-web/
├── Cargo.toml
├── README.md
├── src/
│   └── main.rs           # Web server with 3 routes
└── templates/
    ├── dashboard.html    # Main overview page
    ├── contacts.html     # Contacts list page
    └── digest.html       # Email preview (copied from templates/)
```

## 🚀 How to Use

### 1. Build (outside Cursor to avoid sandbox)

```bash
cargo build --workspace
```

### 2. Set up database (if not already done)

```bash
createdb pepper_crm
psql pepper_crm < migrations/001_initial.sql
```

### 3. Sync contacts to database

```bash
# Run pepper in dry-run to sync contacts
./target/debug/pepper --dry-run
```

This will:
- Parse all VCF files from `contacts/`
- Sync them to PostgreSQL
- Show you what would be sent (but not send)

### 4. Start web server

```bash
cargo run --bin pepper-web
```

### 5. Open browser

Visit:
- http://localhost:3000 - Dashboard
- http://localhost:3000/contacts - All contacts
- http://localhost:3000/preview - Digest preview

## 🎯 Development Workflow

```
1. Edit VCF files in contacts/
2. Run: pepper --dry-run (syncs to DB)
3. Refresh browser to see changes
4. Check /preview to see what email will contain
5. Run: pepper (sends actual email)
```

## 🎨 Styling

All pages use consistent styling:
- Pepper red (#d32f2f) as primary color
- 🌶️ emoji branding
- Clean cards with shadows
- Responsive grid layouts
- Hover effects
- Same aesthetic as email digest

## 📝 Next Enhancements (Optional)

Future ideas:
- Add search/filter on contacts page
- Show individual contact detail pages
- Display CRM log history
- Add forms to mark tasks complete
- Real-time updates with WebSockets
- Dark mode toggle

## ✨ Benefits

- **Testing**: See what digest will contain before sending
- **Visualization**: Better understand your CRM data
- **Debugging**: Quickly spot issues with contacts or tasks
- **Development**: Fast iteration without sending test emails
- **Monitoring**: Check system status at a glance
