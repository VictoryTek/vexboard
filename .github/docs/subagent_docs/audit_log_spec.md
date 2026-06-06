# Audit Log — Feature Specification
**Phase:** 1 — Research & Specification
**Date:** 2026-06-05
**Feature:** Persistent audit log for sensitive operations

---

## 1. Current State Analysis

VexBoard has no record of who performed which state-mutating operations. All CRUD handlers for services, groups, and quick links, as well as authentication events, execute without leaving any trail beyond transient `tracing` log lines. Log lines are ephemeral — they are not persisted to a queryable store and are lost on process restart or log rotation.

The session system identifies users (`session.get::<String>("username")`), and the `require_auth` middleware guarantees that every protected handler is called by an authenticated user. This provides the necessary actor identity for audit records at the handler level.

Current state-mutating operations without audit trails:
- `POST /api/v1/services` — create service
- `PUT /api/v1/services/{id}` — update service
- `DELETE /api/v1/services/{id}` — delete service
- `POST /api/v1/services/{id}/claim` — claim discovered unit
- `POST /api/v1/groups` — create group
- `PUT /api/v1/groups/{id}` — update group
- `DELETE /api/v1/groups/{id}` — delete group
- `POST /api/v1/quick-links` — create quick link
- `PUT /api/v1/quick-links/{id}` — update quick link
- `DELETE /api/v1/quick-links/{id}` — delete quick link
- `POST /api/v1/auth/login` — login (success/failure)
- `POST /api/v1/auth/logout` — logout
- `PATCH /api/v1/auth/me` — credential change
- `POST /api/v1/setup` — admin account creation
- `POST /api/v1/discovery/refresh` — manual discovery refresh

---

## 2. Problem Definition

Without an audit log:
- There is no accountability for destructive actions (service/group deletion)
- Credential changes leave no trail (who changed what, when)
- Failed login attempts are not persistently tracked for security review
- Discovery refresh triggers are invisible in hindsight
- Shared deployments cannot attribute changes to specific users

---

## 3. Proposed Solution Architecture

### 3.1 Database Schema

New migration file `002_audit_log.sql`:

```sql
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
```

Field rationale:
- `actor` — username of the authenticated user; `"setup"` for initial admin creation (unauthenticated)
- `action` — dot-namespaced string: `"service.create"`, `"auth.login_success"`, etc.
- `resource_type` — entity type: `"service"`, `"group"`, `"quick_link"`, `"user"` (null for auth events)
- `resource_id` — DB row id of the affected entity (null for auth events and discovery refresh)
- `detail` — optional JSON string with context (e.g. `{"display_name":"My App"}` for creates)
- `ip_addr` — originating IP (captured for auth events; null for CRUD handlers to avoid expanding every handler signature)
- `created_at` — UTC timestamp, auto-set by SQLite

### 3.2 Action Name Enum

| Action string | Trigger |
|---|---|
| `auth.login_success` | Successful login |
| `auth.login_failure` | Failed login attempt |
| `auth.logout` | Logout |
| `auth.credential_change` | `PATCH /auth/me` success |
| `service.create` | POST services |
| `service.update` | PUT services/{id} |
| `service.delete` | DELETE services/{id} |
| `service.claim` | POST services/{id}/claim |
| `group.create` | POST groups |
| `group.update` | PUT groups/{id} |
| `group.delete` | DELETE groups/{id} |
| `quick_link.create` | POST quick-links |
| `quick_link.update` | PUT quick-links/{id} |
| `quick_link.delete` | DELETE quick-links/{id} |
| `setup.admin_created` | POST /setup (first-run admin) |
| `discovery.refresh` | POST discovery/refresh |

### 3.3 New Module: `db::audit`

`crates/vexboard-server/src/db/audit.rs` — single fire-and-forget insert function:

```rust
pub async fn insert(
    pool: &SqlitePool,
    actor: &str,
    action: &str,
    resource_type: Option<&str>,
    resource_id: Option<i64>,
    detail: Option<&str>,
    ip_addr: Option<&str>,
)
```

On DB error: log with `tracing::error!`, do not propagate — audit failures must never fail the operation.

### 3.4 Handler Modifications

