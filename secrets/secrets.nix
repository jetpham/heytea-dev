let
  # Synced from https://github.com/jetpham.keys.
  jetPersonal = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIE40ISu3ydCqfdpb26JYD5cIN0Fu0id/FDS+xjB5zpqu";
  jetWork = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPyic30I+SaDw0Lz/EFpMNeHCwxpwPfkgfR6uz3g7io7";

  heyteaDev = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOEo/MYGatgZ8d8WSk9TXupJQCnVuoWAXsWsuGzTSY4P root@heytea-dev";
  recipients = [ jetPersonal jetWork heyteaDev ];
in
{
  "tailscale-auth-key.age".publicKeys = recipients;
  "grafana-secret-key.age".publicKeys = recipients;
  "grafana-admin-password.age".publicKeys = recipients;
  "umami-app-secret.age".publicKeys = recipients;
}
