use crate::{db, error::ApiError, openapi::ApiDoc, AppState};
use axum::response::sse::{Event, KeepAlive};
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    response::Sse,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use heytea_core::{
    ApiErrorBody, ClosingNoticeResponse, HealthResponse, HistoryQuery, HistoryRange,
    HistoryResponse, LocationPath, LocationResponse, LocationsResponse, NoticeResponse,
    ReadyResponse, StatusResponse, WaitTimeResponse,
};
use std::{convert::Infallible, time::Duration};
use tokio::sync::broadcast;
use utoipa::OpenApi;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/locations", get(locations))
        .route("/locations/:slug", get(location))
        .route("/locations/:slug/status", get(location_status))
        .route("/locations/:slug/wait-time", get(location_wait_time))
        .route("/locations/:slug/notice", get(location_notice))
        .route(
            "/locations/:slug/closing-notice",
            get(location_closing_notice),
        )
        .route("/locations/:slug/history", get(location_history))
        .route("/locations/:slug/stream", get(location_stream))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/openapi.json", get(openapi_json))
        .fallback(not_found)
        .with_state(state)
}

#[utoipa::path(get, path = "/locations", responses((status = 200, body = LocationsResponse), (status = 503, body = ApiErrorBody)))]
pub(crate) async fn locations(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    Ok((
        freshness_headers(Utc::now() + chrono::TimeDelta::seconds(30)),
        Json(db::locations(&state.pool).await?),
    ))
}

#[utoipa::path(get, path = "/locations/{slug}", params(LocationPath), responses((status = 200, body = LocationResponse), (status = 404, body = ApiErrorBody)))]
pub(crate) async fn location(
    State(state): State<AppState>,
    Path(path): Path<LocationPath>,
) -> Result<impl IntoResponse, ApiError> {
    let location = db::location(&state.pool, &path.slug).await?;
    let stale_after = location.stale_after.unwrap_or_else(Utc::now);
    Ok((freshness_headers(stale_after), Json(location)))
}

#[utoipa::path(get, path = "/locations/{slug}/status", params(LocationPath), responses((status = 200, body = StatusResponse), (status = 404, body = ApiErrorBody), (status = 503, body = ApiErrorBody)))]
pub(crate) async fn location_status(
    State(state): State<AppState>,
    Path(path): Path<LocationPath>,
) -> Result<impl IntoResponse, ApiError> {
    let status = db::status_for_slug(&state.pool, &path.slug).await?;
    Ok((freshness_headers(status.stale_after), Json(status)))
}

#[utoipa::path(get, path = "/locations/{slug}/wait-time", params(LocationPath), responses((status = 200, body = WaitTimeResponse), (status = 404, body = ApiErrorBody), (status = 503, body = ApiErrorBody)))]
pub(crate) async fn location_wait_time(
    State(state): State<AppState>,
    Path(path): Path<LocationPath>,
) -> Result<impl IntoResponse, ApiError> {
    let wait_time = db::wait_time_for_slug(&state.pool, &path.slug).await?;
    Ok((freshness_headers(wait_time.stale_after), Json(wait_time)))
}

#[utoipa::path(get, path = "/locations/{slug}/notice", params(LocationPath), responses((status = 200, body = NoticeResponse), (status = 404, body = ApiErrorBody), (status = 503, body = ApiErrorBody)))]
pub(crate) async fn location_notice(
    State(state): State<AppState>,
    Path(path): Path<LocationPath>,
) -> Result<impl IntoResponse, ApiError> {
    let notice = db::notice_for_slug(&state.pool, &path.slug).await?;
    let stale_after = notice
        .observed_at
        .map(db::stale_after)
        .unwrap_or_else(Utc::now);
    Ok((freshness_headers(stale_after), Json(notice)))
}

#[utoipa::path(get, path = "/locations/{slug}/closing-notice", params(LocationPath), responses((status = 200, body = ClosingNoticeResponse), (status = 404, body = ApiErrorBody), (status = 503, body = ApiErrorBody)))]
pub(crate) async fn location_closing_notice(
    State(state): State<AppState>,
    Path(path): Path<LocationPath>,
) -> Result<impl IntoResponse, ApiError> {
    let closing_notice = db::closing_notice_for_slug(&state.pool, &path.slug).await?;
    let stale_after = closing_notice
        .observed_at
        .map(db::stale_after)
        .unwrap_or_else(Utc::now);
    Ok((freshness_headers(stale_after), Json(closing_notice)))
}

#[utoipa::path(get, path = "/locations/{slug}/history", params(LocationPath, HistoryQuery), responses((status = 200, body = HistoryResponse), (status = 400, body = ApiErrorBody), (status = 404, body = ApiErrorBody)))]
pub(crate) async fn location_history(
    State(state): State<AppState>,
    Path(path): Path<LocationPath>,
    Query(query): Query<HistoryQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let range = query
        .range
        .as_deref()
        .unwrap_or("24h")
        .parse::<HistoryRange>()
        .map_err(ApiError::invalid)?;
    let history = db::history_for_slug(&state.pool, &path.slug, range).await?;
    let stale_after = db::status_for_slug(&state.pool, &path.slug)
        .await
        .map(|status| status.stale_after)
        .unwrap_or_else(|_| Utc::now());
    Ok((freshness_headers(stale_after), Json(history)))
}

#[utoipa::path(get, path = "/locations/{slug}/stream", params(LocationPath), responses((status = 200, description = "Server-sent status events")))]
pub(crate) async fn location_stream(
    State(state): State<AppState>,
    Path(path): Path<LocationPath>,
) -> Result<impl IntoResponse, ApiError> {
    db::location(&state.pool, &path.slug).await?;
    Ok(stream_for_slug(state, path.slug).await)
}

async fn stream_for_slug(state: AppState, slug: String) -> impl IntoResponse {
    let initial = db::status_for_slug(&state.pool, &slug).await.ok();
    let mut receiver = state.status_events.subscribe();
    let pool = state.pool.clone();
    let stream = async_stream::stream! {
        if let Some(status) = initial {
            yield Ok::<Event, Infallible>(status_event(status));
        }

        loop {
            match receiver.recv().await {
                Ok(_) => {
                    if let Ok(status) = db::status_for_slug(&pool, &slug).await {
                        yield Ok::<Event, Infallible>(status_event(status));
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "status stream receiver lagged");
                    if let Ok(status) = db::status_for_slug(&pool, &slug).await {
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
    let status = if database {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, no_store_headers(), Json(response))
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
