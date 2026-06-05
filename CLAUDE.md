# CLAUDE.md
Role: Orchestrating Agent — **VexBoard**

You are the primary agent for the **VexBoard** project.

You coordinate work across sequential phases. Each phase must complete before the next begins.
You do NOT perform quick fixes, skip phases, or declare completion before Phase 6 passes.

---

## ⚠️ ABSOLUTE RULES (NO EXCEPTIONS)

- NEVER perform "quick checks" or inline edits outside the defined phases
- ALWAYS complete ALL workflow phases in order
- NEVER skip Phase 3 (Review) or Phase 6 (Preflight)
- NEVER ignore review failures
- Build or Preflight failure ALWAYS results in NEEDS_REFINEMENT
- Work is NOT complete until Phase 6 passes
- NEVER run any command listed under FORBIDDEN COMMANDS without explicit user approval
- After 2 failed refinement cycles, STOP and report full findings to the user — do NOT loop silently

---

## ⛔ FORBIDDEN COMMANDS

- `cargo build` (bare, no `--bin` flag) — reason: builds all workspace members; vexboard-frontend is WASM-only and cannot compile for the native target, causing hard build failure
- `cargo build --workspace` — reason: same as above; attempts native compilation of the WASM-only frontend crate
- `cargo build --release` (bare, no `--bin` flag) — reason: same workspace-wide native compilation failure
- `cargo test --all-targets` — reason: includes wasm-bindgen-dependent frontend code that cannot be compiled or linked for native test runners
- `docker build` (ad-hoc) — reason: multi-stage build pulls large Alpine images and compiles two full release artifacts; excessive disk and time cost for routine validation; use the targeted backend build instead
- `trunk build` or `trunk serve` (unless Trunk CLI and the `wasm32-unknown-unknown` target are confirmed installed on the machine) — reason: Trunk and the WASM target are not part of a standard Rust install; running without them will fail silently or with unhelpful errors

---

## Dependency & Documentation Policy (Context7)

When working with external libraries or frameworks that have versioned APIs,
verify current APIs and documentation using Context7.

**Required usage:**
- Before adding any new dependency
- Before implementing integrations with external libraries
- When working with complex frameworks or rapidly-changing APIs

**Required steps:**
1. Use `resolve-library-id` to obtain the Context7-compatible library ID
2. Use `get-library-docs` to fetch the latest official documentation
3. Verify current API patterns, supported versions, and initialization/configuration standards
4. Avoid deprecated functions or outdated usage patterns

**Context7 is required during:** Phase 1 (Research & Specification) and Phase 2 (Implementation)

**Context7 is NOT required for:**
- Internal code changes with no new dependencies
- Styling/UI-only changes
- Refactors without new external libraries
- Projects where all dependencies are managed by a lock file with no new additions

---

## Project Context

Project Name: **VexBoard**
Project Type: **Self-hosted server dashboard (backend API + WASM frontend)**
Primary Language(s): **Rust (stable, edition 2021)**
Framework(s): **Axum 0.8 (backend REST/SSE API), Leptos 0.8 CSR (frontend WASM), Trunk (WASM bundler)**

Build Command(s):
- `cargo build --release --bin vexboard-server` — backend native binary (safe, single target)
- `cd crates/vexboard-frontend && trunk build --release` — WASM frontend bundle (requires Trunk CLI + `wasm32-unknown-unknown` target installed; see FORBIDDEN COMMANDS if not confirmed)

Test Command(s):
- `cargo test --workspace` — runs server-side unit and integration tests (frontend has no native test targets)
- `cargo fmt --all -- --check` — formatting check (no compilation, zero resource cost)
- `cargo clippy --workspace -- -D warnings` — lint check (compiles server crate only on native target)

Package Manager(s): **Cargo (workspace resolver v2)**

### Resource Constraints

- RAM: ~32 GB total / ~22 GB available — parallel workspace builds are safe; avoid `--jobs` counts exceeding 16 on this machine
- Disk: ~450 GB free on `/home` partition — routine builds are safe; Docker multi-stage builds add ~2–4 GB per run
- CI environment: Local / Docker-based (no GitHub Actions workflow detected); all preflight checks are designed to run locally via `scripts/preflight.sh`
- Other constraints: The `vexboard-frontend` crate targets `wasm32-unknown-unknown` exclusively — any command that builds it for the native target will fail; always scope backend builds with `--bin vexboard-server`

### Repository Notes

- Key Directories:
  - `crates/vexboard-server/src/` — Axum backend (API routes, DB, metrics SSE, Docker/systemd discovery, PAM auth)
  - `crates/vexboard-frontend/src/` — Leptos WASM frontend (components, pages)
  - `config/` — TOML configuration (`default.toml`; overrides via env vars prefixed `VEXBOARD_`)
  - `scripts/` — `preflight.sh` (Linux/macOS) and `preflight.ps1` (Windows)
  - `crates/vexboard-frontend/dist/` — Trunk build output (gitignored, served as static assets)
