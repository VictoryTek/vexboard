{ config, lib, pkgs, ... }:
let
  cfg = config.services.vexboard;

  # Generate /etc/vexboard/config.toml from the module options and any extra
  # settings the user provides. env vars still take highest priority per the
  # server's load order, so nothing set here can be accidentally overridden.
  settingsFormat = pkgs.formats.toml { };

  baseConfig = {
    server = {
      host = cfg.host;
      port = cfg.port;
      # Point the server at the installed assets inside the Nix store.
      assets_path = "${cfg.package}/share/vexboard/assets";
    };
    database.path = "${cfg.dataDir}/vexboard.db";
  };

  configFile = settingsFormat.generate "vexboard.toml"
    (lib.recursiveUpdate baseConfig cfg.settings);
in
{
  options.services.vexboard = {
    enable = lib.mkEnableOption "VexBoard dashboard";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.vexboard;
      defaultText = lib.literalExpression "pkgs.vexboard";
      description = ''
        The vexboard package to use. Requires that the vexboard overlay is
        applied to nixpkgs (add `inputs.vexboard.overlays.default` to
        `nixpkgs.overlays`) or set this to the package from the flake directly:
          package = inputs.vexboard.packages.''${pkgs.system}.vexboard;
      '';
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 7280;
      description = "Port to listen on.";
    };

    host = lib.mkOption {
      type = lib.types.str;
      default = "0.0.0.0";
      description = "Address to bind to.";
    };

    dataDir = lib.mkOption {
      type = lib.types.path;
      default = "/var/lib/vexboard";
      description = "Directory for VexBoard data (SQLite database, etc.).";
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Whether to open the firewall port for VexBoard. Defaults to false —
        firewall exposure is an explicit opt-in. Enable only after configuring
        a secret (secretFile) and deciding whether plain-HTTP local-network
        access is acceptable for your threat model.
      '';
    };

    secretFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = ''
        Path to a file containing environment variable overrides loaded at
        service startup. This option MUST be set — the service will refuse to
        start if VEXBOARD_AUTH__SECRET is absent or still the default
        placeholder value.

        The file must contain at minimum:

          VEXBOARD_AUTH__SECRET=<your-random-secret>

        Generate a suitable secret with:
          openssl rand -base64 48

        Then write it to a root-owned file and reference it here:
          echo "VEXBOARD_AUTH__SECRET=$(openssl rand -base64 48)" \
            > /etc/vexboard/secret.env
          chmod 0400 /etc/vexboard/secret.env
          # In your NixOS configuration:
          services.vexboard.secretFile = "/etc/vexboard/secret.env";
      '';
    };

    settings = lib.mkOption {
      type = settingsFormat.type;
      default = { };
      description = ''
        Additional settings merged into /etc/vexboard/config.toml using
        lib.recursiveUpdate. Values here override the defaults derived from
        the other module options (host, port, dataDir). Use TOML-compatible
        Nix attribute syntax. Example:

          settings = {
            auth.secure_cookies = true;
            discovery.interval_secs = 30;
            notifications.webhooks = [
              { url = "https://hooks.example.com/vexboard";
                events = [ "service.down" "service.up" ]; }
            ];
          };
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    users.users.vexboard = {
      isSystemUser = true;
      group = "vexboard";
      home = cfg.dataDir;
      createHome = false;
      description = "VexBoard service user";
    };
    users.groups.vexboard = { };

    security.pam.services.vexboard = { };

    environment.etc."vexboard/config.toml".source = configFile;

    systemd.services.vexboard = {
      description = "VexBoard Dashboard";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" "dbus.service" ];
      preStart = ''
        secret="''${VEXBOARD_AUTH__SECRET:-}"
        if [ -z "$secret" ] || [ "$secret" = "change-me-in-production" ]; then
          echo "" >&2
          echo "ERROR: VexBoard will not start because no auth secret has been configured." >&2
          echo "" >&2
          echo "  1. Generate a secret:" >&2
          echo "       openssl rand -base64 48" >&2
          echo "" >&2
          echo "  2. Write it to a root-owned file (mode 0400):" >&2
          echo "       echo 'VEXBOARD_AUTH__SECRET=<generated>' > /etc/vexboard/secret.env" >&2
          echo "       chmod 0400 /etc/vexboard/secret.env" >&2
          echo "" >&2
          echo "  3. Set in your NixOS configuration:" >&2
          echo "       services.vexboard.secretFile = \"/etc/vexboard/secret.env\";" >&2
          echo "" >&2
          exit 1
        fi
      '';
      serviceConfig = {
        ExecStart = "${cfg.package}/bin/vexboard-server";
        StateDirectory = "vexboard";
        DynamicUser = false;
        User = "vexboard";
        Group = "vexboard";
        SupplementaryGroups = [ "shadow" "systemd-journal" ];
        PrivateTmp = true;
        ProtectSystem = "strict";
        ReadWritePaths = [ cfg.dataDir ];
        EnvironmentFiles = lib.optional (cfg.secretFile != null) cfg.secretFile;
      };
    };

    networking.firewall.allowedTCPPorts = lib.mkIf cfg.openFirewall [ cfg.port ];
  };
}
