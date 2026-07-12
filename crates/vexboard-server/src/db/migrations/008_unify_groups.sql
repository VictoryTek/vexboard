-- 008_unify_groups.sql
-- Create the rebuilt quick_links table with group_id referencing the unified
-- groups table instead of the now-removed quick_link_groups table. The actual
-- data copy (with group_id remapped from old quick_link_groups ids to new
-- groups ids) is done in Rust immediately after this runs, since it depends on
-- a per-database id mapping computed at migration time (see
-- unify_quick_link_groups in db/mod.rs).

CREATE TABLE quick_links_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    title       TEXT NOT NULL,
    url         TEXT NOT NULL,
    icon        TEXT,
    description TEXT,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    group_id    INTEGER REFERENCES groups(id) ON DELETE SET NULL,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);
