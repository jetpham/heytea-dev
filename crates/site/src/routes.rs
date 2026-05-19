use crate::{error::SiteError, templates, AppState};
use askama::Template;
use axum::{
    extract::{ConnectInfo, Path, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{any, get},
    Json, Router,
};
use chrono::Utc;
use heytea_core::{
    HistoryResponse, LocationPath, LocationResponse, LocationsResponse, StatusResponse,
};
use serde::de::DeserializeOwned;
use serde_json::json;
use std::{net::SocketAddr, time::Instant};
use tower_http::services::{ServeDir, ServeFile};

pub fn router(state: AppState) -> Router {
    let asset_root = state.assets.root();

    Router::new()
        .route("/", any(finder))
        .route("/status", get(status))
        .route("/docs", get(docs))
        .route("/robots.txt", get(robots))
        .route("/sitemap.xml", get(sitemap))
        .route("/llms.txt", get(llms))
        .route("/llms-full.txt", get(llms_full))
        .route("/agents.txt", get(agents))
        .route("/skill.md", get(skill))
        .route("/openapi.json", get(openapi_proxy))
        .route("/site.webmanifest", get(manifest))
        .route("/icon.svg", get(icon))
        .route("/favicon.svg", get(icon))
        .route("/og.svg", get(og_image))
        .route("/.well-known/security.txt", get(security_txt))
        .route("/.well-known/ai-plugin.json", get(ai_plugin))
        .route("/.well-known/agent.json", get(agent_json))
        .route("/.well-known/mcp.json", get(mcp_json))
        .route("/.well-known/webmcp.json", get(webmcp_json))
        .nest_service("/assets", ServeDir::new(asset_root.join("assets")))
        .route_service("/a.woff2", ServeFile::new(asset_root.join("a.woff2")))
        .route("/:slug", get(location_dashboard))
        .fallback(not_found)
        .with_state(state)
}

async fn finder(
    method: Method,
    State(state): State<AppState>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
) -> Result<Response, SiteError> {
    if htcpcp_method(&method) {
        return Ok(htcpcp_response(&method, &headers));
    }
    if method != Method::GET && method != Method::HEAD {
        return Ok(method_not_allowed().into_response());
    }

    if wants_json(&headers) {
        return Ok((
            negotiated_headers("public, max-age=60, must-revalidate"),
            Json(json!({
                "name": "heytea.dev",
                "description": "Location finder and live pickup waits for HeyTea.",
                "api": "https://api.heytea.dev/openapi.json",
                "mcp": "https://mcp.heytea.dev/mcp",
                "llms": "https://heytea.dev/llms.txt"
            })),
        )
            .into_response());
    }
    if wants_markdown(&headers) || agent_user_agent(&headers) {
        return Ok(markdown(LLMS_TXT).into_response());
    }
    if wants_text(&headers) {
        return Ok(text(LLMS_TXT).into_response());
    }

    let peer = connect_info.map(|ConnectInfo(addr)| addr);
    let inferred_place = state.geoip.lookup_request(&headers, peer);
    let attribution = inferred_place
        .as_ref()
        .and_then(|_| state.geoip.attribution());
    let locations = api_json::<LocationsResponse>(&state, "/locations").await;
    let template = templates::FinderTemplate::new(
        locations,
        inferred_place.as_ref(),
        attribution,
        evil_request(&headers),
    );
    Ok((finder_html_headers(), Html(template.render()?)).into_response())
}

async fn location_dashboard(
    State(state): State<AppState>,
    Path(path): Path<LocationPath>,
    headers: HeaderMap,
) -> Result<Response, SiteError> {
    if wants_json(&headers) {
        let location =
            api_json::<LocationResponse>(&state, &format!("/locations/{}", path.slug)).await;
        return match location {
            Some(location) => Ok((short_cache_headers(30), Json(location)).into_response()),
            None => Ok(not_found(headers).await),
        };
    }

    let evil = evil_request(&headers);
    let location_path = format!("/locations/{}", path.slug);
    let Some(location) = api_json::<LocationResponse>(&state, &location_path).await else {
        return Ok(not_found(headers).await);
    };
    let status_path = format!("/locations/{}/status", path.slug);
    let history_path = format!("/locations/{}/history?range=today", path.slug);
    let (status, history) = tokio::join!(
        api_json::<StatusResponse>(&state, &status_path),
        api_json::<HistoryResponse>(&state, &history_path)
    );
    let stream_url = format!(
        "{}/locations/{}/stream",
        state.public_api_url.trim_end_matches('/'),
        path.slug
    );
    let template =
        templates::DashboardTemplate::new(status, history, stream_url.clone(), location, evil);
    Ok((html_shell_headers(&stream_url), Html(template.render()?)).into_response())
}

async fn status(State(state): State<AppState>) -> Result<Response, SiteError> {
    let (health, readiness, locations, mcp) = tokio::join!(
        live_component(
            &state,
            "API health",
            "https://api.heytea.dev/healthz",
            "api",
            "/healthz"
        ),
        live_component(
            &state,
            "API readiness",
            "https://api.heytea.dev/readyz",
            "api",
            "/readyz"
        ),
        live_component(
            &state,
            "Location catalog",
            "https://api.heytea.dev/locations",
            "api",
            "/locations"
        ),
        live_component(&state, "MCP", "https://mcp.heytea.dev/mcp", "mcp", "/mcp"),
    );
    let components = vec![
        templates::StatusComponent {
            name: "Website".to_string(),
            target: "https://heytea.dev".to_string(),
            state: "Operational".to_string(),
            class_name: "ok".to_string(),
            uptime: "live".to_string(),
            latency: "current request".to_string(),
        },
        health,
        readiness,
        locations,
        mcp,
    ];
    let all_operational = components
        .iter()
        .all(|component| component.class_name == "ok");
    let template = templates::StatusTemplate {
        assets: state.assets.tags("src/status.ts", false),
        summary: if all_operational {
            "All systems operational"
        } else {
            "Some systems degraded"
        }
        .to_string(),
        summary_class: if all_operational { "ok" } else { "bad" }.to_string(),
        checked: templates::site_time(Utc::now()),
        components,
    };
    Ok((short_cache_headers(30), Html(template.render()?)).into_response())
}

async fn docs(State(state): State<AppState>) -> Result<Response, SiteError> {
    let template = templates::DocsTemplate {
        assets: state.assets.tags("src/docs.ts", true),
    };
    Ok((static_headers(), Html(template.render()?)).into_response())
}

async fn robots() -> impl IntoResponse {
    text("User-agent: *\nAllow: /\n\nUser-agent: GPTBot\nAllow: /\n\nUser-agent: ClaudeBot\nAllow: /\n\nUser-agent: anthropic-ai\nAllow: /\n\nUser-agent: PerplexityBot\nAllow: /\n\nUser-agent: Google-Extended\nAllow: /\n\nSitemap: https://heytea.dev/sitemap.xml\n")
}

async fn sitemap() -> impl IntoResponse {
    xml(
        r#"<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"><url><loc>https://heytea.dev/</loc><changefreq>hourly</changefreq><priority>1.0</priority></url><url><loc>https://docs.heytea.dev/</loc><changefreq>weekly</changefreq><priority>0.7</priority></url><url><loc>https://status.heytea.dev/</loc><changefreq>hourly</changefreq><priority>0.6</priority></url></urlset>"#,
    )
}

async fn llms() -> impl IntoResponse {
    markdown(LLMS_TXT)
}

async fn llms_full() -> impl IntoResponse {
    markdown(LLMS_FULL_TXT)
}

async fn agents() -> impl IntoResponse {
    text(AGENTS_TXT)
}

async fn skill() -> impl IntoResponse {
    markdown(SKILL_MD)
}

async fn manifest() -> impl IntoResponse {
    (
        static_headers(),
        Json(json!({
            "name": "heytea.dev",
            "short_name": "heytea.dev",
            "description": "Live wait time and status for HeyTea locations.",
            "start_url": "/",
            "display": "standalone",
            "background_color": "#09100c",
            "theme_color": "#101510",
            "icons": [{ "src": "/icon.svg", "sizes": "any", "type": "image/svg+xml" }]
        })),
    )
}

async fn icon() -> impl IntoResponse {
    svg_owned(
        templates::favicon_svg(),
        "public, max-age=31536000, immutable",
    )
}

async fn og_image(State(state): State<AppState>) -> impl IntoResponse {
    let (status, history) = tokio::join!(
        api_json::<StatusResponse>(&state, "/locations/downtown-metreon/status"),
        api_json::<HistoryResponse>(&state, "/locations/downtown-metreon/history?range=today")
    );
    let view = templates::DashboardView::new(
        status.as_ref(),
        history.as_ref(),
        "downtown metreon",
        "America/Los_Angeles",
    );
    svg_owned(og_svg(&view), "public, max-age=30, must-revalidate")
}

async fn security_txt() -> impl IntoResponse {
    text("Contact: mailto:security@heytea.dev\nPreferred-Languages: en\nCanonical: https://heytea.dev/.well-known/security.txt\n")
}

async fn ai_plugin() -> impl IntoResponse {
    (
        static_headers(),
        Json(json!({
            "schema_version": "v1",
            "name_for_human": "heytea.dev",
            "name_for_model": "heytea_dev",
            "description_for_human": "Live wait time and status for HeyTea locations.",
            "description_for_model": "Query current wait time, notices, locations, and historical wait-time data for HeyTea.",
            "auth": { "type": "none" },
            "api": { "type": "openapi", "url": "https://api.heytea.dev/openapi.json" },
            "logo_url": "https://heytea.dev/icon.svg",
            "contact_email": "hello@heytea.dev",
            "legal_info_url": "https://heytea.dev/"
        })),
    )
}

async fn agent_json() -> impl IntoResponse {
    (
        static_headers(),
        Json(json!({
            "name": "heytea.dev",
            "url": "https://heytea.dev/",
            "description": "Live wait-time status tools for HeyTea locations.",
            "version": "0.1.0",
            "capabilities": { "streaming": true, "pushNotifications": false },
            "skills": [
                {
                    "id": "get_status",
                    "name": "Get current status",
                    "description": "Fetch live wait time, catalog open state, notices, and freshness metadata."
                }
            ]
        })),
    )
}

async fn mcp_json() -> impl IntoResponse {
    (
        static_headers(),
        Json(json!({
            "name": "heytea.dev",
            "description": "Public anonymous MCP server for HeyTea location status APIs.",
            "url": "https://mcp.heytea.dev/mcp",
            "transport": "http",
            "auth": { "type": "none" }
        })),
    )
}

async fn webmcp_json() -> impl IntoResponse {
    (
        static_headers(),
        Json(json!({
            "name": "heytea.dev",
            "version": "0.1.0",
            "description": "Browser-accessible live status tools for HeyTea locations.",
            "tools": [
                {
                    "name": "get_status",
                    "description": "Get current wait time, catalog open state, notices, and freshness metadata.",
                    "inputSchema": { "type": "object", "properties": {} }
                }
            ]
        })),
    )
}

async fn openapi_proxy(State(state): State<AppState>) -> Response {
    let url = format!("{}/openapi.json", state.api_url.trim_end_matches('/'));
    match state.client.get(url).send().await {
        Ok(response) if response.status().is_success() => {
            match response.json::<serde_json::Value>().await {
                Ok(value) => (short_cache_headers(3600), Json(value)).into_response(),
                Err(error) => {
                    tracing::warn!(?error, "openapi proxy response decode failed");
                    site_error(StatusCode::BAD_GATEWAY, "openapi unavailable")
                }
            }
        }
        Ok(response) => {
            tracing::warn!(status = %response.status(), "openapi proxy failed");
            site_error(StatusCode::BAD_GATEWAY, "openapi unavailable")
        }
        Err(error) => {
            tracing::warn!(?error, "openapi proxy request failed");
            site_error(StatusCode::BAD_GATEWAY, "openapi unavailable")
        }
    }
}

async fn api_json<T>(state: &AppState, path: &str) -> Option<T>
where
    T: DeserializeOwned,
{
    api_check(state, path).await.value
}

async fn api_check<T>(state: &AppState, path: &str) -> ApiCheck<T>
where
    T: DeserializeOwned,
{
    let url = format!("{}{}", state.api_url.trim_end_matches('/'), path);
    match state.client.get(url).send().await {
        Ok(response) => {
            let value = if response.status().is_success() {
                response.json::<T>().await.ok()
            } else {
                None
            };
            ApiCheck { value }
        }
        Err(error) => {
            tracing::warn!(?error, path, "site api fetch failed");
            ApiCheck { value: None }
        }
    }
}

pub struct ApiCheck<T> {
    pub value: Option<T>,
}

async fn live_component(
    state: &AppState,
    name: &str,
    public_target: &str,
    service: &str,
    path: &str,
) -> templates::StatusComponent {
    let base_url = match service {
        "mcp" => &state.mcp_url,
        _ => &state.api_url,
    };
    let url = format!("{}{}", base_url.trim_end_matches('/'), path);
    let start = Instant::now();
    let ok = match state.client.get(url).send().await {
        Ok(response) => response.status().is_success(),
        Err(error) => {
            tracing::warn!(?error, service, path, "status live check failed");
            false
        }
    };
    let latency = start.elapsed().as_millis();
    templates::StatusComponent {
        name: name.to_string(),
        target: public_target.to_string(),
        state: if ok { "Operational" } else { "Degraded" }.to_string(),
        class_name: if ok { "ok" } else { "bad" }.to_string(),
        uptime: "live".to_string(),
        latency: format!("{latency} ms"),
    }
}

fn text(body: &'static str) -> impl IntoResponse {
    (
        typed_headers("text/plain; charset=utf-8", "public, max-age=86400"),
        body,
    )
}

fn markdown(body: &'static str) -> impl IntoResponse {
    (
        typed_headers("text/markdown; charset=utf-8", "public, max-age=86400"),
        body,
    )
}

fn xml(body: &'static str) -> impl IntoResponse {
    (
        typed_headers("application/xml; charset=utf-8", "public, max-age=86400"),
        body,
    )
}

fn svg_owned(body: String, cache_control: &str) -> impl IntoResponse {
    (
        typed_headers("image/svg+xml; charset=utf-8", cache_control),
        body,
    )
}

fn og_svg(view: &templates::DashboardView) -> String {
    let status_line = escape_xml(&view.status_line);
    let graph = if view.closed {
        String::new()
    } else {
        format!(
            r##"<rect x="72" y="320" width="1056" height="220" fill="#fff" stroke="#000" stroke-width="4"/><g transform="translate(72 320) scale(10.56 5)"><polyline points="{}" fill="none" stroke="#000" stroke-width="4" stroke-linecap="square" stroke-linejoin="miter" vector-effect="non-scaling-stroke"/></g>"##,
            escape_xml(&view.trend_points)
        )
    };

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630"><rect width="1200" height="630" fill="#fff"/><text x="72" y="118" fill="#000" font-family="Atkinson Hyperlegible,Arial,sans-serif" font-size="48">heytea.dev</text><text x="72" y="240" fill="#000" font-family="Atkinson Hyperlegible,Arial,sans-serif" font-size="72">{status_line}</text>{graph}</svg>"##
    )
}

