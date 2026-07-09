-- 005_dismissed_units.sql
-- Tracks discovered systemd/docker/podman units the admin chose to dismiss.

CREATE TABLE IF NOT EXISTS dismissed_units (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    source      TEXT NOT NULL,
    unit_name   TEXT NOT NULL,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(source, unit_name)
);
