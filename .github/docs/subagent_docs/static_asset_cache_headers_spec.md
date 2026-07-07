# Static Asset Cache-Control Headers — Spec

## Current state analysis

`crates/vexboard-server/src/main.rs:134-143` serves the frontend bundle with:

```rust
let app = app.fallback_service(
    ServeDir::new(&assets_root).fallback(ServeFile::new(format!("{}/index.html", assets_root))),
);
```

No `Cache-Control` headers are set anywhere. `ServeDir::fallback` means **any** request
that doesn't resolve to a file on disk — including a genuinely missing hashed asset —
returns HTTP 200 with `index.html`'s HTML body.

Trunk (`crates/vexboard-frontend/Trunk.toml`, `index.html`) builds:
- Content-hashed, cache-forever-safe outputs: `vexboard-frontend-<hash>.js`,
  `vexboard-frontend-<hash>_bg.wasm`, and the CSS bundle from `data-trunk rel="css"`
  (also hashed by Trunk), all named `<name>-<hash>.<ext>`.
- Non-hashed copied files via `data-trunk rel="copy-file"`: `vexboard-logo.png`,
  `icons-index.json` — filenames stay stable even if content changes.
- `index.html` itself, which references the current build's hashed filenames.

## Problem

Browsers heuristically cache `index.html` (no `Cache-Control` given). After a rebuild,
a client can hold a stale `index.html` pointing at a `.wasm`/`.js` filename that no
longer exists on disk. `ServeDir::fallback` then serves the *current* `index.html`
(200, HTML) for that stale asset path instead of a 404, and the browser's
`WebAssembly.instantiate` fails trying to parse HTML as WASM.

## Proposed solution

Replace the router fallback with a single async service (`spa_asset_service`) that:

1. Serves the request through a plain `ServeDir::new(&assets_root)` (no built-in
   `.fallback()`), so **genuinely missing files 404 by default**.
2. If the result is `404` and the request path has no file extension on its last
   segment, treat it as a client-side route (`/setup`, `/login`, ...) and serve
   `index.html` instead, tagged `Cache-Control: no-cache, must-revalidate`.
   If the path *does* have an extension, it was a real asset request — the 404
   passes through unchanged.
3. If the result is a normal success response, attach `Cache-Control` based on the
   filename:
   - Content-hashed filenames (`<name>-<hash>.<ext>` / `<name>-<hash>_bg.wasm`,
     hash = ≥8 lowercase hex chars after the last `-`) →
     `public, max-age=31536000, immutable`.
   - Everything else (`index.html`, `vexboard-logo.png`, `icons-index.json`, and the
     SPA-fallback response) → `no-cache, must-revalidate`.

Hash detection is done with plain string ops (rsplit on `-`, `is_ascii_hexdigit`) —
no new dependency needed (no `regex` crate in the workspace, and this is easy without
one; Context7 not required since no external dependency/API is introduced).

Implementation lives directly in `main.rs` as a `tower::service_fn` closure that owns
the `assets_root` and constructs `ServeDir`/`ServeFile` per call (both are just path
handles, so this is cheap) — this centralizes the header/fallback decision in one
place rather than sprinkling header-setting logic around, matching the "custom Layer
wrapping ServeDir/ServeFile" guidance without needing a hand-written `tower::Layer`
impl (`service_fn` + `ServiceExt::oneshot` gives the same composition).

## Implementation steps

1. In `crates/vexboard-server/src/main.rs`, add two small helpers:
   - `fn has_extension(path: &str) -> bool`
   - `fn is_hashed_asset(path: &str) -> bool`
2. Add `async fn spa_asset_service(assets_root: String, req: Request<Body>) -> Result<Response, Infallible>`
   implementing the logic above.
3. Replace `app.fallback_service(ServeDir::new(&assets_root).fallback(ServeFile::new(...)))`
   with `app.fallback_service(tower::service_fn(move |req| spa_asset_service(assets_root.clone(), req)))`.

## Dependencies

None added. `tower-http` (`fs` feature) and `tower` (`util` feature, for `ServiceExt::oneshot`)
are already present in `crates/vexboard-server/Cargo.toml`.

## Configuration changes

None.

## Risks and mitigations

- **Risk:** heuristic hash detection could misclassify a legitimately non-hashed file
  that happens to end in a long hex-looking suffix.
  **Mitigation:** worst case is that file gets `no-cache` instead of `immutable` —
  correctness-safe, only a minor caching-efficiency miss. No file in the current
  frontend build output matches this edge case.
- **Risk:** removing `ServeDir::fallback()` changes behavior for directory requests.
  **Mitigation:** `ServeDir` still serves `index.html` automatically for directory
  paths (default `append_index_html_on_directories`), so `/` behaves the same as
  before — it's just now also given `no-cache, must-revalidate`.
