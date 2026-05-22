let
  # Synced from https://github.com/jetpham.keys.
  jetPersonal = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIE40ISu3ydCqfdpb26JYD5cIN0Fu0id/FDS+xjB5zpqu";
  jetWork = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPyic30I+SaDw0Lz/EFpMNeHCwxpwPfkgfR6uz3g7io7";

  heyteaDev = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGJ8nD1QQMhlskZityqZBEmjVjanKvmWfWd6Yvpioldk root@heytea-dev-512";
  heyteaDevNew = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAJgYNiE3uT+cwWuK5ik0ddfPFVctipFxznFTbXHSSyy root@heytea-dev-512-new";
  recipients = [ jetPersonal jetWork heyteaDev heyteaDevNew ];
in
{
  "tailscale-auth-key.age".publicKeys = recipients;
}
