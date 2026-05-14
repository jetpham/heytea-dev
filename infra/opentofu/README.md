# OpenTofu Infra

This directory defines the DigitalOcean VPS, Cloudflare DNS/proxy settings, and Backblaze B2 backup bucket for `heytea.dev`.

Do not run `tofu apply` casually. The files are scaffolded for review and CI planning, but applying them will create or change real infrastructure.

Public Cloudflare-proxied records:

- `heytea.dev`
- `api.heytea.dev`
- `docs.heytea.dev`
- `mcp.heytea.dev`
- `analytics.heytea.dev`
- `status.heytea.dev`

SSH should be Tailscale-only after bootstrap. The DigitalOcean firewall intentionally exposes only 80/443.