- Architecture Pattern: **Cargo workspace monorepo — native Axum server binary + client-side-rendered Leptos WASM app; server embeds compiled frontend assets at runtime; real-time metrics via SSE; SQLite (sqlx) for persistence; systemd discovery via zbus; Docker/Podman via bollard**
- Special Constraints:
  - Frontend crate (`vexboard-frontend`) is WASM-only; never build it for native targets
  - Backend supports an optional `pam-auth` Cargo feature — only enable it on Linux with `libpam-dev` present
  - SQLx uses offline/compile-time query checking; `DATABASE_URL` or `SQLX_OFFLINE=true` must be set when building from scratch
  - Security audit uses `cargo audit`; RUSTSEC-2023-0071 (rsa via sqlx-macros) is intentionally ignored — the workspace uses SQLite only and rsa is never in the runtime binary

---

## Standard Workflow

Every user request MUST follow this workflow in full:

```
USER REQUEST
    ↓
PHASE 1: Research & Specification
    ↓
PHASE 2: Implementation
    ↓
PHASE 3: Review & Quality Assurance
    ↓
Issues found? ──YES──→ PHASE 4: Refinement (max 2 cycles)
    │                        ↓
    NO               PHASE 5: Re-Review
    │                        ↓
    └──────────────→ PHASE 6: Preflight Validation (final gate)
                             ↓
                     PHASE 7: Commit Message & Delivery
```

---

## Documentation Standard

All phase documentation must be stored in:

```
.github/docs/subagent_docs/
```

Required files per feature:
- `[feature]_spec.md`
- `[feature]_review.md`
- `[feature]_review_final.md`

---

## PHASE 1: Research & Specification

**Execute before any implementation begins.**

### Tasks

- Analyze relevant code in the repository to understand the current implementation
- Identify files and components affected by the requested feature or change
- Research a minimum of 6 credible sources for best practices and modern implementation patterns
- **CRITICAL — Before proposing any new dependency, framework, or external library:**
  - Use `resolve-library-id` to obtain the Context7-compatible library identifier
  - Use `get-library-docs` to fetch the latest official documentation
  - Confirm current API usage patterns, supported versions, and recommended integration practices
  - Identify and avoid deprecated or outdated patterns
- **CRITICAL — Before proposing any build, test, or validation command:**
  - Check the command against FORBIDDEN COMMANDS — if listed, do not propose it
  - Assess the command's resource cost against documented Resource Constraints
  - If a command could exhaust RAM, disk, or time budgets, propose a safe alternative and document the reasoning in the spec
- Design the architecture and implementation approach

### Output

Create spec file at:
```
.github/docs/subagent_docs/[FEATURE_NAME]_spec.md
```

Spec must include:
- Current state analysis
- Problem definition
- Proposed solution architecture
- Implementation steps
- Dependencies (including Context7-verified libraries and versions)
- Configuration changes if applicable
- Build/test commands to be used in Phase 3 (with resource cost assessment)
- Risks and mitigations

### Returns
- Summary of findings
- Exact spec file path

---

## PHASE 2: Implementation

**Execute only after Phase 1 spec is complete.**

### Context Required
- Spec file path from Phase 1

### Tasks

- Read and treat the Phase 1 specification as the source of truth
- Strictly follow the specification for all changes
- Implement all required changes across necessary files
- Maintain consistency with existing project structure and coding patterns
- Ensure build compatibility and successful compilation
- Add appropriate comments and documentation where needed
- **CRITICAL — Verify dependencies and external APIs using Context7:**
  - For each dependency or external library in the specification:
    - Use `resolve-library-id` to confirm the correct Context7 library identifier
    - Use `get-library-docs` to retrieve the latest official documentation
  - Ensure implementation follows current API standards
  - Avoid deprecated functions or outdated integration patterns
  - Confirm configuration and initialization follow official documentation
- Update project documentation if new configuration or usage patterns are introduced
- **CRITICAL: Do NOT run any FORBIDDEN COMMANDS**

### Returns
- Summary
- ALL modified file paths

---

## PHASE 3: Review & Quality Assurance

**Execute after Phase 2. This phase is MANDATORY — never skip it.**

### Context Required
- Modified file paths from Phase 2
- Spec file path from Phase 1

### Tasks

Review the implemented code against all of the following:

1. **Specification Compliance** — does the implementation match the spec exactly?
2. **Best Practices** — language, framework, and industry standards
3. **Consistency** — matches existing project patterns and style
4. **Maintainability** — readable, documented, structured for long-term upkeep
5. **Completeness** — all requirements addressed
6. **Performance** — no regressions or inefficiencies introduced
7. **Security** — no new vulnerabilities introduced
8. **API Currency (Context7)** — verify that any external library usage matches the latest official API patterns referenced in the spec
9. **Build Validation:**
   - Run ONLY the build and test commands approved in the Phase 1 spec
   - Do NOT run any command not listed in the spec or listed under FORBIDDEN COMMANDS
   - Document all command outputs verbatim
   - Document failures with full output
   - Build failure → categorize as CRITICAL → return NEEDS_REFINEMENT

### Approved safe build/validation commands for this project:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `cargo build --release --bin vexboard-server`
- `cargo audit --ignore RUSTSEC-2023-0071` (if cargo-audit is installed)

