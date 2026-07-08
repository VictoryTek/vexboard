# Remote Badge Color — Spec

## Current State Analysis

`ServiceCard` (crates/vexboard-frontend/src/components/service_card.rs) renders a
"source badge" pill in the top-right of each service card, colored per discovery
source:

| Source   | Color     | Hex       |
|----------|-----------|-----------|
| Docker   | cyan-blue | `#0db7ed` |
| Podman   | purple    | `#892ca0` |
| Systemd  | orange    | `#e8873a` |
| Remote   | blue      | `#5b8def` (two occurrences: lines 58, 68) |

Additionally, status badges (`.status-badge-up` / `-down` / `-unknown` in
`style/main.css`) use green (`--color-success`, ~`#22c55e`) and red
(`--color-danger`, ~`#ef4444`) for up/down, and gray for unknown.

The badge rendering pattern (service_card.rs:113-122) uses the color three ways:
solid text color, `{color}22` (13% alpha) background, `{color}40` (25% alpha)
border — this already adapts reasonably across light/dark themes since it's
alpha-blended over the card background rather than a flat fill.

## Problem

"Remote" badge color (`#5b8def`) is a blue, and Docker's badge is also a blue
(`#0db7ed`). The user wants Remote's badge changed to a distinct hue that isn't
already used elsewhere in the badge/status palette, and that stays clearly
legible on both light and dark themes.

## Proposed Solution

Replace `#5b8def` with `#ec4899` (Tailwind `pink-500`) for both "Remote" badge
occurrences. Rationale:
- Not blue (Docker), not purple (Podman), not orange (Systemd), not green/red/gray
  (status badges) — a genuinely new hue in the palette.
- High saturation/lightness gives strong contrast at low alpha (`22`/`40` suffix)
  against both light and dark card backgrounds, consistent with how the other
  three badges already achieve theme-agnostic legibility.
- No new dependency, no CSS variable needed — matches the existing inline hex
  pattern used by the other three badge colors.

## Implementation Steps

1. In `crates/vexboard-frontend/src/components/service_card.rs`, replace both
   occurrences of `"#5b8def".to_string()` (lines 58 and 68) with
   `"#ec4899".to_string()`.

## Dependencies

None (no new crates; Context7 lookup not applicable — internal styling-only
change with no external library involved).

## Configuration Changes

None.

## Risks and Mitigations

- Risk: color could visually clash with existing palette. Mitigated by choosing
  a hue (pink) not present anywhere else in the badge/status color set.
- Risk: low-alpha pink could be low-contrast on some backgrounds. Mitigated by
  using the same alpha-blend approach (`22`/`40` suffixes) already validated by
  the other three badges across both themes.
