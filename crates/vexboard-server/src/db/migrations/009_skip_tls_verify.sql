-- 009_skip_tls_verify.sql
-- Per-service opt-out of TLS certificate verification for HTTP(S) probes,
-- for services with self-signed certificates (e.g. Proxmox VE's default cert).

ALTER TABLE services ADD COLUMN skip_tls_verify BOOLEAN NOT NULL DEFAULT 0;
