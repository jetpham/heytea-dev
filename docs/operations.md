# Operations

## Public Domains

- `heytea.dev`
- `api.heytea.dev`
- `docs.heytea.dev`
- `mcp.heytea.dev`
- `analytics.heytea.dev`
- `status.heytea.dev`

These are Cloudflare DNS-only records pointing at the DigitalOcean droplet. Caddy terminates HTTPS on the host.

## Tailnet-Only Domains

- `grafana.heytea.dev`
- `prom.heytea.dev`
- `loki.heytea.dev`
- `otel.heytea.dev`
- `postgres.heytea.dev`

Postgres uses the local Unix socket with peer authentication. It should not bind to Caddy or public interfaces.

## Backups

Daily restic backups go to Backblaze B2. At minimum, back up Postgres logical dumps.

## Monitoring

Prometheus scrapes the API and blackbox probes public URLs. Grafana is tailnet-only. Umami serves public analytics at `analytics.heytea.dev`.
