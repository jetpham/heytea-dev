# heytea.dev

Live wait-time and status dashboard for public HeyTea locations.

## Why This Exists

I wanted to go to HeyTea with my friends and did not want to be surprised by the wait time. Ellie said it would be nice to have an app that just checked the wait time, so I decompiled the Android app and found the public endpoints that power wait-time data.

The first version was a singleton app for the Downtown Metreon location in San Francisco. Later, a friend was leaving for the UK, so I extended it to work for every location I could find, including China. China required decompiling the China version of the app too, then stitching both endpoint families into one service.

The result is a small public service for nerds who like HeyTea.

## What It Does

The public API uses stable location slugs and never accepts or returns upstream shop IDs. Status, wait, notice, history, and stream endpoints require an explicit location slug.

Live values are considered fresh until the next expected poll: `ttl_seconds = max(0, observed_at + poll_interval - now)`. Poller commits normalized data to Postgres, sends a Postgres notification, and the API broadcasts `status.updated` to connected SSE clients.

## Technical Breakdown

- `heytea-poller` discovers locations and polls public HeyTea app endpoints for wait times and notices.
- Postgres + TimescaleDB store the normalized location catalog, current status, and wait-time history.
- `heytea-api` serves JSON, OpenAPI, health/readiness, metrics, and location-specific SSE streams.
- `heytea-site` serves the finder, location pages, docs shell, status page, and discovery files.
- `heytea-mcp` exposes anonymous HTTP MCP tools for agents.
- `heytea-ssh-tui` lets anyone run `ssh heytea.dev` for a readonly terminal view.
- The Rust SDK crate and `heytea` CLI make the public API scriptable.
- Postgres notifications fan out live updates without Redis.
- NixOS, Caddy, deploy-rs, agenix, and Tailscale run the minimal production host.
- Caddy serves HTTP/1.1, HTTP/2, HTTP/3/QUIC, TLS 1.2, and TLS 1.3.

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
nix build .#heytea-api .#heytea-poller .#heytea-mcp .#heytea-ssh-tui .#heytea-cli .#heytea-assets .#heytea-site-assets .#heytea-site .#heytea-migrations --no-link
```

## CI/CD

GitHub Actions run CI on pull requests and pushes to `main`, deploy production after successful CI on `main`, publish the Rust SDK from GitHub releases, and attach the Linux CLI tarball to releases. See `docs/ci-cd.md` for required GitHub secrets, environments, and Tailscale/crates.io setup.

## DigitalOcean NixOS Deploy

Production uses a normal DigitalOcean Ubuntu droplet, `nixos-anywhere` for the initial NixOS install, and deploy-rs for updates. See `docs/deploy-nixos-digitalocean.md`.
