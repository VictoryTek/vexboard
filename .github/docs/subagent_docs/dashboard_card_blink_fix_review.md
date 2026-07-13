# Dashboard Card Status Blink (Regression) — Review

## Scope

Reviewed: `crates/vexboard-frontend/src/components/service_card.rs` diff against
spec `.github/docs/subagent_docs/dashboard_card_blink_fix_spec.md`.

## Diff Reviewed

```rust
-    let history = LocalResource::new(move || {
-        let _trigger = live_status.with(|m| m.get(&service_id).cloned());
-        async move {
+    let live_entry = Memo::new(move |_| live_status.with(|m| m.get(&service_id).cloned()));
+    let history = LocalResource::new(move || {
+        live_entry.get();
+        async move {
             if probe_enabled {
                 fetch_history(service_id).await
             } else {
                 Vec::new()
             }
         }
     });
```

## Findings

### Specification Compliance
Verbatim match of the spec's Implementation Step 1. Single file touched, as specced.

### Best Practices / Consistency
Uses `Memo::new(move |_| ...)`, the standard Leptos idiom for value-gated derived
state (confirmed against Context7 `/leptos-rs/leptos` reactive_graph docs — a `Memo`
recomputes on every source notification but only notifies its own subscribers when
the newly computed value is unequal to the cached one). `(String, Option<i64>)`
satisfies `PartialEq + 'static`, so this compiles without new trait bounds (confirmed
by clean `clippy`/`build` below).

### Functionality
This directly targets the root cause: `live_status` is one coarse
`RwSignal<HashMap<i64, (String, Option<i64>)>>`, so any raw `.with()` read inside a
resource's source closure subscribes to the whole signal, not a per-key slice — every
SSE tick for any service previously re-ran every card's `history` resource
concurrently, producing the dashboard-wide blink. Routing the read through a `Memo`
preserves per-card recomputation (cheap, synchronous) but only propagates a change
to the `history` resource when *that* card's own tuple actually changed, so only the
one card whose status/latency actually updated refetches its sparkline. This
eliminates the regression while preserving the live-refresh behavior from `94b1078`.

### Code Quality
Minimal, single-file, two-line change. No dead code, no unrelated formatting touched.

### Security
No new I/O or auth surface. No change in what's fetched, only when/how often.

### Performance
Strictly fewer concurrent HTTP requests per SSE tick than before this fix (1 vs. N,
where N = number of rendered cards). Per-card recompute cost of the `Memo` itself is a
single `HashMap::get` + clone of a small tuple, negligible next to the eliminated
network round-trips.

## Build/Test Command Output (verbatim, safe commands only)

### `cargo fmt --all -- --check`
```
(no output — exit 0)
```

### `cargo clippy --workspace -- -D warnings`
```
    Checking vexboard-frontend v0.2.0 (C:\Projects\vexboard\crates\vexboard-frontend)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.98s
```
Exit code: 0

### `cargo test -p vexboard-server`
```
running 40 tests
test result: ok. 40 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
```
Exit code: 0 (no SIGSEGV observed)

### `cargo build --release --bin vexboard-server`
```
    Finished `release` profile [optimized] target(s) in 7.24s
```
Exit code: 0

Note: `trunk build`/`trunk serve` not run (Trunk CLI / `wasm32-unknown-unknown`
presence unconfirmed on this machine, per FORBIDDEN COMMANDS gate); verification of
runtime behavior rests on code review of the reactivity change plus the passing
`cargo clippy --workspace` compile of the frontend crate (native check only, per
project's documented approved-command semantics).

## Score Table

| Category | Score | Grade |
|----------|-------|-------|
| Specification Compliance | 100% | A |
| Best Practices | 100% | A |
| Functionality | 100% | A |
| Code Quality | 100% | A |
| Security | 100% | A |
| Performance | 100% | A |
| Consistency | 100% | A |
| Build Success | 100% | A |

**Overall Grade: A (100%)**

## Verdict

**PASS**
