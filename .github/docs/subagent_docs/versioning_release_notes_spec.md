# Versioning & Release Notes — Spec

## Current state analysis

- `crates/vexboard-server/Cargo.toml` and `crates/vexboard-frontend/Cargo.toml` both
  hardcode `version = "0.1.0"` independently — neither has ever been bumped.
- No `[workspace.package]` section in the root `Cargo.toml` (only `[workspace]` and
  `[workspace.dependencies]`), so there's no single source of truth for "the"
  project version.
- `git tag -l` returns nothing — no release has ever been tagged.
- No `CHANGELOG.md`, no `release-notes/` (or similar) directory anywhere in the repo.
- No release step in `.github/workflows/ci.yml` (it only builds/tests/lints).
- The running server already prints `env!("CARGO_PKG_VERSION")` at startup
  (`crates/vexboard-server/src/main.rs:143` area: `"Starting VexBoard v{}"`), so
  whatever version ends up in `Cargo.toml` is already user-visible — bumping it is
  not cosmetic-only.

**Conclusion: no versioning scheme is established.** This is a from-scratch setup,
not a bump of an existing one.

## Proposed solution

### 1. Versioning scheme: Semantic Versioning (SemVer), single shared version

- Adopt `MAJOR.MINOR.PATCH` per semver.org:
  - `PATCH` — bug fixes, no behavior/API change (this cache-control/404 fix qualifies).
  - `MINOR` — backwards-compatible features.
  - `MAJOR` — breaking changes (config format, API contract, DB schema needing manual migration).
- Single version shared by both crates via Cargo's built-in workspace inheritance:
  add `[workspace.package] version = "0.1.1"` to the root `Cargo.toml`, and change
  each crate's `[package]` to `version.workspace = true`. This makes future bumps a
  one-line change in one file instead of two, and guarantees server/frontend never
  drift apart.
- Starting point: since no release has ever been cut, `0.1.0` is treated as the
  unreleased baseline and this fix becomes the first tracked release, `0.1.1`.

### 2. Release notes directory

- Create `release-notes/` at the repo root (sibling to `README.md`, not buried under
  `.github/docs/subagent_docs/` which is for internal phase docs, not user-facing
  release history).
- `release-notes/README.md` documents the policy: SemVer, one Markdown file per
  release named `vX.Y.Z.md`, what belongs in each entry (Added/Fixed/Changed
  sections), and where the version is bumped (`[workspace.package].version` in root
  `Cargo.toml`).
- `release-notes/v0.1.1.md` documents this release: the static-asset cache-control
  and 404 fix from the prior task.

### 3. Version bump

- Bump `[workspace.package].version` from `0.1.0` to `0.1.1` (patch — bug fix only,
  no API/config changes).

## Implementation steps

1. Root `Cargo.toml`: add `[workspace.package]` with `version = "0.1.1"`.
2. `crates/vexboard-server/Cargo.toml`: replace `version = "0.1.0"` with `version.workspace = true`.
3. `crates/vexboard-frontend/Cargo.toml`: same change.
4. Create `release-notes/README.md` (policy doc).
5. Create `release-notes/v0.1.1.md` (this release's notes).

## Dependencies

None. Pure Cargo workspace metadata — no external crate or library involved, so
Context7 verification doesn't apply.

## Configuration changes

None functionally — `Cargo.lock` will pick up the new version strings on next build
(package versions in the lockfile for workspace members update automatically).

## Risks and mitigations

- **Risk:** `version.workspace = true` requires the crate to *not* set its own
  `version` key — a leftover duplicate key would be a hard Cargo error.
  **Mitigation:** verify with `cargo build --bin vexboard-server` (approved safe
  command) after the edit; Cargo fails loudly and immediately on malformed
  manifests.
- **Risk:** `vexboard-frontend` can't be natively built/tested per project
  constraints, so its Cargo.toml correctness can only be checked by Cargo's manifest
  parsing, not a full compile.
  **Mitigation:** `cargo metadata` or `cargo build --bin vexboard-server` (which
  still parses the whole workspace manifest graph even though it only compiles the
  server binary) is sufficient to catch a malformed `vexboard-frontend/Cargo.toml`.
