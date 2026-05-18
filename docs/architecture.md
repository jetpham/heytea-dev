# Architecture

heytea.dev is a live status platform for public HeyTea locations.

The public API uses stable location slugs and never accepts or returns an upstream shop ID. The poller discovers locations from public HeyTea app endpoints and writes normalized observations into Postgres/TimescaleDB.

```text
HeyTea public app endpoints
  -> heytea-poller
  -> Postgres + TimescaleDB transaction commit
  -> pg_notify('heytea_status_updated')
  -> heytea-api LISTEN task
  -> in-process SSE broadcast
  -> browser EventSource location-page updates
```

Postgres remains canonical and also provides live update fanout with `LISTEN`/`NOTIFY`. Redis is intentionally not in the active path.

Freshness for live data is based on the expected next poll:

```text
ttl_seconds = max(0, observed_at + poll_interval - now)
```

With the default 60 second poll interval, data observed 30 seconds ago has 30 seconds of freshness left. JSON endpoints expose cache headers from that value, and response bodies expose `staleAfter` where applicable.

## Services

- `heytea-api`: Axum JSON API, OpenAPI JSON, Postgres notification listener, and SSE stream.
- `heytea-site`: Axum + Askama server-rendered location finder, optimized location pages, status page, docs shell, and discovery routes. Vite builds generic docs/status browser assets.
- `heytea-poller`: refreshes the location catalog daily, polls wait times every minute, polls provider-supported current notices every minute, persists normalized state in bulk, then publishes a Postgres notification after commit.
- `heytea-mcp`: public anonymous HTTP MCP endpoint backed by the local API.
- `heytea-ssh-tui`: public anonymous readonly SSH interface backed by the local API.
- `postgres`: canonical state and Timescale history.
- `caddy`: public HTTP/1.1, HTTP/2, HTTP/3/QUIC, TLS, and reverse proxy.
- `tailscaled` and `openssh`: admin deploy path over Tailscale only.
