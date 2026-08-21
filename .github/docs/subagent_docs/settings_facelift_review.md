# Settings Page Facelift — Review

Scope reviewed: Phase 1 spec Steps 1–3 (primitives + shell + migrate existing
controls, zero backend change). Steps 4–7 (generalised settings API, DB config
layering, new panes for Monitoring/Notifications/Backup, toast error states)
are **not** part of this change — they depend on backend work not yet built
and are explicitly deferred, per spec.

## Files changed

- `crates/vexboard-frontend/src/pages/settings.rs` — deleted (503 lines)
- `crates/vexboard-frontend/src/pages/settings/mod.rs` — new, 158 lines (rail + pane shell)
- `crates/vexboard-frontend/src/pages/settings/ui.rs` — new, 47 lines (`card`/`row`/`row_stack` primitives)
- `crates/vexboard-frontend/src/pages/settings/appearance.rs` — new, 90 lines
- `crates/vexboard-frontend/src/pages/settings/discovery.rs` — new, 19 lines
- `crates/vexboard-frontend/src/pages/settings/security.rs` — new, 106 lines
- `crates/vexboard-frontend/src/pages/settings/users.rs` — new, 200 lines
- `crates/vexboard-frontend/src/pages/settings/about.rs` — new, 22 lines
- `crates/vexboard-frontend/style/main.css` — settings CSS block replaced (net +204/-538 across both changed files)

`pages/mod.rs` and `main.rs` required no changes — `pub mod settings;` resolves
to the new directory module transparently, and the route still points at
`pages::settings::SettingsPage`.

## Deviations from the Phase 1 spec (called out explicitly)

1. **No URL deep-linking** (`/settings/:section`). Section selection is local
   component state (`RwSignal<Section>`), not synced to the router. Simpler,
   and every other piece of app state in this codebase (sort mode, sidebar
   hover) is likewise client-local rather than URL-driven — deep-linking would
   be new precedent, not a fix. Straightforward to add later if wanted.
2. **The persistent "About" banner is gone**, replaced by a dedicated About
   tab containing the same version string. This was shown in the approved
   rendering and matches the reference product's pattern of a dedicated info
   page rather than a banner repeated on every visit.