fn escape_xml(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '&' => "&amp;".chars().collect::<Vec<_>>(),
            '<' => "&lt;".chars().collect::<Vec<_>>(),
            '>' => "&gt;".chars().collect::<Vec<_>>(),
            '"' => "&quot;".chars().collect::<Vec<_>>(),
            '\'' => "&apos;".chars().collect::<Vec<_>>(),
            _ => vec![ch],
        })
        .collect()
}

async fn not_found(headers: HeaderMap) -> Response {
    if wants_json(&headers) {
        return (
            StatusCode::NOT_FOUND,
            no_store_headers(),
            Json(json!({ "error": { "code": "NOT_FOUND", "message": "not found" } })),
        )
            .into_response();
    }
    if wants_markdown(&headers) || wants_text(&headers) || agent_user_agent(&headers) {
        return (
            StatusCode::NOT_FOUND,
            typed_headers("text/plain; charset=utf-8", "no-store"),
            "not found",
        )
            .into_response();
    }
    (
        StatusCode::NOT_FOUND,
        no_store_headers(),
        Html("<!doctype html><title>Not Found</title><h1>Not Found</h1>"),
    )
        .into_response()
}

fn wants_json(headers: &HeaderMap) -> bool {
    accepts(headers, "application/json")
}

fn wants_markdown(headers: &HeaderMap) -> bool {
    accepts(headers, "text/markdown")
}

