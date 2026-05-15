use crate::{error::SiteError, templates, AppState};
use askama::Template;
use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use heytea_core::{HistoryResponse, ReadyResponse, StatusResponse};
use serde::de::DeserializeOwned;
use serde_json::json;
use tower_http::services::{ServeDir, ServeFile};

pub fn router(state: AppState) -> Router {
    let asset_root = state.assets.root();

    Router::new()
        .route("/", get(dashboard))
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
        .fallback(not_found)
        .with_state(state)
}

async fn dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, SiteError> {
    if wants_json(&headers) {
        return Ok((
            negotiated_headers("public, max-age=60, must-revalidate"),
            Json(json!({
                "name": "heytea.dev",
                "description": "Live wait time and status for HeyTea Downtown Metreon.",
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

    let (status, history) = tokio::join!(
        api_json::<StatusResponse>(&state, "/status"),
        api_json::<HistoryResponse>(&state, "/history?range=today")
    );
    let template = templates::DashboardTemplate::new(
        status,
        history,
        format!("{}/stream", state.public_api_url.trim_end_matches('/')),
    );
    Ok((html_shell_headers(), Html(template.render()?)).into_response())
}

async fn status(State(state): State<AppState>) -> Result<Response, SiteError> {
    let (ready, status) = tokio::join!(
        api_check::<ReadyResponse>(&state, "/readyz"),
        api_check::<StatusResponse>(&state, "/status")
    );
    let template =
        templates::StatusTemplate::new(ready, status, state.assets.tags("src/status.ts", false));
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
            "description": "Live wait time and status for HeyTea Downtown Metreon.",
            "start_url": "/",
            "display": "standalone",
            "background_color": "#09100c",
            "theme_color": "#101510",
            "icons": [{ "src": "/icon.svg", "sizes": "any", "type": "image/svg+xml" }]
        })),
    )
}

async fn icon() -> impl IntoResponse {
    svg(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><rect width="128" height="128" rx="28" fill="#09100c"/><circle cx="64" cy="64" r="42" fill="#80e36a" opacity=".18"/><path d="M35 47h58l-8 48H43L35 47Z" fill="#f3faef"/><path d="M45 34h38" stroke="#80e36a" stroke-width="10" stroke-linecap="round"/><path d="M50 60h28" stroke="#09100c" stroke-width="7" stroke-linecap="round"/></svg>"##,
    )
}

async fn og_image() -> impl IntoResponse {
    svg(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 630"><rect width="1200" height="630" fill="#09100c"/><circle cx="940" cy="120" r="280" fill="#80e36a" opacity=".14"/><text x="72" y="160" fill="#80e36a" font-family="Atkinson Hyperlegible,Arial,sans-serif" font-size="34" letter-spacing="8">HEYTEA.DEV</text><text x="72" y="315" fill="#f3faef" font-family="Atkinson Hyperlegible,Arial,sans-serif" font-size="96" font-weight="700">Downtown Metreon</text><text x="72" y="430" fill="#f3faef" font-family="Atkinson Hyperlegible,Arial,sans-serif" font-size="96" font-weight="700">wait time</text><text x="76" y="520" fill="#a7b49e" font-family="Atkinson Hyperlegible,Arial,sans-serif" font-size="38">Live status for 165 4th St, San Francisco</text></svg>"##,
    )
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
            "description_for_human": "Live wait time and status for HeyTea Downtown Metreon.",
            "description_for_model": "Query current wait time, notices, and historical wait-time data for HeyTea Downtown Metreon.",
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
            "description": "Live wait-time status tools for HeyTea Downtown Metreon.",
            "version": "0.1.0",
            "capabilities": { "streaming": true, "pushNotifications": false },
            "skills": [
                {
                    "id": "get_status",
                    "name": "Get current status",
                    "description": "Fetch live wait time, open state, notices, and freshness metadata."
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
            "description": "Public anonymous MCP server for the singleton HeyTea Downtown Metreon status API.",
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
            "description": "Browser-accessible live status tools for HeyTea Downtown Metreon.",
            "tools": [
                {
                    "name": "get_status",
                    "description": "Get current wait time, open state, notices, and freshness metadata.",
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
            let ok = response.status().is_success();
            let value = response.json::<T>().await.ok();
            ApiCheck { ok, value }
        }
        Err(error) => {
            tracing::warn!(?error, path, "site api fetch failed");
            ApiCheck {
                ok: false,
                value: None,
            }
        }
    }
}

pub struct ApiCheck<T> {
    pub ok: bool,
    pub value: Option<T>,
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

fn svg(body: &'static str) -> impl IntoResponse {
    (
        typed_headers(
            "image/svg+xml; charset=utf-8",
            "public, max-age=31536000, immutable",
        ),
        body,
    )
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

fn html_shell_headers() -> HeaderMap {
    typed_headers(
        "text/html; charset=utf-8",
        "public, max-age=30, must-revalidate",
    )
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

> Live wait time, open state, notices, and history for the singleton HeyTea Downtown Metreon location at 165 4th St, San Francisco.

## Endpoints

- `GET https://api.heytea.dev/status` - Current store state, wait time, notices, observed time, and staleAfter freshness.
- `GET https://api.heytea.dev/wait-time` - Current pickup, delivery, cups, and orders values.
- `GET https://api.heytea.dev/notice` - Current store notice.
- `GET https://api.heytea.dev/closing-notice` - Current closing notice.
- `GET https://api.heytea.dev/history?range=today` - One-minute points for the current same-day open session.
- `GET https://api.heytea.dev/stream` - Server-sent `status.updated` events.
- `GET https://api.heytea.dev/openapi.json` - OpenAPI schema.
- `POST https://mcp.heytea.dev/mcp` - JSON-RPC 2.0 MCP endpoint.

## Authentication

No authentication is required. The API is singleton-shaped: no shop IDs, no location endpoints, no menu endpoints, and no `/v1` prefix.

## Freshness

Data is polled every 60 seconds. Treat live data as fresh until `staleAfter`. HTTP `max-age` is based on `max(0, observedAt + pollInterval - now)`.

## Examples

```bash
curl https://api.heytea.dev/status
curl 'https://api.heytea.dev/history?range=today'
curl -N https://api.heytea.dev/stream
```

```bash
curl https://mcp.heytea.dev/mcp \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_status","arguments":{}}}'
```
"#;

const LLMS_FULL_TXT: &str = r#"# heytea.dev Full Context

heytea.dev is a live singleton status dashboard and public API for HeyTea Downtown Metreon at 165 4th St, San Francisco.

The service polls safe public HeyTea app endpoints every 60 seconds, stores normalized observations in Postgres/TimescaleDB, and publishes live updates to connected browsers with Server-Sent Events. The public API never accepts or returns upstream shop IDs.

Public surfaces:

- Dashboard: https://heytea.dev/
- API: https://api.heytea.dev
- Docs: https://docs.heytea.dev
- MCP: https://mcp.heytea.dev/mcp
- Status page: https://status.heytea.dev/

API endpoints:

- `GET /status`
- `GET /wait-time`
- `GET /notice`
- `GET /closing-notice`
- `GET /history?range=today|1h|6h|24h|7d|30d|1y`
- `GET /stream`
- `GET /healthz`
- `GET /readyz`
- `GET /metrics`
- `GET /openapi.json`

MCP tools:

- `get_status`
- `get_wait_time`
- `get_notice`
- `get_closing_notice`
- `get_history`

Freshness model:

Live values include `observedAt`, `stale`, and `staleAfter` when applicable. Data is expected to expire at the next poll boundary: `ttl_seconds = max(0, observedAt + poll_interval - now)`.

Restrictions:

- No public store IDs.
- No public shop list or location search.
- No menu endpoints.
- No payment, authentication bypass, attestation bypass, or rate-limit bypass.
"#;

const AGENTS_TXT: &str = r#"# Agent Access Policy for heytea.dev

AI agents may read the dashboard, API docs, llms.txt, llms-full.txt, OpenAPI schema, and anonymous public API endpoints.

Agents should use `staleAfter` and cache headers to avoid unnecessary refetches. Agents must not attempt to bypass upstream HeyTea authentication, payment, attestation, CAPTCHA, pinning, or rate limits.
"#;

const SKILL_MD: &str = r#"---
name: heytea-status
description: Query live wait time, open state, notices, and recent wait-time history for HeyTea Downtown Metreon.
---

# heytea-status

Use this skill when a user asks about the current status or wait time for HeyTea Downtown Metreon in San Francisco.

Call `GET https://api.heytea.dev/status` for the current state. Use `GET https://api.heytea.dev/history?range=today` for the current open-session trend. Use the MCP `get_status` or `get_history` tools when MCP is available.
"#;