3. **No fake "System" theme option or search box** were added, even though
   the rendered mockup showed them for illustration. Neither has a real
   implementation behind it today (no `prefers-color-scheme` listener exists,
   and 5 sections don't need search) — adding either would be an unbacked
   control, which the spec's own diagnosis calls out as an existing defect.

## Review checklist

1. **Specification compliance** — Steps 1–3 implemented as written: shared
   primitives extracted, page split into a directory module (mirroring the
   existing `pages/dashboard/` convention), all five original controls
   (theme, sidebar mode, discovery blurb, auth mode, user management) moved
   with unchanged API calls and behavior. ✅
2. **Best practices** — Component/prop patterns match existing Leptos 0.8
   usage elsewhere in the crate (`pub(super) fn X(...) -> impl IntoView`,
   local `use wasm_bindgen::JsCast;` inside closures, `#[cfg(target_arch =
   "wasm32")]` gating for browser-only code) — no new patterns introduced. ✅
3. **Consistency** — New CSS reuses existing design tokens exclusively
   (`--color-bg-surface`, `--color-accent-dim`, etc.); no hardcoded colors.
   The `.settings-nav-option`/`.settings-nav-option-active`/`.settings-nav-dot`
   picker styling was kept as-is and reused (not reinvented) since it already
   matched the target look. ✅
4. **Maintainability** — 503-line single file → 7 files of 19–200 lines,
   one concern per file, matching the `pages/dashboard/` split pattern
   already established in this codebase. Inline `style=` strings and
   `onmouseover` handlers in the old User Management block replaced with
   named CSS classes (`.settings-role-badge`, `.settings-btn-ghost`, etc.). ✅
5. **Completeness** — all 5 previously-reachable settings remain reachable
   with identical behavior; nothing regressed silently. ✅
6. **Performance** — no new network calls; the Security/Users sections'
   fetch effects now only run once their tab is actually shown (previously
   guarded by `is_admin()` inside an always-registered effect) — slightly
   fewer wasted admin-status checks for viewer-role users, not a regression. ✅
7. **Security** — no auth/permission logic touched. Admin-gating preserved
   at both the nav level (`Show when=is_admin`) and the pane level
   (`active == Section::X && is_admin()`), matching the original's
   `<Show when=is_admin>` wrapping. ✅
8. **API currency** — no external dependency or library API touched; no
   Context7 lookup required per the Dependency Policy. ✅
9. **Build validation** — see below, all approved commands pass.

## Build validation (verbatim)

**`cargo fmt --all -- --check`**
Initial run flagged one line-length wrap in `ui.rs::row`; fixed via
`cargo fmt --all`. Re-run: clean, exit 0.

**`cargo clippy --workspace -- -D warnings`**
```
    Checking vexboard-server v0.2.0
    Checking vexboard-frontend v0.2.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 53s
```
Zero warnings across the workspace, including the new settings module. (Note:
CLAUDE.md's Resource Constraints describe `vexboard-frontend` as unable to
compile for the native target — that applies to `cargo build`, which invokes
codegen/linking; `cargo clippy` performs a check-only pass and, as observed
here and confirmed by the project's own `scripts/preflight.ps1`, does
successfully type-check the frontend crate natively. Documenting this since
it wasn't previously verified in this conversation.)

**`cargo test -p vexboard-server`**
```
test result: ok. 48 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```
No backend code was touched by this change; all 48 pre-existing tests pass
unmodified.

**`cargo build --release --bin vexboard-server`**
```
Finished `release` profile [optimized] target(s) in 2m 02s
```

**`cargo audit --ignore RUSTSEC-2023-0071`** (cargo-audit was found installed)
Exit code 0. Only pre-existing "yanked crate" advisories on transitive deps
(`spin`, others) unrelated to this change; RUSTSEC-2023-0071 ignored per
policy.

## WASM/Trunk build — not run

`trunk build` was not attempted: `rustup target list --installed` shows no
`wasm32-unknown-unknown` target and `trunk` is not on PATH in this
environment. Per CLAUDE.md this command is forbidden unless both are
confirmed present, and neither is. **This means the frontend has been
verified to format cleanly, lint cleanly (native check-pass), and match
every existing Leptos/Rust pattern in the codebase — but has not been
compiled to WASM or run in a browser in this session.** The user should run
`trunk build` (or `trunk serve`) in an environment with the WASM toolchain
before considering this fully verified end-to-end.

## Score table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A (Steps 1–3; Steps 4–7 correctly deferred) |
| Best Practices | 100% | A |
| Functionality | 90% | A- (not verified in-browser — see WASM note above) |
| Code Quality | 100% | A |
| Security | 100% | A (no auth logic touched, gating preserved) |
| Performance | 100% | A |
| Consistency | 100% | A |
| Build Success | 95% | A (all native checks pass; WASM build unverified) |

**Overall Grade: A (98%)**

## Result: **PASS**

No CRITICAL issues found. The only open item is the WASM build/browser
verification, which is an environment limitation (no Trunk/wasm32 target
installed) rather than a code defect — flagged above rather than silently
skipped.

## Phase 6 — Preflight

`scripts/preflight.ps1` executed directly (the project's actual CI-equivalent
gate, which runs `cargo fmt`, `cargo clippy --workspace -D warnings`,
`cargo test --workspace`, `cargo build --release --bin vexboard-server`, and
`cargo audit`):

```
[PASS] cargo fmt
[PASS] cargo clippy
[PASS] cargo test          (48 passed; 0 failed)
[PASS] cargo build --release
[PASS] cargo audit         (5 pre-existing advisories on transitive deps,
                             unrelated to this change — 0 new)

All preflight checks passed.
```

Exit code 0. **Phase 6: PASSED on first attempt — no refinement cycles needed.**