fn wants_text(headers: &HeaderMap) -> bool {
    accepts(headers, "text/plain")
}

fn accepts(headers: &HeaderMap, needle: &str) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.contains(needle))
        .unwrap_or(false)
}

fn agent_user_agent(headers: &HeaderMap) -> bool {
    headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            let value = value.to_ascii_lowercase();
            value.contains("claudebot")
                || value.contains("gptbot")
                || value.contains("anthropic-ai")
                || value.contains("perplexitybot")
        })
        .unwrap_or(false)
}

fn evil_request(headers: &HeaderMap) -> bool {
    // RFC 3514 is below HTTP; honor edge/test headers that surface the bit.
    ["x-evil-bit", "x-rfc3514", "x-rfc3514-evil"]
        .iter()
        .any(|name| truthy_header(headers, name))
}

fn truthy_header(headers: &HeaderMap, name: &str) -> bool {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            matches!(value.as_str(), "1" | "true" | "yes" | "evil" | "set")
        })
        .unwrap_or(false)
}

fn htcpcp_method(method: &Method) -> bool {
    matches!(method.as_str(), "BREW" | "WHEN")
}

fn htcpcp_response(method: &Method, headers: &HeaderMap) -> Response {
    let mut response_headers = typed_headers("message/teapot; charset=utf-8", "no-store");
    response_headers.insert("safe", HeaderValue::from_static("yes"));
    response_headers.insert(
        "accept-additions",
        HeaderValue::from_static("milk-type, syrup-type, sweetener-type, spice-type, tea-type"),
    );
    response_headers.insert(
        "x-htcpcp-version",
        HeaderValue::from_static("RFC2324, RFC7168"),
    );

    if method.as_str() == "WHEN" {
        return (StatusCode::NO_CONTENT, response_headers).into_response();
    }

    if coffee_request(headers) {
        return (
            StatusCode::from_u16(418).expect("valid teapot status"),
            response_headers,
            "418 I'm a teapot. This appliance serves tea, not coffee.\n",
        )
            .into_response();
    }

    (
        StatusCode::OK,
        response_headers,
        "HTCPCP-TEA accepted. The requested tea is now steeping.\n",
    )
        .into_response()
}