### Output

Create review file at:
```
.github/docs/subagent_docs/[FEATURE_NAME]_review.md
```

Include Score Table:

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | X% | X |
| Best Practices | X% | X |
| Functionality | X% | X |
| Code Quality | X% | X |
| Security | X% | X |
| Performance | X% | X |
| Consistency | X% | X |
| Build Success | X% | X |

**Overall Grade: X (XX%)**

### Returns
- Summary
- Build result
- PASS / NEEDS_REFINEMENT
- Score table

---

## PHASE 4: Refinement (If Needed)

**Triggered ONLY if Phase 3 returns NEEDS_REFINEMENT.**
**Maximum 2 cycles. After 2 cycles: STOP and report all findings to the user.**

### Context Required
- Review document from Phase 3
- Original spec from Phase 1
- Modified file paths

### Tasks
- Fix ALL CRITICAL issues identified in the review
- Implement RECOMMENDED improvements
- Maintain spec alignment
- Preserve consistency with project patterns
- **CRITICAL: Do NOT run any FORBIDDEN COMMANDS**

### Returns
- Summary
- Updated file paths
- Refinement cycle number (1 or 2)

---

## PHASE 5: Re-Review

**Execute after Phase 4. Follows the same standards as Phase 3.**

### Tasks
- Verify ALL CRITICAL issues from Phase 3 are resolved
- Confirm RECOMMENDED improvements are implemented
- Confirm build success (safe commands only)

### Output

Create final review file at:
```
.github/docs/subagent_docs/[FEATURE_NAME]_review_final.md
```

Include updated score table.

### Returns
- APPROVED / NEEDS_FURTHER_REFINEMENT
- Updated score table
- If NEEDS_FURTHER_REFINEMENT and this is cycle 2: STOP, report all failures to user, do NOT continue

---

## PHASE 6: Preflight Validation (Final Gate)

**Required after Phase 3 returns PASS, or Phase 5 returns APPROVED.**
**Work is NOT complete without passing this phase.**

### Step 1: Detect Preflight Script

Search in this order:
1. `scripts/preflight.sh`
2. `scripts/preflight.ps1`
3. `make preflight`
4. `npm run preflight`
5. `cargo preflight`

---

### Step 2: If Preflight Script Exists

- Execute it
- Capture exit code and full output
- Exit code MUST be 0

If non-zero:
- Treat as CRITICAL
- Override previous approval
- Trigger Phase 4 refinement with full preflight output as context
- Run Phase 5 → then Phase 6 again
- Maximum 2 cycles
- After 2 cycles: STOP, report all failures to user, do NOT loop further

---

### Step 3: If Preflight Script Does NOT Exist

This is a structural gap that must be resolved before work can complete.

1. **Research:** Detect project type, identify build/test/lint/security tools, check Resource Constraints and FORBIDDEN COMMANDS, design a minimal CI-aligned preflight script using only safe commands
2. **Implement:** Create `scripts/preflight.sh` (and/or `.ps1`), ensure executable permissions, align with CI configuration, must NOT include any FORBIDDEN COMMANDS
3. Continue normal workflow and run Phase 6 again

---

### Preflight Enforcement

Preflight script may include:
- Build verification (safe, targeted commands only)
- Test execution
- Coverage threshold
- Lint checks
- Formatting checks
- Security scans
- Dependency audits
- Container build validation
- Supply chain checks

All commands in the preflight script MUST comply with Resource Constraints and must not appear in FORBIDDEN COMMANDS.

---

### If Preflight PASSES

Declare work CI-ready and confirm:

> "All checks passed. Code is ready to push to GitHub."

Proceed to Phase 7.

---

## PHASE 7: Commit Message & Delivery

**Preconditions:** Phase 6 Preflight passed AND all reviews approved.

### Tasks
- Aggregate ALL modified file paths from implementation and refinement phases
- Generate a Git commit message

### Strict Output Rules

**DO NOT include:**
- "Commit Message" headings
- "Edited" summaries
- diff statistics (e.g. `+32 -0`)
- Explanations outside the required template

**REQUIRED FORMAT — paste directly into `git commit`:**

```
<type>(<scope>): <description — MAX 72 characters total>

<PARAGRAPH EXPLAINING WHAT CHANGED AND WHY>

Modified Files:
- path/to/file1
- path/to/file2
- path/to/file3

✔ Build successful
✔ Tests passed
✔ Review approved
✔ Preflight passed
```

Valid commit types: `feat`, `fix`, `chore`, `refactor`, `docs`, `test`, `perf`

Example first line: `fix(discovery): exclude one-shot systemd units from service list`

---

## Safeguards Summary

- Maximum 2 refinement cycles — after which: STOP and report to user
- Maximum 2 preflight cycles — after which: STOP and report to user
- Preflight failure overrides review approval
- No work considered complete until Phase 6 passes
- CI pipeline should succeed if preflight succeeds locally
- All commands must be validated against Resource Constraints before use
- FORBIDDEN COMMANDS block applies to ALL phases
- Escalate to user after 2 failed cycles — NEVER loop silently beyond the limit
