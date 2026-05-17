# Poller

The poller is the only component that calls upstream HeyTea endpoints. Public requests never trigger upstream polling.

## Discovery

The poller discovers shops from two Android-app backends, then persists normalized locations under stable public slugs. Upstream HeyTea shop IDs stay internal and are never exposed in public API responses.

- Overseas app hosts: `https://app-{region}.heytea-co.com`, currently `us`, `ca`, `gb`, `sg`, `au`, `my`, `kr`, and `cn` for Hong Kong/Macao.
- Mainland app host: `https://go.heytea.com`, using the app's China city catalog and per-city shop lists.

`app-jp.heytea-co.com` and `app-fr.heytea-co.com` resolve, but their regional headers currently return `SUCCESS` with empty region/shop catalogs, so they are intentionally not polled yet.

Full catalog discovery is intentionally slower than wait polling. Configure it with `HEYTEA_CATALOG_INTERVAL_SECONDS` (default `86400`). Per-minute upstream request concurrency is bounded by `HEYTEA_UPSTREAM_CONCURRENCY` (default `16`).

## Upstream Endpoints

The poller uses public app endpoints discovered from the Android app and verified with safe unauthenticated probes.

### Wait Time

```text
POST /api/service-ofc/openapi/agent/expect-time/shop/list
```

Normalized fields retained:

- pickup wait minutes
- delivery estimate minutes
- cups in progress
- orders in progress
- estimate flag
- optional upstream wait text
- observed timestamp

### Store Metadata / Open State

```text
GET /api/service-smc/openapi/app/user/closest/shop-list
POST /api/service-smc/grayapi/shop-list
```

The poller retains each discovered location's:

- normalized store name
- normalized address
- open state (`is_open`)
- enabled state
- takeaway support
- business hours JSON
- observed timestamp

Open state is catalog metadata. It follows `HEYTEA_CATALOG_INTERVAL_SECONDS`; per-minute wait polling does not currently provide a reliable explicit open/closed signal.

### Batched Wait Times

International wait times are requested in batches of 10 shop IDs. Mainland China wait times use batches of 100 against `go.heytea.com`, which accepted larger batches in probes.

At the current discovered catalog size this is about 53 wait requests per minute. If a batch fails, the poller records the failed batch and retries the shops individually so one stale shop ID does not suppress the rest of the batch.

### Notice

```text
GET /api/service-smc/openapi/app/shop/notice/{shopId}
```

Retains the current notice text array, if any. Notices are polled every minute for providers that expose notice endpoints. Today that means the `app-{region}.heytea-co.com` providers; mainland China notice endpoints are not confirmed.

### Closing Notice

```text
GET /api/service-smc/openapi/app/event/pop-up/shop-before-closed?shopId={shopId}
```

Retains the current closing notice text array, if any. Closing notices are current-only; there is no notice history table.

## Retention

Raw upstream JSON is not retained. The poller parses upstream responses immediately and stores normalized wait observations.

- Wait observations: 24 hour raw retention.
- Wait aggregates: 1 minute per-location Timescale continuous aggregate retained for 7 days.
- Notices: current state only, no history.
- Upstream endpoint health: 7 day retention.
