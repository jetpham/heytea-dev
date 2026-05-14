use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use utoipa::{IntoParams, ToSchema};

pub const DEFAULT_STALE_AFTER_SECONDS: i64 = 90;

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
    pub notice: Option<String>,
    pub closing_notice: Option<String>,
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
    pub observed_at: DateTime<Utc>,
    pub stale: bool,
    pub stale_after: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NoticeResponse {
    pub notice: Option<String>,
    pub observed_at: Option<DateTime<Utc>>,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClosingNoticeResponse {
    pub closing_notice: Option<String>,
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
    /// Lookback range. Supported values: 1h, 6h, 24h, 7d, 30d, 1y.
    pub range: Option<String>,
    /// Chart resolution. Supported values: 1m, 5m, 15m, 1h, 1d.
    pub bucket: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HistoryResponse {
    pub range: String,
    pub bucket: String,
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
    OneHour,
    SixHours,
    OneDay,
    SevenDays,
    ThirtyDays,
    OneYear,
}

impl HistoryRange {
    pub fn sql_interval(self) -> &'static str {
        match self {
            Self::OneHour => "1 hour",
            Self::SixHours => "6 hours",
            Self::OneDay => "24 hours",
            Self::SevenDays => "7 days",
            Self::ThirtyDays => "30 days",
            Self::OneYear => "1 year",
        }
    }
}

impl fmt::Display for HistoryRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OneHour => "1h",
            Self::SixHours => "6h",
            Self::OneDay => "24h",
            Self::SevenDays => "7d",
            Self::ThirtyDays => "30d",
            Self::OneYear => "1y",
        })
    }
}

impl FromStr for HistoryRange {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "1h" => Ok(Self::OneHour),
            "6h" => Ok(Self::SixHours),
            "24h" => Ok(Self::OneDay),
            "7d" => Ok(Self::SevenDays),
            "30d" => Ok(Self::ThirtyDays),
            "1y" => Ok(Self::OneYear),
            _ => Err("unsupported range"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum HistoryBucket {
    OneMinute,
    FiveMinutes,
    FifteenMinutes,
    OneHour,
    OneDay,
}

impl HistoryBucket {
    pub fn sql_interval(self) -> &'static str {
        match self {
            Self::OneMinute => "1 minute",
            Self::FiveMinutes => "5 minutes",
            Self::FifteenMinutes => "15 minutes",
            Self::OneHour => "1 hour",
            Self::OneDay => "1 day",
        }
    }
}

impl fmt::Display for HistoryBucket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OneMinute => "1m",
            Self::FiveMinutes => "5m",
            Self::FifteenMinutes => "15m",
            Self::OneHour => "1h",
            Self::OneDay => "1d",
        })
    }
}

impl FromStr for HistoryBucket {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "1m" => Ok(Self::OneMinute),
            "5m" => Ok(Self::FiveMinutes),
            "15m" => Ok(Self::FifteenMinutes),
            "1h" => Ok(Self::OneHour),
            "1d" => Ok(Self::OneDay),
            _ => Err("unsupported bucket"),
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
