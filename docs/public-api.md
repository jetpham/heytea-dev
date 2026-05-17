# Public API

Base URL: `https://api.heytea.dev`

The API is slug-shaped. It exposes stable public location slugs, never upstream shop IDs, and has no `/v1` prefix. Legacy singleton endpoints remain as aliases for `downtown-metreon`.

## Endpoints

- `GET /locations`: public locations with catalog open state and current pickup wait.
- `GET /locations/{slug}`: one public location.
- `GET /locations/{slug}/status`: current store state, wait-time values, notices, and freshness.
- `GET /locations/{slug}/wait-time`: current wait-time and queue values.
- `GET /locations/{slug}/notice`: current store notices as an array.
- `GET /locations/{slug}/closing-notice`: current closing notices as an array.
- `GET /locations/{slug}/history?range=today`: one-minute historical wait-time values.
- `GET /locations/{slug}/stream`: server-sent live updates for a location.
- `GET /status`, `/wait-time`, `/notice`, `/closing-notice`, `/history`, `/stream`: aliases for `downtown-metreon`.
- `GET /openapi.json`: OpenAPI schema.

## Freshness And TTL

Live resources use the current observation time and 60 second poll interval for freshness:

```text
ttl_seconds = max(0, observed_at + poll_interval - now)
```

The API sends `Cache-Control: public, max-age=<ttl_seconds>, must-revalidate` and `x-data-ttl-seconds` on live JSON endpoints. SSE streams are not cached and receive `status.updated` events after successful poll commits to Postgres.

Wait times and notices are polled every minute. `isOpen` is upstream catalog metadata and follows the slower catalog refresh interval, not the wait-time poll interval.

## History Ranges

`range` is the lookback window. History is always returned at one-minute resolution.

Supported ranges:

- `today`
- `1h`
- `6h`
- `24h`
- `7d`

`today` returns observations from the current local calendar day for that location.

Raw normalized wait observations are retained for 24 hours. A one-minute per-location Timescale aggregate is retained for 7 days. The API never stores or returns raw upstream JSON.

## MCP

The public MCP endpoint is `POST https://mcp.heytea.dev/mcp`. It is anonymous and exposes tools for location listing, nearest-location lookup, status, wait time, notices, closing notices, and history. `get_history` defaults to `today` and `downtown-metreon`.
