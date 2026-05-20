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

```nix
{
  inputs.vexboard.url = "github:victorytek/vexboard";

  # In your configuration.nix:
  services.vexboard = {
    enable = true;
    port = 7280;
    openFirewall = true;
  };
}
```

### Development

```bash
# Enter dev shell (requires Nix with flakes)
nix develop

# Run the backend
cd crates/vexboard-server
cargo run

# Run the frontend (separate terminal)
cd crates/vexboard-frontend
trunk serve
```

## Architecture

```
┌────────────────────────────────────┐
│           Leptos WASM UI           │
│  (ServiceCards, MetricBar, SSE)    │
└──────────────┬─────────────────────┘
               │ HTTP + SSE
┌──────────────▼─────────────────────┐
│         Axum HTTP Server           │
│  /api/v1/services, groups, metrics │
├────────────────────────────────────┤
│  Discovery │ Prober │ Metrics      │
│  (zbus)    │(reqwest)│ (/proc)     │
├────────────────────────────────────┤
│           SQLite (sqlx)            │
└────────────────────────────────────┘
```

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