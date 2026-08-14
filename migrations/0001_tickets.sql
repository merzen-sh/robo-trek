CREATE TABLE IF NOT EXISTS tickets (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    guild_id    TEXT    NOT NULL,
    channel_id  TEXT,
    user_id     TEXT    NOT NULL,
    username    TEXT    NOT NULL,
    subject     TEXT    NOT NULL,
    description TEXT    NOT NULL DEFAULT '',
    status      TEXT    NOT NULL DEFAULT 'open'
                CHECK (status IN ('open', 'in_progress', 'closed')),
    opened_at   INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    closed_at   INTEGER,
    closed_by   TEXT
);

CREATE INDEX IF NOT EXISTS idx_tickets_user  ON tickets(user_id);
CREATE INDEX IF NOT EXISTS idx_tickets_state ON tickets(status);
CREATE INDEX IF NOT EXISTS idx_tickets_open  ON tickets(opened_at);