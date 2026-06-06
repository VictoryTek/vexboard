-- 002_audit_log.sql
-- Persistent audit trail for all state-mutating operations.

CREATE TABLE IF NOT EXISTS audit_log (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    actor         TEXT NOT NULL,
    action        TEXT NOT NULL,
    resource_type TEXT,
    resource_id   INTEGER,
    detail        TEXT,
    ip_addr       TEXT,
    created_at    DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_audit_log_created_at ON audit_log(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_log_actor      ON audit_log(actor);
