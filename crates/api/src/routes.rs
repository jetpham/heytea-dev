use crate::{db, error::ApiError, openapi::ApiDoc, AppState};
use axum::response::sse::{Event, KeepAlive};
use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue},
    response::IntoResponse,
    response::Sse,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use heytea_core::{
    ApiErrorBody, ClosingNoticeResponse, HealthResponse, HistoryQuery, HistoryRange,
    HistoryResponse, NoticeResponse, ReadyResponse, StatusResponse, WaitTimeResponse,
};
use std::{convert::Infallible, time::Duration};
use tokio::sync::broadcast;
use utoipa::OpenApi;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/status", get(status))
        .route("/wait-time", get(wait_time))
        .route("/notice", get(notice))
        .route("/closing-notice", get(closing_notice))
        .route("/history", get(history))
        .route("/stream", get(stream))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/openapi.json", get(openapi_json))
        .fallback(not_found)
        .with_state(state)
}

#[utoipa::path(get, path = "/status", responses((status = 200, body = StatusResponse), (status = 503, body = ApiErrorBody)))]
pub(crate) async fn status(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let status = db::status(&state.pool).await?;
    Ok((freshness_headers(status.stale_after), Json(status)))
}

#[utoipa::path(get, path = "/wait-time", responses((status = 200, body = WaitTimeResponse), (status = 503, body = ApiErrorBody)))]
pub(crate) async fn wait_time(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let wait_time = db::wait_time(&state.pool).await?;
    Ok((freshness_headers(wait_time.stale_after), Json(wait_time)))
}

#[utoipa::path(get, path = "/notice", responses((status = 200, body = NoticeResponse), (status = 503, body = ApiErrorBody)))]
pub(crate) async fn notice(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let notice = db::notice(&state.pool).await?;
    let stale_after = notice
        .observed_at
        .map(db::stale_after)
        .unwrap_or_else(Utc::now);
    Ok((freshness_headers(stale_after), Json(notice)))
}

#[utoipa::path(get, path = "/closing-notice", responses((status = 200, body = ClosingNoticeResponse), (status = 503, body = ApiErrorBody)))]
pub(crate) async fn closing_notice(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let closing_notice = db::closing_notice(&state.pool).await?;
    let stale_after = closing_notice
        .observed_at
        .map(db::stale_after)
        .unwrap_or_else(Utc::now);
    Ok((freshness_headers(stale_after), Json(closing_notice)))
}

#[utoipa::path(get, path = "/history", params(HistoryQuery), responses((status = 200, body = HistoryResponse), (status = 400, body = ApiErrorBody)))]
pub(crate) async fn history(
    State(state): State<AppState>,
    Query(query): Query<HistoryQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let range = query
        .range
        .as_deref()
        .unwrap_or("24h")
        .parse::<HistoryRange>()
        .map_err(ApiError::invalid)?;
    let history = db::history(&state.pool, range).await?;
    let stale_after = db::status(&state.pool)
        .await
        .map(|status| status.stale_after)
        .unwrap_or_else(|_| Utc::now());
    Ok((freshness_headers(stale_after), Json(history)))
}

#[utoipa::path(get, path = "/stream", responses((status = 200, description = "Server-sent status events")))]
pub(crate) async fn stream(State(state): State<AppState>) -> impl IntoResponse {
    let initial = db::status(&state.pool).await.ok();
    let mut receiver = state.status_events.subscribe();
    let pool = state.pool.clone();
    let stream = async_stream::stream! {
        if let Some(status) = initial {
            yield Ok::<Event, Infallible>(status_event(status));
        }

        loop {
            match receiver.recv().await {
                Ok(status) => yield Ok::<Event, Infallible>(status_event(status)),
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "status stream receiver lagged");
                    if let Ok(status) = db::status(&pool).await {
                        yield Ok::<Event, Infallible>(status_event(status));
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    (
        stream_headers(),
        Sse::new(stream).keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        ),
    )
}

#[utoipa::path(get, path = "/healthz", responses((status = 200, body = HealthResponse)))]
pub(crate) async fn healthz() -> impl IntoResponse {
    let response = HealthResponse {
        ok: true,
        service: "heytea-api".to_string(),
        checked_at: Utc::now(),
    };
    (no_store_headers(), Json(response))
}

#[utoipa::path(get, path = "/readyz", responses((status = 200, body = ReadyResponse), (status = 503, body = ReadyResponse)))]
pub(crate) async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    let database = sqlx::query_scalar::<_, i32>("select 1")
        .fetch_one(&state.pool)
        .await
        .is_ok()
        && db::schema_ready(&state.pool).await;

    let response = ReadyResponse {
        ok: database,
        database,
        checked_at: Utc::now(),
    };
    (no_store_headers(), Json(response))
}

async fn metrics() -> &'static str {
    "# HELP heytea_api_up Whether the heytea API process is up\n# TYPE heytea_api_up gauge\nheytea_api_up 1\n"
}

async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

async fn not_found() -> ApiError {
    ApiError::not_found("endpoint not found")
}

fn status_event(status: StatusResponse) -> Event {
    Event::default()
        .event("status.updated")
        .data(serde_json::to_string(&status).unwrap_or_else(|_| "{}".to_string()))
}

fn freshness_headers(stale_after: DateTime<Utc>) -> HeaderMap {
    let ttl = db::ttl_seconds(stale_after);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_str(&format!("public, max-age={ttl}, must-revalidate"))
            .expect("valid cache header"),
    );
    headers.insert(
        "x-data-ttl-seconds",
        HeaderValue::from_str(&ttl.to_string()).expect("valid ttl header"),
    );
    headers.insert(
        header::EXPIRES,
        HeaderValue::from_str(&stale_after.to_rfc2822()).expect("valid expires header"),
    );
    headers
}

fn no_store_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers
}

fn stream_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers
}
