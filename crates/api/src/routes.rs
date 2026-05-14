use crate::{db, error::ApiError, openapi::ApiDoc, AppState};
use axum::response::sse::{Event, KeepAlive};
use axum::{
    extract::{Query, State},
    response::{Html, Sse},
    routing::get,
    Json, Router,
};
use chrono::Utc;
use heytea_core::{
    ApiErrorBody, ClosingNoticeResponse, HealthResponse, HistoryBucket, HistoryQuery, HistoryRange,
    HistoryResponse, NoticeResponse, ReadyResponse, StatusResponse, WaitTimeResponse,
};
use std::{convert::Infallible, str::FromStr, time::Duration};
use tokio_stream::{wrappers::IntervalStream, StreamExt};
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
        .route("/docs", get(docs))
        .with_state(state)
}

#[utoipa::path(get, path = "/status", responses((status = 200, body = StatusResponse), (status = 503, body = ApiErrorBody)))]
pub(crate) async fn status(
    State(state): State<AppState>,
) -> Result<Json<heytea_core::StatusResponse>, ApiError> {
    Ok(Json(db::status(&state.pool).await?))
}

#[utoipa::path(get, path = "/wait-time", responses((status = 200, body = WaitTimeResponse), (status = 503, body = ApiErrorBody)))]
pub(crate) async fn wait_time(
    State(state): State<AppState>,
) -> Result<Json<heytea_core::WaitTimeResponse>, ApiError> {
    Ok(Json(db::wait_time(&state.pool).await?))
}

#[utoipa::path(get, path = "/notice", responses((status = 200, body = NoticeResponse), (status = 503, body = ApiErrorBody)))]
pub(crate) async fn notice(
    State(state): State<AppState>,
) -> Result<Json<heytea_core::NoticeResponse>, ApiError> {
    Ok(Json(db::notice(&state.pool).await?))
}

#[utoipa::path(get, path = "/closing-notice", responses((status = 200, body = ClosingNoticeResponse), (status = 503, body = ApiErrorBody)))]
pub(crate) async fn closing_notice(
    State(state): State<AppState>,
) -> Result<Json<heytea_core::ClosingNoticeResponse>, ApiError> {
    Ok(Json(db::closing_notice(&state.pool).await?))
}

#[utoipa::path(get, path = "/history", params(HistoryQuery), responses((status = 200, body = HistoryResponse), (status = 400, body = ApiErrorBody)))]
pub(crate) async fn history(
    State(state): State<AppState>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<heytea_core::HistoryResponse>, ApiError> {
    let range = query
        .range
        .as_deref()
        .unwrap_or("24h")
        .parse::<HistoryRange>()
        .map_err(ApiError::invalid)?;
    let bucket = query
        .bucket
        .as_deref()
        .map(HistoryBucket::from_str)
        .transpose()
        .map_err(ApiError::invalid)?
        .unwrap_or(match range {
            HistoryRange::OneHour => HistoryBucket::OneMinute,
            HistoryRange::SixHours | HistoryRange::OneDay => HistoryBucket::FiveMinutes,
            HistoryRange::SevenDays => HistoryBucket::FifteenMinutes,
            HistoryRange::ThirtyDays => HistoryBucket::OneHour,
            HistoryRange::OneYear => HistoryBucket::OneDay,
        });

    Ok(Json(db::history(&state.pool, range, bucket).await?))
}

#[utoipa::path(get, path = "/stream", responses((status = 200, description = "Server-sent status events")))]
pub(crate) async fn stream(
    State(state): State<AppState>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let ticks = IntervalStream::new(tokio::time::interval(Duration::from_secs(15)));
    let stream = ticks.then(move |_| {
        let state = state.clone();
        async move {
            match db::status(&state.pool).await {
                Ok(status) => Ok(Event::default()
                    .event("status.updated")
                    .data(serde_json::to_string(&status).unwrap_or_else(|_| "{}".to_string()))),
                Err(_) => Ok(Event::default()
                    .event("heartbeat")
                    .data(format!(r#"{{"at":"{}"}}"#, Utc::now().to_rfc3339()))),
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[utoipa::path(get, path = "/healthz", responses((status = 200, body = HealthResponse)))]
pub(crate) async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        service: "heytea-api".to_string(),
        checked_at: Utc::now(),
    })
}

#[utoipa::path(get, path = "/readyz", responses((status = 200, body = ReadyResponse), (status = 503, body = ReadyResponse)))]
pub(crate) async fn readyz(State(state): State<AppState>) -> Json<ReadyResponse> {
    let database = sqlx::query_scalar::<_, i32>("select 1")
        .fetch_one(&state.pool)
        .await
        .is_ok();

    Json(ReadyResponse {
        ok: database,
        database,
        checked_at: Utc::now(),
    })
}

async fn metrics() -> &'static str {
    "# HELP heytea_api_up Whether the heytea API process is up\n# TYPE heytea_api_up gauge\nheytea_api_up 1\n"
}

async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

async fn docs() -> Html<&'static str> {
    Html(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>heytea.dev API Docs</title>
  </head>
  <body>
    <script id="api-reference" data-url="/openapi.json"></script>
    <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
  </body>
</html>"#,
    )
}