fn coffee_request(headers: &HeaderMap) -> bool {
    [header::ACCEPT, header::CONTENT_TYPE]
        .iter()
        .filter_map(|name| headers.get(name))
        .filter_map(|value| value.to_str().ok())
        .map(str::to_ascii_lowercase)
        .any(|value| value.contains("coffee") || value.contains("message/coffeepot"))
}

fn method_not_allowed() -> (StatusCode, HeaderMap, &'static str) {
    let mut headers = no_store_headers();
    headers.insert(
        header::ALLOW,
        HeaderValue::from_static("GET, HEAD, BREW, WHEN"),
    );
    (
        StatusCode::METHOD_NOT_ALLOWED,
        headers,
        "method not allowed",
    )
}

fn html_shell_headers(stream_url: &str) -> HeaderMap {
    html_shell_headers_with_cache(stream_url, "public, max-age=30, must-revalidate")
}

fn finder_html_headers() -> HeaderMap {
    let mut headers = html_shell_headers_with_cache("", "private, no-store");
    headers.insert(
        header::VARY,
        HeaderValue::from_static("Accept, User-Agent, X-Forwarded-For"),
    );
    headers
}

fn html_shell_headers_with_cache(stream_url: &str, cache_control: &str) -> HeaderMap {
    let mut headers = typed_headers("text/html; charset=utf-8", cache_control);
    let csp = format!(
        "default-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; object-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; font-src data:; img-src data:; prefetch-src 'self'; connect-src {}",
        csp_origin(stream_url)
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_str(&csp).expect("valid content security policy"),
    );
    headers
}

