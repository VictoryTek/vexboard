# Service Icons — Phase 1 Specification

## Current State Analysis

VexBoard already has full icon infrastructure in place:

- `icon: Option<String>` exists on `Service`, `CreateService`, `UpdateService`, `ServiceWithStatus`, `ServiceData`, and `ServiceResponse` structs.
- The SQLite schema stores `icon TEXT` (migration `001_init.sql`).
- `ServiceCard` and `QuickLinkCard` already distinguish between URL-based icons and text/emoji icons:
  - URL detected via `.starts_with("http://") || .starts_with("https://")`
  - Broken URL fallback: renders first letter of display name
- `EditModal` auto-generates a favicon URL from the service URL (`{scheme}://{host}/favicon.ico`) and allows manual override.

What is **missing**:
- No icon search/suggestion UI — users must know and type the URL themselves.
- No integration with a curated self-hosted service icon library.
- No fallback chain: favicon → icon library → letter.

---

## Problem Definition

When a service is added (manually or via auto-discovery), the user must either:
1. Know and manually enter a URL for an icon, or
2. Accept the auto-generated favicon URL (which is often missing or low-quality for self-hosted apps).

There is no discovery mechanism for high-quality, curated icons for common self-hosted services.

---

## External Icon Library Research

### selfhst/icons

- **GitHub:** https://github.com/selfhst/icons
- **CDN:** `https://cdn.jsdelivr.net/gh/selfhst/icons@main`
- **License:** Mixed per-icon (repository is MIT for tooling; individual icons carry their upstream license — acceptable for display use in a self-hosted dashboard)
- **Self-hostable:** Yes — Docker image at `ghcr.io/selfhst/icons:latest`
- **Icon count:** 2,787 icons (as of research date)

### Manifest

A machine-readable JSON manifest is available at:
```
https://raw.githubusercontent.com/selfhst/icons/main/index.json
```

Structure (per entry):
```json
{
  "Name": "Actual Budget",
  "Reference": "actual-budget",
  "SVG": "Yes",
  "PNG": "Yes",
  "WebP": "Yes",
  "Light": "Yes",
  "Dark": "Yes",
  "Category": "Self-Hosted",
  "Tags": "",
  "CreatedAt": "2024-08-16 00:31:51+00:00"
}
```

**Naming convention:** `Reference` = lowercase name, non-alphanumeric chars replaced with hyphens.

### URL Pattern

```
https://cdn.jsdelivr.net/gh/selfhst/icons@main/{format}/{reference}-{variant}.{ext}
```

Examples:
- `https://cdn.jsdelivr.net/gh/selfhst/icons@main/svg/actual-budget.svg` (default/color)
- `https://cdn.jsdelivr.net/gh/selfhst/icons@main/svg/actual-budget-light.svg`
- `https://cdn.jsdelivr.net/gh/selfhst/icons@main/svg/actual-budget-dark.svg`
- `https://cdn.jsdelivr.net/gh/selfhst/icons@main/webp/actual-budget.webp`
- `https://cdn.jsdelivr.net/gh/selfhst/icons@main/png/actual-budget.png`

Available formats: `svg`, `png`, `webp`, `avif`, `ico`

For VexBoard we will use **SVG** as the default format (scalable, crisp at all sizes, smallest file size for logos).

---

## Proposed Solution Architecture

### Overview

A two-part feature:

1. **Icon picker in `EditModal`** — a search input that queries a bundled icon manifest and suggests matching icons from selfhst/icons. Selecting one fills the icon URL field.
2. **Improved fallback chain in `ServiceCard` / `QuickLinkCard`** — when no icon is set, try the service's favicon, then the letter fallback.

### Approach: Embed the Manifest at Build Time

Rather than fetching the manifest at runtime from GitHub (which would fail for air-gapped users and introduce a network dependency on the dashboard UI itself), we bundle a **static copy of `index.json`** into the WASM frontend at build time as a Trunk-copied asset.

This gives us:
- Zero runtime network dependency for icon search
- Air-gap compatibility
- No backend changes required
- Manifest can be refreshed by updating a single file

