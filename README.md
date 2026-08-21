# VexBoard

A self-hosted server dashboard designed for NixOS, also runnable via Docker Compose. VexBoard auto-discovers running systemd services, displays real-time status and system metrics, and lets users manage service metadata through a polished web UI.

## Features

- **systemd Discovery** — Automatically finds running services via D-Bus
- **Live Metrics** — CPU, RAM, network, and disk I/O streamed over SSE
- **Uptime Probing** — HTTP health checks with latency tracking
- **Service Management** — Display name, icon, URL, group, description — no YAML editing
- **Dark-first UI** — Built with Leptos (Rust → WASM), Tailwind CSS, data-dense layout

## Quick Start

### Docker Compose

```bash
docker compose up -d
```

Visit `http://localhost:7280`.

### NixOS

Add VexBoard as a flake input, apply the overlay so `pkgs.vexboard` is
available, then enable the module:

```nix
# flake.nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    vexboard.url  = "github:VictoryTek/vexboard";
  };

  outputs = { nixpkgs, vexboard, ... }: {
    nixosConfigurations.myserver = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        vexboard.nixosModules.default
        ({ pkgs, ... }: {
          nixpkgs.overlays = [ vexboard.overlays.default ];

          services.vexboard = {
            enable      = true;
            port        = 7280;
            openFirewall = true;

            # Path to a file containing:  VEXBOARD_AUTH__SECRET=<random>
            # Generate one with:  openssl rand -base64 48
            secretFile = "/run/secrets/vexboard-auth-secret";
          };
        })
      ];
    };
  };
}
```

That's all that's required. The module creates the `vexboard` system user,
registers a systemd service, and writes `/etc/vexboard/config.toml` from
the module options.

**Extra settings** beyond host/port can be passed as structured Nix attrs:

```nix
services.vexboard.settings = {
  auth.secure_cookies    = true;   # enable when behind TLS
  discovery.interval_secs = 30;
  notifications.webhook_secret = "";   # optional global HMAC signing secret
};
```

Notification destinations (webhook / Discord / ntfy) are managed from the
Settings UI and stored in the database, not declared here.

**Override the package** (e.g. to use a local checkout):

```nix
services.vexboard.package =
  vexboard.packages.${pkgs.system}.vexboard;
```

### Development

```bash
# Enter dev shell — provides Rust (with WASM target), Trunk, sqlx-cli,
# wasm-bindgen-cli, wasm-opt, and Tailwind CSS (requires Nix with flakes)
nix develop

# Start the backend (creates dev.db on first run)
cd crates/vexboard-server && cargo run

# Start the frontend with hot-reload (separate terminal)
cd crates/vexboard-frontend && trunk serve
```

Visit `http://localhost:8080` (Trunk proxy) or `http://localhost:7280`
(backend directly).

### Build the Nix package

```bash
nix build
```

On first run this will fail twice with hash mismatches for
`wasm-bindgen-cli`. Copy the `got: sha256-...` value from each error
into `flake.nix` (the two `fakeHash` placeholders), then re-run. This
is a one-time step required to pin the CLI to the exact version in
`Cargo.lock`.

## Architecture

VexBoard is a Cargo workspace with two crates: a native Axum server binary
and a client-side-rendered Leptos WASM app. The server serves the compiled
frontend as static assets and exposes a REST + SSE API.

