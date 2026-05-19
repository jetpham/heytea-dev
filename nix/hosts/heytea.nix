{ config, pkgs, lib, modulesPath, ... }:

let
  hasTailscaleAuthKey = builtins.pathExists ../../secrets/tailscale-auth-key.age;
  cloudflareIpv4Cidrs = [
    "173.245.48.0/20"
    "103.21.244.0/22"
    "103.22.200.0/22"
    "103.31.4.0/22"
    "141.101.64.0/18"
    "108.162.192.0/18"
    "190.93.240.0/20"
    "188.114.96.0/20"
    "197.234.240.0/22"
    "198.41.128.0/17"
    "162.158.0.0/15"
    "104.16.0.0/13"
    "104.24.0.0/14"
    "172.64.0.0/13"
    "131.0.72.0/22"
  ];
  cloudflareIpv6Cidrs = [
    "2400:cb00::/32"
    "2606:4700::/32"
    "2803:f800::/32"
    "2405:b500::/32"
    "2405:8100::/32"
    "2a06:98c0::/29"
    "2c0f:f248::/32"
  ];
  allowCloudflareIpv4Web = lib.concatMapStringsSep "\n" (cidr: ''
    ${pkgs.iptables}/bin/iptables -A nixos-fw -p tcp -s ${cidr} -m multiport --dports 80,443 -j nixos-fw-accept
  '') cloudflareIpv4Cidrs;
  allowCloudflareIpv6Web = lib.concatMapStringsSep "\n" (cidr: ''
    ${pkgs.iptables}/bin/ip6tables -A nixos-fw -p tcp -s ${cidr} -m multiport --dports 80,443 -j nixos-fw-accept
  '') cloudflareIpv6Cidrs;
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
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIORZnYAU2nrjmek2zLVHPn+fjQh3XPezZtxPcTKUCMkk github-actions-heytea-dev-deploy-2026-05-18"
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
    allowedTCPPorts = [ 22 ];
    allowedUDPPorts = [ ];
    trustedInterfaces = [ "tailscale0" ];
    extraCommands = ''
      ${allowCloudflareIpv4Web}
      ${allowCloudflareIpv6Web}
    '';
  };

  system.stateVersion = "24.11";
}
