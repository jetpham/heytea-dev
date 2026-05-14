# OpenTofu Infra

This directory defines the DigitalOcean VPS, Cloudflare DNS/proxy settings, and Backblaze B2 backup bucket for `heytea.dev`.

Do not run `tofu apply` casually. The files are scaffolded for review and CI planning, but applying them will create or change real infrastructure.

The droplet is expected to boot from a NixOS DigitalOcean custom image built from the repository:

```sh
nix build .#nixos-do-image
```

Upload the resulting image to DigitalOcean, then pass its image ID with `-var droplet_image=<id>` or a local `terraform.tfvars`/`tofu.tfvars` file that is not committed. The default Ubuntu image has intentionally been removed so production cannot accidentally deploy the wrong OS.

The build output is `result/heytea-dev-digital-ocean.qcow2`.

For first boot, set `bootstrap_ssh_source_addresses = ["<your-ip>/32"]` temporarily so you can SSH in and join Tailscale. Remove it and re-apply OpenTofu after Tailscale works.

Public Cloudflare-proxied records:

- `heytea.dev`
- `api.heytea.dev`
- `docs.heytea.dev`
- `mcp.heytea.dev`
- `analytics.heytea.dev`
- `status.heytea.dev`

SSH should be Tailscale-only after bootstrap. The DigitalOcean firewall intentionally exposes only 80/443.
