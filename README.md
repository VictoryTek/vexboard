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
  notifications.webhooks = [
    { url = "https://hooks.example.com/vexboard";
      events = [ "service.down" "service.up" ]; }
  ];
};
```

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