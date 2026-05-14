# Operations

## Public Domains

- `heytea.dev`
- `api.heytea.dev`
- `docs.heytea.dev`
- `mcp.heytea.dev`
- `analytics.heytea.dev`
- `status.heytea.dev`

These are Cloudflare-proxied.

## Tailnet-Only Domains

- `grafana.heytea.dev`
- `prom.heytea.dev`
- `loki.heytea.dev`
- `otel.heytea.dev`
- `postgres.heytea.dev`

Postgres should bind only to localhost or Tailscale, not Caddy/public interfaces.

## Backups

Daily restic backups go to Backblaze B2. At minimum, back up Postgres logical dumps.

## Monitoring

Prometheus scrapes the API and blackbox probes public URLs through Cloudflare. Grafana is tailnet-only.
