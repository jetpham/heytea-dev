use axum::{http::StatusCode, response::IntoResponse};

#[derive(Debug)]
pub struct SiteError(anyhow::Error);

impl<E> From<E> for SiteError
where
    E: Into<anyhow::Error>,
{
    fn from(error: E) -> Self {
        Self(error.into())
    }
}

impl IntoResponse for SiteError {
    fn into_response(self) -> axum::response::Response {
        tracing::error!(error = ?self.0, "site render error");
        (StatusCode::INTERNAL_SERVER_ERROR, "internal site error").into_response()
    }
}