The icon *images themselves* are fetched from the CDN on demand (when the user selects an icon or when a card renders). This is acceptable because:
- The user is already online to be using the browser UI
- Air-gapped users can still manually enter a local/internal URL
- We do not bundle 2,787 icon images into the binary

### Configuration

Add an optional `icon_cdn_base` setting to `[server]` config, defaulting to the jsDelivr CDN. Air-gapped or self-hosting users can point this at their own selfhst/icons Docker instance.

```toml
[server]
icon_cdn_base = "https://cdn.jsdelivr.net/gh/selfhst/icons@main"
```

This value is exposed via a new backend API endpoint `GET /api/v1/config/public` (or added to the existing health/config response), so the frontend can construct icon URLs without hardcoding the CDN base.

### Components to Change

#### Backend (vexboard-server)

1. **`crates/vexboard-server/src/config.rs`** — add `icon_cdn_base: String` to `ServerConfig` with default value.
2. **`crates/vexboard-server/src/api/`** — expose `icon_cdn_base` in an existing or new public config endpoint so the frontend knows what CDN base to use.
3. **`config/default.toml`** — add `icon_cdn_base` entry.

#### Frontend (vexboard-frontend)

4. **`crates/vexboard-frontend/public/icons-index.json`** — static copy of the selfhst/icons `index.json` manifest (2,787 entries, ~200 KB). Copied into dist by Trunk.
5. **`crates/vexboard-frontend/index.html`** — add `<link data-trunk rel="copy-file" href="public/icons-index.json" />`.
6. **`crates/vexboard-frontend/src/components/icon_picker.rs`** — new Leptos component: search input + dropdown of matching icons. Props: `on_select: Callback<String>` (called with the full CDN URL). Loads manifest from `/icons-index.json` on first open (fetch, parse, cache in a signal). Fuzzy-matches on `Name` and `Reference` fields.
7. **`crates/vexboard-frontend/src/components/modal_edit.rs`** — integrate `IconPicker` below the icon URL input field. When an icon is selected, it populates the icon field. The existing manual URL entry is preserved.
8. **`crates/vexboard-frontend/src/components/service_card.rs`** — improve fallback: if icon is None/empty and a service URL exists, try `{origin}/favicon.ico` (already done for auto-generated icons in `modal_edit.rs` — align the display logic to match).

### What We Are NOT Doing

- No local icon file uploads (out of scope; adds significant complexity).
- No server-side icon proxying (the CDN is public; proxying adds latency and maintenance cost).
- No bundling of icon image files into the binary.
- No changes to the `icon` database column or API shape — `icon` remains `Option<String>` containing a URL.

---

## Implementation Steps

1. Add `icon_cdn_base` to `ServerConfig` and `default.toml`.
2. Expose it via `GET /api/v1/config/public` (new minimal endpoint returning `{ "icon_cdn_base": "..." }`).
3. Download `index.json` from selfhst/icons and place at `crates/vexboard-frontend/public/icons-index.json`.
4. Register it in `index.html` as a Trunk copy-file asset.
5. Implement `icon_picker.rs` Leptos component (fetch manifest, filter, render dropdown with preview).
6. Integrate `IconPicker` into `modal_edit.rs`.
7. Adjust `service_card.rs` fallback to use favicon from service URL when `icon` is None.

---

## Dependencies

No new Rust crates required. The manifest fetch uses the existing `gloo-net` or `reqwest` wasm client already used in the frontend (verify which is in use).

No new npm/JS dependencies.

---

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| jsDelivr CDN unavailable | Config override `icon_cdn_base` lets users point at self-hosted instance |
| Air-gapped deployment | Icon search works offline (manifest is local); image display degrades gracefully (letter fallback) |
| Manifest staleness | Documented as a manual update step; manifest is a static file in the repo |
| 200 KB manifest parse time | Parse once on first picker open; cache result in a Leptos stored signal |
| selfhst/icons naming doesn't match service display_name | Fuzzy search on both `Name` and `Reference` fields; user can still type any URL manually |
