use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "heytea.dev API",
        version = "0.1.0",
        description = "Singleton API for the HeyTea Downtown Metreon wait-time dashboard."
    ),
    paths(
        crate::routes::status,
        crate::routes::wait_time,
        crate::routes::notice,
        crate::routes::closing_notice,
        crate::routes::history,
        crate::routes::stream,
        crate::routes::healthz,
        crate::routes::readyz,
    ),
    components(schemas(
        heytea_core::StatusResponse,
        heytea_core::WaitTimeResponse,
        heytea_core::NoticeResponse,
        heytea_core::ClosingNoticeResponse,
        heytea_core::HistoryResponse,
        heytea_core::HistoryPoint,
        heytea_core::HealthResponse,
        heytea_core::ReadyResponse,
        heytea_core::ApiErrorBody,
        heytea_core::ApiErrorDetail,
        heytea_core::ApiErrorCode,
    )),
    servers(
        (url = "https://api.heytea.dev", description = "Production")
    )
)]
pub struct ApiDoc;
