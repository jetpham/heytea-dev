{ config, pkgs, lib, ... }:

{
  networking.hostName = "heytea";
  time.timeZone = "America/Los_Angeles";

  nix.settings.experimental-features = [ "nix-command" "flakes" ];
  nixpkgs.config.allowUnfreePredicate = pkg: builtins.elem (lib.getName pkg) [ "timescaledb" ];

  boot.loader.grub.enable = true;
  boot.loader.grub.devices = [ "/dev/vda" ];

  fileSystems."/" = {
    device = "/dev/disk/by-label/nixos";
    fsType = "ext4";
  };

  services.grafana.settings.security.secret_key = "$__file{/run/agenix/grafana-secret-key}";

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

  networking.firewall = {
    enable = true;
    allowedTCPPorts = [ 80 443 ];
    trustedInterfaces = [ "tailscale0" ];
  };

  system.stateVersion = "24.11";
}