fn csp_origin(url: &str) -> String {
    let Ok(url) = reqwest::Url::parse(url) else {
        return "'self'".to_string();
    };
    let Some(host) = url.host_str() else {
        return "'self'".to_string();
    };
    match url.port() {
        Some(port) => format!("{}://{}:{}", url.scheme(), host, port),
        None => format!("{}://{}", url.scheme(), host),
    }
}

fn static_headers() -> HeaderMap {
    cache_headers("public, max-age=86400")
}

fn short_cache_headers(seconds: u64) -> HeaderMap {
    cache_headers(&format!("public, max-age={seconds}, must-revalidate"))
}

fn negotiated_headers(cache_control: &str) -> HeaderMap {
    let mut headers = cache_headers(cache_control);
    headers.insert(header::VARY, HeaderValue::from_static("Accept, User-Agent"));
    headers
}

fn no_store_headers() -> HeaderMap {
    cache_headers("no-store")
}

fn typed_headers(content_type: &'static str, cache_control: &str) -> HeaderMap {
    let mut headers = cache_headers(cache_control);
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers
}

fn cache_headers(cache_control: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_str(cache_control).expect("valid cache control"),
    );
    headers
}

fn site_error(status: StatusCode, message: &'static str) -> Response {
    (
        status,
        no_store_headers(),
        Json(json!({ "error": message })),
    )
        .into_response()
}

