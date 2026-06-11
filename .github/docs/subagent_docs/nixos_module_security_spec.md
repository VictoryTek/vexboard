# Spec: NixOS Module Security Hardening

**Feature:** `nixos-module-security`
**Date:** 2026-06-11

---

## Current State Analysis

`nix/module.nix` exposes a NixOS service module for VexBoard. Two security issues exist:

1. **`openFirewall` description** — The default is already `false` (correct), but the description
   ("Whether to open the firewall for VexBoard's port.") does not communicate the security
   rationale or that it is an explicit opt-in. Users may not understand the implication.

2. **No secret enforcement** — `services.vexboard.secretFile` defaults to `null`. When null (or
   when the file omits `VEXBOARD_AUTH__SECRET`), the server loads the compiled-in placeholder
   `"change-me-in-production"` from `config/default.toml`. The service starts successfully with
   that placeholder, giving any network-reachable user a session signed with a public constant.

3. **Typo in `config/default.toml`** — Line 15 comment says `VEXBOARD_AUTH_SECRET` (single
   underscore) but the config loader uses separator `__`, so the correct env var is
   `VEXBOARD_AUTH__SECRET` (double underscore after prefix).

---

## Problem Definition

- A user who enables the VexBoard NixOS module without reading all documentation ends up with
  an instance whose session tokens can be forged by anyone who knows the public placeholder.
- The module gives no feedback at startup if the secret is unconfigured.
- The env-var name in the config comment is wrong, making it harder to fix the issue.

---

## Proposed Solution

### Change 1 — `openFirewall` description (nix/module.nix)

Update the description of `openFirewall` to be explicit about the security rationale:

```nix
description = ''
  Whether to open the firewall port for VexBoard. Defaults to false — firewall
  exposure must be an explicit opt-in. Enable only after configuring
  authentication (secretFile) and deciding whether plain-HTTP local-network
  access is acceptable for your threat model.
'';
```

### Change 2 — Startup secret guard (nix/module.nix)

Add a `preStart` script to the `systemd.services.vexboard` block that exits 1 (preventing
the service from starting) when `VEXBOARD_AUTH__SECRET` is unset, empty, or equal to the
known placeholder value.

Systemd propagates `EnvironmentFile` entries to all `Exec*` directives in the same unit,
so `$VEXBOARD_AUTH__SECRET` is available in `preStart` without re-sourcing.

```bash
secret="${VEXBOARD_AUTH__SECRET:-}"
if [ -z "$secret" ] || [ "$secret" = "change-me-in-production" ]; then
    echo ""
    echo "ERROR: VexBoard will not start because no auth secret has been configured."
    echo ""
    echo "  1. Generate a secret:"
    echo "     openssl rand -base64 48"
    echo ""
    echo "  2. Write it to a file owned by root (mode 0400):"
    echo "     echo 'VEXBOARD_AUTH__SECRET=<generated>' > /etc/vexboard/secret.env"
    echo "     chmod 0400 /etc/vexboard/secret.env"
    echo ""
    echo "  3. Point the NixOS option at that file:"
    echo "     services.vexboard.secretFile = \"/etc/vexboard/secret.env\";"
    echo ""
    exit 1
fi
```

### Change 3 — `secretFile` description update (nix/module.nix)

Clarify that omitting `secretFile` prevents the service from starting and update the example
env-var name to the correct double-underscore form.

### Change 4 — Fix env var comment (config/default.toml)

Change `VEXBOARD_AUTH_SECRET` → `VEXBOARD_AUTH__SECRET` on the comment line so it matches
the actual loader separator.

---

## Implementation Steps

1. Edit `nix/module.nix`:
   - Update `openFirewall.description`
   - Update `secretFile.description` (mention startup failure, correct env var name)
   - Add `preStart` script to `systemd.services.vexboard.serviceConfig`

2. Edit `config/default.toml`:
   - Fix typo on line 15 comment

---

## Dependencies

No new dependencies. Pure Nix/shell changes. Context7 not required.

---

## Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| Breaking change: existing deployments without secretFile set will fail at startup | This is intentional. The error message gives clear remediation steps. |
| preStart runs as the vexboard user (ProtectSystem=strict) but only needs to read env | Env vars are read-only process state — no filesystem access required. |
| D-Bus-unavailable environments cause SIGSEGV in tests | Unrelated to this change; already documented in CLAUDE.md as a known pre-existing issue. |
