{ lib, modulesPath, ... }:

{
  imports = [
    "${modulesPath}/virtualisation/digital-ocean-config.nix"
    ../disko/digitalocean.nix
  ];

  networking.hostName = "heytea-dev";
  time.timeZone = "America/Los_Angeles";

  nix.settings.experimental-features = [ "nix-command" "flakes" ];

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

  boot.loader.grub = {
    enable = true;
    devices = lib.mkForce [ "/dev/vda" ];
  };

  services.tailscale.enable = true;

  networking.firewall = {
    enable = true;
    allowedTCPPorts = [ 22 ];
    trustedInterfaces = [ "tailscale0" ];
  };

  system.stateVersion = "24.11";
}
