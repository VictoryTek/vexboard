# Dashboard Blink + Admin Role — Final Review

## End-to-End Verification (browser, not inference)

Trunk was installed and both bundles were built from the same source tree — the
only difference being `Suspense` vs `Transition`. Both were served by a real
`vexboard-server` against the **same seeded database** (24 probe-enabled services,
5s probe interval) and driven with Playwright across several live SSE probe ticks.

| Measurement | BASELINE (`Suspense`) | FIXED (`Transition`) |
|---|---|---|
| Real cards on screen | 24 → **0** | 24 → **24** |
| Skeleton fallback samples | **2** | **0** |
| scrollTop | 837 → **0** | 837 → **837** |
| Max scroll drop | **837 px** | **0 px** |
| Verdict | BLINK ❌ / SCROLL LOST ❌ | NO BLINK ✅ / SCROLL HELD ✅ |

The baseline run reproduces the reported bug exactly: the whole grid is replaced
by the fallback and the user is thrown to the top of the page.

### Data still refreshes in the background (not frozen)

| Check | Result |
|---|---|
| Original card DOM node survives (never remounted) | **true** ✅ |
| Status/latency text still changes on new probes | **true** ✅ |
| Sparklines present throughout | 48 → 48 ✅ |

This confirms the cards update in place — the Homepage / Uptime-Kuma behaviour
requested — rather than the UI merely being frozen.

## Role Regression

Verified by temporarily reverting `resolve_role` to the old session-only
behaviour: `test_role_is_read_from_db_not_stale_session` failed with `403` where
`201` was expected — the user's exact symptom — and passed once restored.

## Preflight

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace -- -D warnings` | PASS |
| `cargo test -p vexboard-server` | PASS (42 passed, 0 failed) |
| `cargo build --release --bin vexboard-server` | PASS |
| `trunk build --release` (frontend WASM) | PASS |
| Exit code | **0** |

**Result: APPROVED**

## Note for the maintainer

`login_pam` grants bootstrap admin via `try_claim_setting`, which returns `false`
once the key exists — *even for the same user*. The bootstrap admin is therefore
demoted to viewer on their **second** login. Only affects builds with the
`pam-auth` feature (not the default, not the Docker image). Left unchanged as
out of scope.
