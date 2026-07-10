CREATE TABLE IF NOT EXISTS quick_link_groups (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL UNIQUE,
    icon        TEXT,
    color       TEXT,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

ALTER TABLE quick_links ADD COLUMN group_id INTEGER REFERENCES quick_link_groups(id) ON DELETE SET NULL;
