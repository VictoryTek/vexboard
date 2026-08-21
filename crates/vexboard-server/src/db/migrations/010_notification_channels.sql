-- 010_notification_channels.sql
-- User-managed notification destinations. Replaces the config-file-only
-- webhook list — delivery tuning (retry count/delay, global secret) stays
-- in config.toml, but the set of destinations belongs in the database
-- like every other admin-managed collection in this app.

CREATE TABLE IF NOT EXISTS notification_channels (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK(kind IN ('webhook', 'discord', 'ntfy')),
    target      TEXT NOT NULL,
    secret      TEXT,
    events      TEXT NOT NULL DEFAULT '[]',
    enabled     BOOLEAN NOT NULL DEFAULT 1,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);
