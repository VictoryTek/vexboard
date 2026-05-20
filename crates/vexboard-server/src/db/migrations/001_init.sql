-- 001_init.sql
-- VexBoard initial schema

CREATE TABLE IF NOT EXISTS groups (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL UNIQUE,
    icon        TEXT,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS services (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    systemd_unit    TEXT,
    display_name    TEXT NOT NULL,
    description     TEXT,
    url             TEXT,
    icon            TEXT,
    group_id        INTEGER REFERENCES groups(id) ON DELETE SET NULL,
    sort_order      INTEGER NOT NULL DEFAULT 0,
    probe_enabled   BOOLEAN NOT NULL DEFAULT 1,
    probe_interval  INTEGER NOT NULL DEFAULT 30,
    tags            TEXT,
    visible         BOOLEAN NOT NULL DEFAULT 1,
    created_at      DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at      DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS probe_results (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    service_id  INTEGER NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    status      TEXT NOT NULL CHECK(status IN ('up', 'down', 'unknown')),
    latency_ms  INTEGER,
    checked_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS users (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    username        TEXT NOT NULL UNIQUE,
    password_hash   TEXT NOT NULL,
    created_at      DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS settings (
    key     TEXT PRIMARY KEY,
    value   TEXT NOT NULL
);
