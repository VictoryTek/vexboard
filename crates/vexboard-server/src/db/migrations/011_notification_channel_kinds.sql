-- 011_notification_channel_kinds.sql
-- Widen notification_channels.kind to allow telegram and gotify, alongside
-- the existing webhook/discord/ntfy. SQLite can't alter a CHECK constraint
-- in place, so recreate the table with the wider constraint and copy rows
-- across — same pattern already used in 008_unify_groups.sql.

CREATE TABLE notification_channels_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK(kind IN ('webhook', 'discord', 'ntfy', 'telegram', 'gotify')),
    target      TEXT NOT NULL,
    secret      TEXT,
    events      TEXT NOT NULL DEFAULT '[]',
    enabled     BOOLEAN NOT NULL DEFAULT 1,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO notification_channels_new (id, name, kind, target, secret, events, enabled, created_at)
SELECT id, name, kind, target, secret, events, enabled, created_at FROM notification_channels;

DROP TABLE notification_channels;
ALTER TABLE notification_channels_new RENAME TO notification_channels;
