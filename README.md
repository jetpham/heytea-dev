# heytea.dev

Live wait-time and status dashboard for the HeyTea Downtown Metreon location.

This project intentionally models a single configured store. The public API never accepts or returns an upstream shop ID; the configured upstream ID lives in `config/shop-id` and is only used by the poller.

## Stack

- Rust API with Axum, sqlx, utoipa, and Scalar docs
- Rust poller service for HeyTea public app endpoints
- Public anonymous HTTP MCP server
- Rust SDK crate and CLI binary named `heytea`
- SolidJS + Vite + pnpm frontend
- Postgres + TimescaleDB for canonical state and history
- Redis for rate limits, SSE fanout, and hot cache
- NixOS, Caddy, deploy-rs, agenix
- OpenTofu for DigitalOcean and Cloudflare
- Umami analytics, Prometheus, Loki, Grafana, OpenTelemetry, blackbox_exporter
- Daily Backblaze B2 backups via restic

## Public API

Base URL: `https://api.heytea.dev`

- `GET /status`
- `GET /wait-time`
- `GET /notice`
- `GET /closing-notice`
- `GET /history?range=24h&bucket=5m`
- `GET /stream`
- `GET /healthz`
- `GET /readyz`
- `GET /metrics`
- `GET /openapi.json`

No `/v1`, no store-listing endpoints, no location endpoints, and no menu endpoints.

## Local Development

```sh
nix develop
cargo check
pnpm install
pnpm build
```

Do not run OpenTofu `apply` or deploy commands unless you intend to provision infrastructure.
