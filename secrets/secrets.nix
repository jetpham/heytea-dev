let
  # Synced from https://github.com/jetpham.keys.
  jetPersonal = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIE40ISu3ydCqfdpb26JYD5cIN0Fu0id/FDS+xjB5zpqu";
  jetWork = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPyic30I+SaDw0Lz/EFpMNeHCwxpwPfkgfR6uz3g7io7";

  # Add the server SSH host key after the first boot, then rekey secrets.
  # Example:
  # heyteaDev = "ssh-ed25519 AAAA... root@heytea-dev";
  recipients = [ jetPersonal jetWork ];
in
{
  "tailscale-auth-key.age".publicKeys = recipients;
  "grafana-secret-key.age".publicKeys = recipients;
  "grafana-admin-password.age".publicKeys = recipients;
}
