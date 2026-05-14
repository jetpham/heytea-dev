# Architecture

heytea.dev is a singleton live status platform for the HeyTea Downtown Metreon location.

The public API never accepts or returns an upstream shop ID. The poller reads a one-line config file containing the upstream HeyTea shop ID and writes normalized observations into Postgres/TimescaleDB.

```text
HeyTea public app endpoints
  -> heytea-poller
  -> Postgres + TimescaleDB transaction commit
  -> pg_notify('heytea_status_updated')
  -> heytea-api LISTEN task
  -> in-process SSE broadcast
  -> browser EventSource dashboard updates
```

Postgres remains canonical and also provides live update fanout with `LISTEN`/`NOTIFY`. Redis is intentionally not in the active path.

Freshness for live data is based on the expected next poll:

```text
ttl_seconds = max(0, observed_at + poll_interval - now)
```

With the default 60 second poll interval, data observed 30 seconds ago has 30 seconds of freshness left. JSON endpoints expose cache headers from that value, and response bodies expose `staleAfter` where applicable.

## Services

- `heytea-api`: Axum JSON API, OpenAPI JSON, Postgres notification listener, and SSE stream.
- `heytea-site`: Axum + Askama server-rendered dashboard, status page, docs shell, and discovery routes. Vite builds the browser assets and Swagger UI bundle.
- `heytea-poller`: polls upstream every minute, persists normalized state, then publishes a Postgres notification after commit.
- `heytea-mcp`: public anonymous HTTP MCP endpoint backed by the local API.
- `postgres`: canonical state and Timescale history.
- `umami`: self-hosted analytics backed by Postgres.
- `prometheus`, `blackbox_exporter`, `loki`, `grafana`, `otel-collector`: observability.
