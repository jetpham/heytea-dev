use anyhow::Context;
use heytea_core::{
    ClosingNoticeResponse, HealthResponse, HistoryResponse, NoticeResponse, StatusResponse,
    WaitTimeResponse,
};
use url::Url;

pub use heytea_core;

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

    pub async fn wait_time(&self) -> anyhow::Result<WaitTimeResponse> {
        self.get("wait-time").await
    }

    pub async fn notice(&self) -> anyhow::Result<NoticeResponse> {
        self.get("notice").await
    }

    pub async fn closing_notice(&self) -> anyhow::Result<ClosingNoticeResponse> {
        self.get("closing-notice").await
    }

    pub async fn history(
        &self,
        range: &str,
        bucket: Option<&str>,
    ) -> anyhow::Result<HistoryResponse> {
        let mut url = self.base_url.join("history")?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("range", range);
            if let Some(bucket) = bucket {
                query.append_pair("bucket", bucket);
            }
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
