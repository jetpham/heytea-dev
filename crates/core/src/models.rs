use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use utoipa::{IntoParams, ToSchema};

pub const DEFAULT_POLL_INTERVAL_SECONDS: i64 = 60;
pub const DEFAULT_STALE_AFTER_SECONDS: i64 = DEFAULT_POLL_INTERVAL_SECONDS;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LocationsResponse {
    pub generated_at: DateTime<Utc>,
    pub locations: Vec<LocationResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LocationResponse {
    pub slug: String,
    pub name: String,
    pub address: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub timezone: String,
    pub is_enabled: Option<bool>,
    pub support_takeaway: Option<bool>,
    pub is_open: Option<bool>,
    pub pickup_wait_minutes: Option<i32>,
    pub observed_at: Option<DateTime<Utc>>,
    pub stale: bool,
    pub stale_after: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct LocationPath {
    /// Stable public location slug, such as downtown-metreon.
    pub slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    pub name: String,
    pub address: String,
    pub is_open: Option<bool>,
    pub pickup_wait_minutes: Option<i32>,
    pub delivery_estimate_minutes: Option<i32>,
    pub making_cups: Option<i32>,
    pub making_orders: Option<i32>,
    pub is_estimate: Option<bool>,
    pub text: Option<String>,
    pub notices: Vec<String>,
    pub closing_notices: Vec<String>,
    pub observed_at: DateTime<Utc>,
    pub stale: bool,
    pub stale_after: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WaitTimeResponse {
    pub pickup_wait_minutes: Option<i32>,
    pub delivery_estimate_minutes: Option<i32>,
    pub making_cups: Option<i32>,
    pub making_orders: Option<i32>,
    pub is_estimate: Option<bool>,
    pub text: Option<String>,
    pub observed_at: DateTime<Utc>,
    pub stale: bool,
    pub stale_after: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NoticeResponse {
    pub notices: Vec<String>,
    pub observed_at: Option<DateTime<Utc>>,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClosingNoticeResponse {
    pub closing_notices: Vec<String>,
    pub observed_at: Option<DateTime<Utc>>,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub ok: bool,
    pub service: String,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReadyResponse {
    pub ok: bool,
    pub database: bool,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HistoryQuery {
    /// Lookback range. Supported values: today, 1h, 6h, 24h, 7d.
    pub range: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HistoryResponse {
    pub range: String,
    pub generated_at: DateTime<Utc>,
    pub points: Vec<HistoryPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPoint {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub avg_pickup_wait_minutes: Option<f64>,
    pub min_pickup_wait_minutes: Option<i32>,
    pub max_pickup_wait_minutes: Option<i32>,
    pub avg_delivery_estimate_minutes: Option<f64>,
    pub avg_making_cups: Option<f64>,
    pub avg_making_orders: Option<f64>,
    pub sample_count: i64,
}

#[derive(Debug, Clone, Copy)]
pub enum HistoryRange {
    Today,
    OneHour,
    SixHours,
    OneDay,
    SevenDays,
}

impl HistoryRange {
    pub fn sql_interval(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::OneHour => "1 hour",
            Self::SixHours => "6 hours",
            Self::OneDay => "24 hours",
            Self::SevenDays => "7 days",
        }
    }
}

impl fmt::Display for HistoryRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Today => "today",
            Self::OneHour => "1h",
            Self::SixHours => "6h",
            Self::OneDay => "24h",
            Self::SevenDays => "7d",
        })
    }
}

impl FromStr for HistoryRange {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "today" => Ok(Self::Today),
            "1h" => Ok(Self::OneHour),
            "6h" => Ok(Self::SixHours),
            "24h" => Ok(Self::OneDay),
            "7d" => Ok(Self::SevenDays),
            _ => Err("unsupported range"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorBody {
    pub error: ApiErrorDetail,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiErrorCode {
    NotReady,
    NotFound,
    InvalidRequest,
    RateLimited,
    UpstreamUnavailable,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorDetail {
    pub code: ApiErrorCode,
    pub message: String,
    pub retry_after_seconds: Option<u64>,
}