```
┌─────────────────────────────────────────────────────────┐
│  Browser                                                │
│  Leptos WASM (crates/vexboard-frontend)                 │
│  • Reactive UI — service cards, metric bars, modals     │
│  • Client-side routing (/dashboard, /settings, …)       │
│  • gloo-net HTTP calls to /api/v1/*                     │
│  • EventSource → /api/v1/metrics/stream (SSE)           │
└────────────────────────┬────────────────────────────────┘
                         │ HTTP / SSE
┌────────────────────────▼────────────────────────────────┐
│  Axum HTTP Server (crates/vexboard-server)              │
│                                                         │
│  REST API (/api/v1/*)          Middleware stack         │
│  • services  CRUD              • session auth           │
│  • groups    CRUD              • require_auth           │
│  • quick-links CRUD            • require_admin          │
│  • users     CRUD (admin)      • CORS                   │
│  • audit     read              • login rate-limiter     │
│  • discovery read/refresh      • tower-sessions (SQLite)│
│  • setup     first-run                                  │
│  • health    liveness probe                             │
│                                                         │
│  SSE endpoint                  Background tasks         │
│  /api/v1/metrics/stream        • systemd discovery loop │
│  tokio broadcast channel       • Docker/Podman loop     │
│                                • uptime probe loop      │
│                                • metrics collector      │
├─────────────────────────────────────────────────────────┤
│  Data layer                                             │
│  • SQLite via sqlx (services, groups, users, sessions,  │
│    quick_links, probe_results, audit_log)               │
│  • Migrations embedded in binary (sqlx::migrate!)       │
├─────────────────────────────────────────────────────────┤
│  System interfaces                                      │
│  • zbus → org.freedesktop.systemd1 (unit enumeration)  │
│  • bollard → Docker/Podman Unix sockets                 │
│  • /proc/stat, /proc/meminfo, /proc/net/dev (metrics)  │
│  • libpam (optional, feature = "pam-auth")             │
└─────────────────────────────────────────────────────────┘
```

### Request lifecycle

1. The browser loads `index.html` and the WASM bundle from the server's
   static file handler (`tower-http ServeDir`).
2. Leptos boots in the browser, reads `localStorage` for theme, and begins
   fetching `/api/v1/services` and `/api/v1/metrics/stream`.
3. The metrics SSE endpoint subscribes to a `tokio::sync::broadcast` channel
   that a background task feeds every 2 s by reading `/proc`.
4. Discovery loops run on configurable intervals (default 60 s), writing
   discovered units to an `Arc<RwLock<Vec<DiscoveredUnit>>>` in memory.
5. Probe loops issue HTTP requests to each service URL and write results to
   `probe_results`; history older than `probe.history_retention_days` is
   pruned per service.
6. All mutating API calls are gated by `require_auth` middleware (session
   cookie check) and write a row to the `audit_log` table.

### Codebase layout

```
crates/
  vexboard-server/src/
    main.rs             — startup, config load, router assembly
    config.rs           — AppConfig and sub-structs (serde + config crate)
    api/                — Axum handlers (services, groups, auth, …)
    db/                 — SQLite pool init, migrations, model types, helpers
    discovery/          — systemd (zbus) + Docker (bollard) discovery loops
    probe/              — HTTP uptime prober + history pruning
    metrics.rs          — /proc reader + SSE broadcaster
    middleware/         — require_auth, require_admin
    session_store.rs    — custom SQLite-backed tower-sessions store
    rate_limit.rs       — sliding-window login rate limiter
    pam_auth.rs         — PAM authentication (feature-gated)
  vexboard-frontend/src/
    main.rs             — Leptos app mount
    pages/              — dashboard, settings, login, setup, discovered
    components/         — service cards, modals, discovery panel, metric bar
config/
  default.toml          — all configurable knobs with inline documentation
nix/
  package.nix           — Nix derivation (backend + WASM frontend)
  module.nix            — NixOS service module
scripts/
  preflight.sh / .ps1   — local CI gate (fmt, clippy, tests, build)
```

## Troubleshooting

### No services appear on the dashboard

- **Discovery is disabled** — check `[discovery] enabled = true` in config.
- **D-Bus socket not accessible** — the server user needs access to the
  system bus. In Docker, ensure `/run/dbus/system_bus_socket` is bind-mounted
  (it is in the provided `docker-compose.yml`). In NixOS the module adds the
  `vexboard` user to `systemd-journal`; confirm with `systemctl status vexboard`.
