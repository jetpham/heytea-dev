# Architecture

heytea.dev is a singleton live status platform for the HeyTea Downtown Metreon location.

The public API never accepts or returns an upstream shop ID. The poller reads a one-line config file containing the upstream HeyTea shop ID and writes normalized observations into Postgres/TimescaleDB.

```text
HeyTea public app endpoints
  -> heytea-poller
  -> Postgres + TimescaleDB
  -> heytea-api
  -> SolidJS frontend / MCP / SDK / CLI
```

Redis is used for rate limits, short hot cache, and future SSE fanout. Postgres remains canonical.

## Services

- `heytea-api`: Axum API, OpenAPI JSON, Scalar docs route, SSE stream.
- `heytea-poller`: polls upstream every minute and persists normalized state.
- `heytea-mcp`: public anonymous HTTP MCP endpoint.
- `frontend`: SolidJS/Vite static site.
- `postgres`: canonical state and Timescale history.
- `redis`: hot path support.
- `umami`: self-hosted analytics backed by Postgres.
- `prometheus`, `blackbox_exporter`, `loki`, `grafana`, `otel-collector`: observability.
