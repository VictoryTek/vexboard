# Service Icon Overlay Fix — Phase 1 Specification

## Current State Analysis

The icon rendering logic lives in two nearly-identical components:

- `crates/vexboard-frontend/src/components/service_card.rs` (icon block around lines 34-111)
- `crates/vexboard-frontend/src/components/quick_link_card.rs` (icon block around lines 47-61)

State derivation (`service_card.rs:34-45`):

```rust
let first = service.display_name.chars().next().unwrap_or('?');
let letter = first.to_ascii_uppercase().to_string();
let icon_opt = service.icon.clone().filter(|i| !i.is_empty());
let is_url_icon = icon_opt
    .as_ref()
    .is_some_and(|i| i.starts_with("http://") || i.starts_with("https://"));
let icon_text = if is_url_icon {
    letter.clone()
} else {
    icon_opt.clone().unwrap_or(letter)
};
let icon_url = if is_url_icon { icon_opt } else { None };
```

Render (`service_card.rs:96-111`, same pattern in `quick_link_card.rs:47-61`):

```rust
<div class="service-icon" style="position:relative; flex-shrink:0;">
    <span>{icon_text}</span>
    {icon_url.map(|src| view! {
        <img src={src} alt=""
            style="position:absolute;top:0;left:0;width:100%;height:100%;object-fit:contain;border-radius:inherit;padding:3px;"
            on:error=move |ev| { /* hides img on load error */ }
        />
    })}
</div>
```

`icon: Option<String>` (on `Service`/`ServiceData` and `QuickLinkData`) does double duty: a plain-text emoji/letter, or a URL (detected by `http(s)://` prefix). `is_url_icon` is the flag meaning "a logo image is present."

## Problem Definition

The fallback letter `<span>` is unconditionally rendered regardless of `icon_url`. When a logo URL is present, the `<img>` is layered on top via `position:absolute` instead of replacing the span. If the logo has a transparent background (true for most selfhst/icons SVGs, the icon browser's source), the letter shows through behind the logo. The `on:error` handler only hides the broken image itself — it does not restore/show the letter, but since the letter is always present anyway, this isn't currently relied upon for the fallback path.

## Proposed Solution

Replace the "always render letter + absolutely-position image on top" pattern with a true either/or render: show the `<img>` when a logo URL is present, otherwise show the letter `<span>`. On image load error, fall back to showing the letter instead of just hiding a broken image over nothing.

Approach: use a Leptos signal (e.g. `RwSignal<bool>` `img_failed`) initialized `false`. Render:

```rust
<div class="service-icon" style="position:relative; flex-shrink:0;">
    {move || match (&icon_url, img_failed.get()) {
        (Some(src), false) => view! {
            <img src={src.clone()} alt=""
                style="width:100%;height:100%;object-fit:contain;border-radius:inherit;padding:3px;"
                on:error=move |_| img_failed.set(true)
            />
        }.into_any(),
        _ => view! { <span>{icon_text.clone()}</span> }.into_any(),
    }}
</div>
```

This removes the `position:absolute`/`position:relative` overlay styling (no longer needed since only one element renders at a time) and ensures the letter is the true default — shown when there's no `icon_url`, or when the image fails to load.

Apply the identical fix to both `service_card.rs` and `quick_link_card.rs`.

## Implementation Steps

1. In `service_card.rs`: introduce an `img_failed` signal; replace the icon `<div>` inner markup with the conditional render above; remove now-unneeded `position:relative`/`position:absolute` inline styles.
2. Apply the same change to `quick_link_card.rs`.
3. No CSS file changes needed (`.service-icon` base styles in `style/main.css` are unaffected).
4. No backend, schema, or API changes.

## Dependencies

None — no new crates, no external library integration. Context7 lookup not required (internal Leptos view logic only, no new API surface).

## Configuration Changes

None.

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Regression: image flicker/failure not visually handled | `on:error` sets `img_failed`, triggering re-render to letter fallback |
| Divergence between the two components over time | Keep the two `match` blocks structurally identical; call out in review |
