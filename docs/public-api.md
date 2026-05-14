# Public API

Base URL: `https://api.heytea.dev`

The API is singleton-shaped. There are no store IDs, no location endpoints, and no `/v1` prefix.

## Endpoints

- `GET /status`: current store state, wait-time values, notices, and freshness.
- `GET /wait-time`: current wait-time and queue values.
- `GET /notice`: current store notice.
- `GET /closing-notice`: current closing notice, if any.
- `GET /history?range=today`: one-minute historical wait-time values.
- `GET /stream`: server-sent live updates.
- `GET /openapi.json`: OpenAPI schema.

## Freshness And TTL

Live resources use the current observation time and 60 second poll interval for freshness:

```text
ttl_seconds = max(0, observed_at + poll_interval - now)
```

The API sends `Cache-Control: public, max-age=<ttl_seconds>, must-revalidate` and `x-data-ttl-seconds` on live JSON endpoints. `GET /stream` is not cached and receives `status.updated` events after each successful poll commits to Postgres.

## History Ranges

`range` is the lookback window. History is always returned at one-minute resolution.

Supported ranges:

- `today`
- `1h`
- `6h`
- `24h`
- `7d`
- `30d`
- `1y`

`today` returns observations from the current same-day open session only. If the store is currently closed, `today` returns no points.

Raw normalized observations are retained in Postgres, but the public API returns one-minute history for stable performance and better chart output.

## MCP

The public MCP endpoint is `POST https://mcp.heytea.dev/mcp`. It is anonymous and exposes tools for status, wait time, notices, closing notices, and history. `get_history` defaults to `today`.
