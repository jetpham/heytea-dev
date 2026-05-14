# Poller

The poller is the only component that calls upstream HeyTea endpoints. Public requests never trigger upstream polling.

## Config

The poller reads a one-line config file:

```text
1000092
```

That value is the upstream HeyTea shop ID. It is internal and is not exposed in public API responses.

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
- observed timestamp

### Store Metadata / Open State

```text
GET /api/service-smc/openapi/app/user/closest/shop-list
```

The poller filters the result to the configured shop ID and retains:

- normalized store name
- normalized address
- open state (`is_open`)
- enabled state
- takeaway support
- business hours JSON
- observed timestamp

### Notice

```text
GET /api/service-smc/openapi/app/shop/notice/{shopId}
```

Retains the active notice text, if any.

### Closing Notice

```text
GET /api/service-smc/openapi/app/event/pop-up/shop-before-closed?shopId={shopId}
```

Retains the active closing notice text, if any.

## Retention

Raw upstream JSON is not retained. The poller parses upstream responses immediately and stores normalized values forever.
