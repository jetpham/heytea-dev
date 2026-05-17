//! Rust SDK for the public `heytea.dev` API.

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone)]
pub struct Client {
    base_url: Url,
    http: reqwest::Client,
}

impl Client {
    pub fn new(base_url: impl AsRef<str>) -> anyhow::Result<Self> {
        let base_url = Url::parse(base_url.as_ref()).context("invalid API base URL")?;
        Ok(Self {
            base_url,
            http: reqwest::Client::new(),
        })
    }

    pub fn production() -> anyhow::Result<Self> {
        Self::new("https://api.heytea.dev")
    }

    pub async fn status(&self) -> anyhow::Result<StatusResponse> {
        self.get("status").await
    }

    pub async fn locations(&self) -> anyhow::Result<LocationsResponse> {
        self.get("locations").await
    }

    pub async fn location(&self, slug: &str) -> anyhow::Result<LocationResponse> {
        self.get(&format!("locations/{slug}")).await
    }

    pub async fn status_for_location(&self, slug: &str) -> anyhow::Result<StatusResponse> {
        self.get(&format!("locations/{slug}/status")).await
    }

    pub async fn wait_time(&self) -> anyhow::Result<WaitTimeResponse> {
        self.get("wait-time").await
    }

    pub async fn wait_time_for_location(&self, slug: &str) -> anyhow::Result<WaitTimeResponse> {
        self.get(&format!("locations/{slug}/wait-time")).await
    }

    pub async fn notice(&self) -> anyhow::Result<NoticeResponse> {
        self.get("notice").await
    }

    pub async fn notice_for_location(&self, slug: &str) -> anyhow::Result<NoticeResponse> {
        self.get(&format!("locations/{slug}/notice")).await
    }

    pub async fn closing_notice(&self) -> anyhow::Result<ClosingNoticeResponse> {
        self.get("closing-notice").await
    }

    pub async fn closing_notice_for_location(
        &self,
        slug: &str,
    ) -> anyhow::Result<ClosingNoticeResponse> {
        self.get(&format!("locations/{slug}/closing-notice")).await
    }

    pub async fn history(&self, range: &str) -> anyhow::Result<HistoryResponse> {
        let mut url = self.base_url.join("history")?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("range", range);
        }
        self.http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .map_err(Into::into)
    }

    pub async fn history_for_location(
        &self,
        slug: &str,
        range: &str,
    ) -> anyhow::Result<HistoryResponse> {
        let mut url = self.base_url.join(&format!("locations/{slug}/history"))?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("range", range);
        }
        self.http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .map_err(Into::into)
    }

    pub async fn healthz(&self) -> anyhow::Result<HealthResponse> {
        self.get("healthz").await
    }

    async fn get<T>(&self, path: &str) -> anyhow::Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let url = self.base_url.join(path)?;
        self.http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .map_err(Into::into)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationsResponse {
    pub generated_at: DateTime<Utc>,
    pub locations: Vec<LocationResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoticeResponse {
    pub notices: Vec<String>,
    pub observed_at: Option<DateTime<Utc>>,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosingNoticeResponse {
    pub closing_notices: Vec<String>,
    pub observed_at: Option<DateTime<Utc>>,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub ok: bool,
    pub service: String,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryResponse {
    pub range: String,
    pub generated_at: DateTime<Utc>,
    pub points: Vec<HistoryPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
