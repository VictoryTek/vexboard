{ config, lib, pkgs, ... }:
let
  cfg = config.services.vexboard;
in
{
  options.services.vexboard = {
    enable = lib.mkEnableOption "VexBoard dashboard";

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
      description = "Directory for VexBoard data (database, etc).";
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Whether to open the firewall for VexBoard's port.";
    };

    settings = lib.mkOption {
      type = lib.types.attrs;
      default = {};
      description = "Extra settings merged into config.toml.";
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
    users.groups.vexboard = {};

    security.pam.services.vexboard = {};

    systemd.services.vexboard = {
      description = "VexBoard Dashboard";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" "dbus.service" ];
      serviceConfig = {
        ExecStart = "${pkgs.vexboard}/bin/vexboard-server";
        StateDirectory = "vexboard";
        DynamicUser = false;
        User = "vexboard";
        Group = "vexboard";
        SupplementaryGroups = [ "shadow" "systemd-journal" ];
        PrivateTmp = true;
        ProtectSystem = "strict";
        ReadWritePaths = [ cfg.dataDir ];
        Environment = [
          "VEXBOARD_SERVER__HOST=${cfg.host}"
          "VEXBOARD_SERVER__PORT=${toString cfg.port}"
          "VEXBOARD_DATABASE__PATH=${cfg.dataDir}/vexboard.db"
        ];
      };
    };

    networking.firewall.allowedTCPPorts = lib.mkIf cfg.openFirewall [ cfg.port ];
  };
}