- **All units are filtered** — `server_services_only = true` (default) shows
  only units whose unit file is under `/etc/systemd/system/`. Units installed
  by NixOS live under `/etc/systemd/system/` via symlink, so this works
  correctly on NixOS. On other distros, set `server_services_only = false`.
- **Unit names match an exclude pattern** — review `exclude_units` in config.

### Docker/Podman containers are not discovered

- Confirm the socket path exists: `ls /var/run/docker.sock` or
  `/run/podman/podman.sock`.
- The server user must have read access to the socket. In Docker Compose the
  socket is bind-mounted automatically. For systemd, add the `vexboard` user
  to the `docker` group.
- Check that the container image is not listed in `exclude_images`.
- A container can opt out with the label `vexboard.ignore=true`.

### Can't log in / first-run setup page not appearing

- On first start with no users in the database, `GET /api/v1/setup/status`
  returns `{ "needs_setup": true }` and the UI redirects to `/setup`.
- If the setup page does not appear, the database may have a pre-existing
  user row (e.g. from a previous run with a shared volume). Wipe the database
  file or volume to start fresh.
- In PAM mode (`pam-auth` feature), the setup page is intentionally disabled.
  Log in with a local system account instead.

### Services show "unknown" probe status

- Probe only runs for services that have a URL set. Edit a service and add
  its URL; probing starts within `probe.default_interval_secs` seconds.
- The server must be able to reach the service URL from inside its container
  or host. Use `http://host-ip:port` rather than `http://localhost:port` if
  the server is in Docker and the service is on the host.
- Check `RUST_LOG=vexboard_server=debug` output for probe errors.

### Metrics graph is empty

- Metrics are read from `/proc` — Linux only. On macOS or in a container
  without `/proc` the metrics endpoint will return zeros or fail silently.
- In Docker, `/proc` from the host is available inside the container by
  default; no extra mounts are needed.

### The UI loads but shows a blank page or 404

- The server looks for static assets at the path set in
  `server.assets_path`. The default `"embedded"` falls back to a relative
  `assets/` directory. Ensure you are running the binary from the project
  root (development) or that the NixOS module / Docker image has the built
  frontend at the expected path.
- In development, use `trunk serve` in `crates/vexboard-frontend` and set
  `VEXBOARD_SERVER__ASSETS_PATH` to the Trunk output directory, or just use
  the Trunk dev server proxy at `http://localhost:8080`.

### NixOS: service fails to start

- **`pkgs.vexboard` not found** — the overlay is not applied. Add
  `nixpkgs.overlays = [ inputs.vexboard.overlays.default ]` to your config.
- **`secretFile` points to a missing path** — ensure the secrets file exists
  before the service starts. With `agenix` or `sops-nix`, declare the secret
  as a dependency so it is decrypted before `vexboard.service`.
- **D-Bus access denied** — check that `dbus.service` is running and that the
  `vexboard` user is in the `systemd-journal` supplementary group (the module
  sets this automatically).
- Check logs with `journalctl -u vexboard -n 100`.

### Session expires immediately or on every restart

- The session store is SQLite-backed — sessions survive restarts by default.
- If `session_ttl_hours` is very low, increase it in config.
- If the database file is on a `PrivateTmp` path, sessions will not persist
  across restarts. The NixOS module and Docker setup both use a stable
  `StateDirectory` / named volume — do not override this to `/tmp`.

## Configuration

Configuration is loaded from (highest priority first):

1. Environment variables: `VEXBOARD_SERVER__PORT=9000`
2. `/etc/vexboard/config.toml`
3. `config/default.toml`

See [`config/default.toml`](config/default.toml) for all available options.

## Tech Stack

- **Backend**: Rust, Axum, Tokio, SQLite (sqlx), zbus
- **Frontend**: Leptos (WASM), Tailwind CSS
- **Packaging**: Nix Flake, Docker

## License

MIT