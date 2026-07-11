-- 007_unique_systemd_unit.sql
-- Prevent duplicate claims of the same systemd/docker/podman unit.

-- Resolve any pre-existing duplicates (possible before this constraint existed)
-- by clearing systemd_unit on all but the earliest claim, so the index below
-- can always be created cleanly.
UPDATE services
SET systemd_unit = NULL
WHERE systemd_unit IS NOT NULL
  AND id NOT IN (
      SELECT MIN(id) FROM services WHERE systemd_unit IS NOT NULL GROUP BY systemd_unit
  );

CREATE UNIQUE INDEX IF NOT EXISTS idx_services_systemd_unit_unique
    ON services(systemd_unit) WHERE systemd_unit IS NOT NULL;
