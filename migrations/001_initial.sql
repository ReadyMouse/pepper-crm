-- migrations/001_initial.sql

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE contacts (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    vcard_uid       TEXT UNIQUE NOT NULL,
    full_name       TEXT NOT NULL,
    email           TEXT,
    last_synced_at  TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE tasks (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    contact_id      UUID REFERENCES contacts(id) ON DELETE CASCADE,
    body            TEXT NOT NULL,
    status          TEXT DEFAULT 'pending' CHECK (status IN ('pending','done','snoozed')),
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE reconnects (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    contact_id      UUID REFERENCES contacts(id) ON DELETE CASCADE,
    due_date        DATE NOT NULL,
    status          TEXT DEFAULT 'pending' CHECK (status IN ('pending','sent','dismissed','deferred')),
    original_tag    TEXT,
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    sent_at         TIMESTAMPTZ
);

CREATE TABLE digest_log (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    sent_at         TIMESTAMPTZ DEFAULT NOW(),
    recipient       TEXT,
    contact_count   INTEGER,
    task_count      INTEGER,
    reconnect_count INTEGER
);

-- Create indexes for common queries
CREATE INDEX idx_tasks_contact_status ON tasks(contact_id, status);
CREATE INDEX idx_reconnects_contact_status ON reconnects(contact_id, status);
CREATE INDEX idx_reconnects_due_date ON reconnects(due_date);