const LLMS_TXT: &str = r#"# heytea.dev

> Live wait time, catalog open state, notices, history, and location discovery for public HeyTea locations.

## Endpoints

- `GET https://api.heytea.dev/locations` - Public locations with catalog open state and current pickup wait.
- `GET https://api.heytea.dev/locations/{slug}/status` - Current store state, wait time, notices, observed time, and staleAfter freshness.
- `GET https://api.heytea.dev/locations/{slug}/wait-time` - Current pickup, delivery, cups, and orders values.
- `GET https://api.heytea.dev/locations/{slug}/notice` - Current store notices.
- `GET https://api.heytea.dev/locations/{slug}/closing-notice` - Current closing notices.
- `GET https://api.heytea.dev/locations/{slug}/history?range=today` - One-minute points for the current same-day open session.
- `GET https://api.heytea.dev/locations/{slug}/stream` - Server-sent `status.updated` events.
- `GET https://api.heytea.dev/openapi.json` - OpenAPI schema.
- `POST https://mcp.heytea.dev/mcp` - JSON-RPC 2.0 MCP endpoint.

## Authentication

No authentication is required. The API exposes public slugs, not upstream shop IDs. There are no menu endpoints, no `/v1` prefix, and no default-location aliases.

## Freshness

Waits and notices are polled every 60 seconds. Catalog metadata, including open state, refreshes more slowly. Treat live wait data as fresh until `staleAfter`. HTTP `max-age` is based on `max(0, observedAt + pollInterval - now)`.

## Examples

```bash
curl https://api.heytea.dev/locations
curl https://api.heytea.dev/locations/downtown-metreon/status
curl 'https://api.heytea.dev/locations/downtown-metreon/history?range=today'
curl -N https://api.heytea.dev/locations/downtown-metreon/stream
```

