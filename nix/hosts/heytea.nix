{ config, pkgs, lib, modulesPath, ... }:

let
  hasGrafanaSecret = builtins.pathExists ../../secrets/grafana-secret-key.age;
  hasGrafanaAdminPassword = builtins.pathExists ../../secrets/grafana-admin-password.age;
  hasGrafanaSecrets = hasGrafanaSecret && hasGrafanaAdminPassword;
  hasTailscaleAuthKey = builtins.pathExists ../../secrets/tailscale-auth-key.age;
  hasUmamiAppSecret = builtins.pathExists ../../secrets/umami-app-secret.age;
in

{
  imports = [
    "${modulesPath}/virtualisation/digital-ocean-config.nix"
  ];

  networking.hostName = "heytea-dev";
  time.timeZone = "America/Los_Angeles";

  nix.settings.experimental-features = [ "nix-command" "flakes" ];
  nixpkgs.config.allowUnfreePredicate = pkg: builtins.elem (lib.getName pkg) [ "timescaledb" ];

  # DigitalOcean provides interface config through cloud-init; DHCP-only
  # networking can leave nixos-anywhere installs unreachable after reboot.
  networking.useDHCP = lib.mkForce false;
  services.cloud-init = {
    enable = true;
    network.enable = true;
    settings = {
      datasource_list = [ "ConfigDrive" ];
      datasource.ConfigDrive = { };
      cloud_config_modules = lib.mkForce [ ];
      cloud_final_modules = lib.mkForce [ "final-message" ];
    };
  };

  services.openssh = {
    enable = true;
    hostKeys = [
      {
        path = "/etc/ssh/ssh_host_ed25519_key";
        type = "ed25519";
      }
    ];
    settings = {
      PasswordAuthentication = false;
      PermitRootLogin = "prohibit-password";
    };
  };

  users.users.root.openssh.authorizedKeys.keys = [
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIE40ISu3ydCqfdpb26JYD5cIN0Fu0id/FDS+xjB5zpqu"
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPyic30I+SaDw0Lz/EFpMNeHCwxpwPfkgfR6uz3g7io7"
  ];

  boot.loader.grub.enable = lib.mkDefault true;
  boot.loader.grub.devices = lib.mkDefault [ "/dev/vda" ];

  fileSystems."/" = lib.mkDefault {
    device = "/dev/disk/by-label/nixos";
    fsType = "ext4";
  };

  age.secrets = lib.mkMerge [
    (lib.optionalAttrs hasGrafanaSecret {
      grafana-secret-key = {
        file = ../../secrets/grafana-secret-key.age;
        owner = "grafana";
      };
    })
    (lib.optionalAttrs hasGrafanaAdminPassword {
      grafana-admin-password = {
        file = ../../secrets/grafana-admin-password.age;
        owner = "grafana";
      };
    })
    (lib.optionalAttrs hasTailscaleAuthKey {
      tailscale-auth-key.file = ../../secrets/tailscale-auth-key.age;
    })
    (lib.optionalAttrs hasUmamiAppSecret {
      umami-app-secret.file = ../../secrets/umami-app-secret.age;
    })
  ];

  services.grafana.enable = lib.mkIf (!hasGrafanaSecrets) (lib.mkForce false);
  services.grafana.settings.security.secret_key = lib.mkIf hasGrafanaSecret "$__file{/run/agenix/grafana-secret-key}";
  services.grafana.settings.security.admin_password = lib.mkIf hasGrafanaAdminPassword "$__file{/run/agenix/grafana-admin-password}";

  services.umami = lib.mkIf hasUmamiAppSecret {
    enable = true;
    settings = {
      APP_SECRET_FILE = config.age.secrets.umami-app-secret.path;
      HOSTNAME = "127.0.0.1";
      PORT = 3003;
      DISABLE_TELEMETRY = true;
    };
  };

  services.heytea = {
    enable = true;
    domain = "heytea.dev";
    apiDomain = "api.heytea.dev";
    docsDomain = "docs.heytea.dev";
    mcpDomain = "mcp.heytea.dev";
    statusDomain = "status.heytea.dev";
    analyticsDomain = "analytics.heytea.dev";
    shopConfigPath = "/etc/heytea/shop-id";
  };

  services.tailscale.enable = true;

  systemd.services.tailscale-autoconnect = lib.mkIf hasTailscaleAuthKey {
    wantedBy = [ "multi-user.target" ];
    wants = [ "network-online.target" "tailscaled.service" ];
    after = [ "network-online.target" "tailscaled.service" ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
    };
    script = ''
      if ${pkgs.tailscale}/bin/tailscale ip -4 >/dev/null 2>&1; then
        exit 0
      fi

      auth_key="$(cat ${config.age.secrets.tailscale-auth-key.path})"
      ${pkgs.tailscale}/bin/tailscale up \
        --auth-key="$auth_key" \
        --hostname=heytea-dev \
        --advertise-tags=tag:server
    '';
  };

  networking.firewall = {
    enable = true;
    allowedTCPPorts = [ 22 80 443 ];
    trustedInterfaces = [ "tailscale0" ];
  };

  system.stateVersion = "24.11";
}
