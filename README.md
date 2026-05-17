# heytea.dev

Live wait-time and status dashboard for public HeyTea locations.

The public API uses stable location slugs and never accepts or returns upstream shop IDs. Status, wait, notice, history, and stream endpoints require an explicit location slug.

Live values are considered fresh until the next expected poll: `ttl_seconds = max(0, observed_at + poll_interval - now)`. Poller commits normalized data to Postgres, sends a Postgres notification, and the API broadcasts `status.updated` to connected SSE clients.

## Stack

- Rust API with Axum, sqlx, and utoipa OpenAPI
- Rust poller service for HeyTea public app endpoints
- Public anonymous HTTP MCP server
- Rust SDK crate and CLI binary named `heytea`
- Rust/Axum + Askama server-rendered dashboard, docs, and status pages
- Vite-built browser assets for tiny SSE updates and Swagger UI docs
- Postgres + TimescaleDB for canonical state and history
- Postgres notifications for live SSE fanout; no Redis cache
- NixOS, Caddy, deploy-rs, agenix
- Umami analytics, Prometheus, Loki, Grafana, OpenTelemetry, blackbox_exporter
- Daily Backblaze B2 backups via restic

## Public API

Base URL: `https://api.heytea.dev`

- `GET /locations`
- `GET /locations/{slug}`
- `GET /locations/{slug}/status`
- `GET /locations/{slug}/wait-time`
- `GET /locations/{slug}/notice`
- `GET /locations/{slug}/closing-notice`
- `GET /locations/{slug}/history?range=today`
- `GET /locations/{slug}/stream`
- `GET /healthz`
- `GET /readyz`
- `GET /metrics`
- `GET /openapi.json`

No `/v1`, no upstream shop IDs, no menu endpoints, and no default-location aliases.

## Local Development

```sh
nix develop
cargo check
pnpm build
nix build .#heytea-api .#heytea-poller .#heytea-mcp .#heytea-cli .#heytea-assets .#heytea-site-assets .#heytea-site .#heytea-migrations --no-link
```

## CI/CD

GitHub Actions run CI on pull requests and pushes to `main`, deploy production after successful CI on `main`, publish the Rust SDK from GitHub releases, and attach the Linux CLI tarball to releases. See `docs/ci-cd.md` for required GitHub secrets, environments, and Tailscale/crates.io setup.

## DigitalOcean NixOS Deploy

Production uses a normal DigitalOcean Ubuntu droplet, `nixos-anywhere` for the initial NixOS install, and deploy-rs for updates. See `docs/deploy-nixos-digitalocean.md`.