```bash
curl https://mcp.heytea.dev/mcp \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_status","arguments":{"slug":"downtown-metreon"}}}'
```
"#;

const LLMS_FULL_TXT: &str = r#"# heytea.dev Full Context

heytea.dev is a live status dashboard and public API for public HeyTea locations.

The service discovers locations, polls safe public HeyTea app endpoints every 60 seconds, stores normalized observations in Postgres/TimescaleDB, and publishes live updates to connected browsers with Server-Sent Events. The public API never accepts or returns upstream shop IDs.

Public surfaces:

- Location finder: https://heytea.dev/
- API: https://api.heytea.dev
- Docs: https://docs.heytea.dev
- MCP: https://mcp.heytea.dev/mcp
- Status page: https://status.heytea.dev/

API endpoints:

- `GET /locations`
- `GET /locations/{slug}`
- `GET /locations/{slug}/status`
- `GET /locations/{slug}/wait-time`
- `GET /locations/{slug}/notice`
- `GET /locations/{slug}/closing-notice`
- `GET /locations/{slug}/history?range=today|1h|6h|24h|7d`
- `GET /locations/{slug}/stream`
- `GET /healthz`
- `GET /readyz`
- `GET /metrics`
- `GET /openapi.json`

MCP tools:

- `list_locations`
- `find_nearest_location`
- `get_status`
- `get_wait_time`
- `get_notice`
- `get_closing_notice`
- `get_history`

Freshness model:

Live values include `observedAt`, `stale`, and `staleAfter` when applicable. Data is expected to expire at the next poll boundary: `ttl_seconds = max(0, observedAt + poll_interval - now)`.

Restrictions:

- No public upstream store IDs.
- No menu endpoints.
- No payment, authentication bypass, attestation bypass, or rate-limit bypass.
"#;

const AGENTS_TXT: &str = r#"# Agent Access Policy for heytea.dev

AI agents may read the dashboard, API docs, llms.txt, llms-full.txt, OpenAPI schema, and anonymous public API endpoints.

Agents should use `staleAfter` and cache headers to avoid unnecessary refetches. Agents must not attempt to bypass upstream HeyTea authentication, payment, attestation, CAPTCHA, pinning, or rate limits.
"#;

const SKILL_MD: &str = r#"---
name: heytea-status
description: Query live wait time, catalog open state, notices, locations, and recent wait-time history for HeyTea.
---

# heytea-status

Use this skill when a user asks about the current status or wait time for HeyTea locations.

Call `GET https://api.heytea.dev/locations` to discover slugs. Use `GET https://api.heytea.dev/locations/{slug}/status` for current state and `GET https://api.heytea.dev/locations/{slug}/history?range=today` for the current-day trend. Use the MCP `list_locations`, `get_status`, or `get_history` tools when MCP is available.
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn htcpcp_brews_tea() {
        let response = htcpcp_response(&Method::from_bytes(b"BREW").unwrap(), &HeaderMap::new());

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "message/teapot; charset=utf-8"
        );
        assert_eq!(
            response.headers().get("x-htcpcp-version").unwrap(),
            "RFC2324, RFC7168"
        );
    }

    #[test]
    fn htcpcp_refuses_coffee() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            HeaderValue::from_static("message/coffeepot"),
        );

        let response = htcpcp_response(&Method::from_bytes(b"BREW").unwrap(), &headers);

        assert_eq!(response.status(), StatusCode::from_u16(418).unwrap());
    }

    #[test]
    fn htcpcp_when_stops_additions() {
        let response = htcpcp_response(&Method::from_bytes(b"WHEN").unwrap(), &HeaderMap::new());

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(response.headers().get("safe").unwrap(), "yes");
    }

    #[test]
    fn htcpcp_methods_are_custom() {
        assert!(htcpcp_method(&Method::from_bytes(b"BREW").unwrap()));
        assert!(htcpcp_method(&Method::from_bytes(b"WHEN").unwrap()));
        assert!(!htcpcp_method(&Method::GET));
    }
}
