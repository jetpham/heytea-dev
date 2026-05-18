{ config, pkgs, lib, modulesPath, ... }:

let
  hasTailscaleAuthKey = builtins.pathExists ../../secrets/tailscale-auth-key.age;
in

{
  imports = [
    "${modulesPath}/virtualisation/digital-ocean-config.nix"
  ];

  networking.hostName = "heytea-dev";
  time.timeZone = "America/Los_Angeles";

  nix = {
    settings = {
      experimental-features = [ "nix-command" "flakes" ];
      auto-optimise-store = true;
    };
    gc = {
      automatic = true;
      dates = "daily";
      options = "--delete-older-than 3d";
    };
  };
  nixpkgs.config.allowUnfreePredicate = pkg: builtins.elem (lib.getName pkg) [ "timescaledb" ];
  boot.kernel.sysctl."net.ipv6.bindv6only" = 0;
  zramSwap = {
    enable = true;
    memoryPercent = 100;
  };
  services.do-agent.enable = false;
  services.journald.extraConfig = ''
    SystemMaxUse=64M
    MaxRetentionSec=7day
  '';

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
    openFirewall = false;
    ports = [ 2222 ];
    hostKeys = [
      {
        path = "/etc/ssh/ssh_host_ed25519_key";
        type = "ed25519";
      }
    ];
    settings = {
      AllowAgentForwarding = false;
      AllowTcpForwarding = false;
      PasswordAuthentication = false;
      PermitRootLogin = "prohibit-password";
      X11Forwarding = false;
    };
  };

  users.users.root.openssh.authorizedKeys.keys = [
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIE40ISu3ydCqfdpb26JYD5cIN0Fu0id/FDS+xjB5zpqu"
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPyic30I+SaDw0Lz/EFpMNeHCwxpwPfkgfR6uz3g7io7"
  ];

  boot.loader.grub.enable = lib.mkDefault true;
  boot.loader.grub.devices = lib.mkDefault [ "/dev/vda" ];
  boot.loader.grub.configurationLimit = lib.mkDefault 3;

  fileSystems."/" = lib.mkDefault {
    device = "/dev/disk/by-label/nixos";
    fsType = "ext4";
  };

  age.secrets = lib.mkMerge [
    (lib.optionalAttrs hasTailscaleAuthKey {
      tailscale-auth-key.file = ../../secrets/tailscale-auth-key.age;
    })
  ];

  services.heytea = {
    enable = true;
    domain = "heytea.dev";
    apiDomain = "api.heytea.dev";
    docsDomain = "docs.heytea.dev";
    mcpDomain = "mcp.heytea.dev";
    statusDomain = "status.heytea.dev";
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
    allowedUDPPorts = [ 443 ];
    trustedInterfaces = [ "tailscale0" ];
  };

  system.stateVersion = "24.11";
}
