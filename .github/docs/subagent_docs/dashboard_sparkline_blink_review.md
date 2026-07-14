# Dashboard Sparkline Blink Fix — Review

## Change

`crates/vexboard-frontend/src/components/service_card.rs`: added a
`held_history` signal that retains the last resolved probe history, fed by a
guarded `Effect` that only copies `history.get()` when `Some`. The sparkline
now renders from `held_history` instead of directly from the refetching
`LocalResource`, so it never blanks mid-refetch.

## Validation

| Category | Result |
|----------|--------|
| Spec compliance | Matches spec exactly |
| Frontend compile (`cargo check -p vexboard-frontend --target wasm32`) | PASS |
| `cargo fmt --all -- --check` (preflight) | PASS |
| `cargo clippy --workspace -D warnings` (preflight) | PASS |
| `cargo test -p vexboard-server` (preflight) | PASS — 40 passed |
| `cargo build --release --bin vexboard-server` (preflight) | PASS |
| Preflight exit code | 0 |

## Notes

- Root cause: a probe cycle probes every service, so all cards refetch history
  in the same tick; a refetching `LocalResource` yields `None`, collapsing every
  sparkline strip at once, dropping page height and forcing scroll to top. Prior
  fixes (5513366, 94b1078) changed which resources refetch but not the transient
  `None` blanking. This change eliminates the blank window.
- `Effect` reads the resource and writes a distinct signal it never reads — no
  reactive loop.

**Result: PASS**