**`api/auth.rs`** — already has `session: Session`. Add audit calls:
- After successful login: `auth.login_success` with `ip_addr`
- After failed login (bad credentials): `auth.login_failure` with `ip_addr` and `detail: {"username": ...}`
- After logout: `auth.logout`
- After successful credential change: `auth.credential_change`

**`api/services.rs`** — add `session: Session` extractor to `create_service`, `update_service`, `delete_service`, `claim_service`. Extract actor with:
```rust
let actor = session.get::<String>("username").await.ok().flatten().unwrap_or_else(|| "unknown".to_string());
```
- `service.create` — `detail: {"display_name": payload.display_name}`
- `service.update` — `detail: {"id": id}`
- `service.delete` — `detail: {"id": id}`
- `service.claim` — `detail: {"systemd_unit": ...}`

**`api/groups.rs`** — same pattern as services.

**`api/quick_links.rs`** — same pattern.

**`api/setup.rs`** — actor is `payload.username` (not yet authenticated); no session. Audit after successful `INSERT INTO users`.

**`discovery/mod.rs`** — add `session: Session` to `trigger_refresh`; audit `discovery.refresh`.

### 3.5 New Module: `api::audit`

`crates/vexboard-server/src/api/audit.rs` — read-only admin endpoint:

```
GET /api/v1/audit?limit=50&offset=0
```

Protected by `require_auth` middleware (added in `api/mod.rs`). Returns paginated audit entries ordered by `created_at DESC`.

Response shape:
```json
{
  "entries": [...],
  "total": 1234,
  "limit": 50,
  "offset": 0
}
```

### 3.6 Model

`db/models.rs` — add `AuditEvent`:
```rust
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AuditEvent {
    pub id: i64,
    pub actor: String,
    pub action: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<i64>,
    pub detail: Option<String>,
    pub ip_addr: Option<String>,
    pub created_at: Option<NaiveDateTime>,
}
```

---

## 4. Implementation Steps

1. Create `crates/vexboard-server/src/db/migrations/002_audit_log.sql`
2. Update `crates/vexboard-server/src/db/mod.rs` — run migration 002, add `pub mod audit`
3. Create `crates/vexboard-server/src/db/audit.rs` — `insert` helper
4. Update `crates/vexboard-server/src/db/models.rs` — add `AuditEvent`
5. Create `crates/vexboard-server/src/api/audit.rs` — paginated GET endpoint
6. Update `crates/vexboard-server/src/api/mod.rs` — add `pub mod audit`, nest audit route
7. Update `crates/vexboard-server/src/api/auth.rs` — 4 audit call sites
8. Update `crates/vexboard-server/src/api/services.rs` — 4 audit call sites + `session` extractor
9. Update `crates/vexboard-server/src/api/groups.rs` — 3 audit call sites + `session` extractor
10. Update `crates/vexboard-server/src/api/quick_links.rs` — 3 audit call sites + `session` extractor
11. Update `crates/vexboard-server/src/api/setup.rs` — 1 audit call site
12. Update `crates/vexboard-server/src/discovery/mod.rs` — 1 audit call site + `session` extractor

---

## 5. Dependencies

No new dependencies required. All components already in `Cargo.toml`:
- `sqlx` — audit table insert
- `serde_json` — detail field JSON serialization
- `serde` — `AuditEvent` serialization
- `tracing` — error logging on audit write failure
- `tower-sessions` — session actor extraction

---

## 6. Configuration

No new configuration needed. The audit log is always enabled; it uses the existing SQLite database.

---

## 7. Build/Test Commands (Phase 3)

Approved commands:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `cargo build --release --bin vexboard-server`

Resource cost: All are safe, targeted, low-cost commands. None build the workspace for native targets.

---

## 8. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Audit write failure blocks user operation | `insert` is fire-and-forget; errors only log, never propagate |
| Audit table grows unbounded | Acceptable for self-hosted deployments; pruning can be a future addition |
| Adding `session: Session` to handlers changes function signatures | Axum handles this gracefully; `Session` is a standard extractor. No behavioral change |
| `detail` field stores user-supplied data | All values go through `serde_json` serialization from typed structs, not raw user strings |
| Login failure logs the attempted username | This is intentional — security audit trail. Username field is already stored in the users table |
| `setup.create_admin` has no session | Actor is set to the created username itself — unambiguous and correct for bootstrap events |
