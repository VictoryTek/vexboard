-- 003_user_roles.sql
-- Add role column to users. Existing rows default to 'admin'.
-- Applied idempotently via pragma_table_info check in db/mod.rs.
ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'admin';
