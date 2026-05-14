# Public API

Base URL: `https://api.heytea.dev`

The API is singleton-shaped. There are no store IDs, no location endpoints, and no `/v1` prefix.

## Endpoints

- `GET /status`: current store state, wait-time values, notices, and freshness.
- `GET /wait-time`: current wait-time and queue values.
- `GET /notice`: current store notice.
- `GET /closing-notice`: current closing notice, if any.
- `GET /history?range=24h&bucket=5m`: bucketed historical wait-time values.
- `GET /stream`: server-sent live updates.
- `GET /openapi.json`: OpenAPI schema.

## History Buckets

`range` is the lookback window. `bucket` is the returned chart resolution.

Supported ranges:

- `1h`
- `6h`
- `24h`
- `7d`
- `30d`
- `1y`

Supported buckets:

- `1m`
- `5m`
- `15m`
- `1h`
- `1d`

Raw normalized observations are retained in Postgres, but the public API returns bucketed history for stable performance and better chart output.
