use anyhow::anyhow;
use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::time::Instant;

const BASE_URL: &str = "https://app.heytea-co.com";

#[derive(Clone)]
pub struct HeyTeaClient {
    http: reqwest::Client,
}

impl HeyTeaClient {
    pub fn new() -> anyhow::Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/prs.heytea.v1+json"),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("Client", HeaderValue::from_static("2"));
        headers.insert("X-client", HeaderValue::from_static("app"));
        headers.insert("X-version", HeaderValue::from_static("2.3.1"));
        headers.insert("version", HeaderValue::from_static("2.3.1"));
        headers.insert("client-system", HeaderValue::from_static("android"));
        headers.insert("x-region-code", HeaderValue::from_static("US"));
        headers.insert("x-region-id", HeaderValue::from_static("16"));

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(10))
            .build()?;

        Ok(Self { http })
    }

    pub async fn fetch_shop_metadata(&self, shop_id: i64) -> PollResult<ShopMetadata> {
        let started = Instant::now();
        let url = format!("{BASE_URL}/api/service-smc/openapi/app/user/closest/shop-list");
        let response = self
            .http
            .get(url)
            .query(&[
                ("country_code", "840"),
                ("city_code", "s840100028"),
                ("user_location", "-122.399,37.781"),
            ])
            .send()
            .await;

        match response {
            Ok(response) => {
                let status_code = response.status().as_u16();
                let envelope = response.json::<Envelope<Vec<ShopListItem>>>().await;
                match envelope {
                    Ok(envelope) if envelope.code == 0 => {
                        let item = envelope
                            .data
                            .unwrap_or_default()
                            .into_iter()
                            .find(|shop| shop.id == shop_id)
                            .ok_or_else(|| anyhow!("configured shop was not present in shop list"));
                        match item {
                            Ok(shop) => PollResult::ok(
                                status_code,
                                started,
                                ShopMetadata {
                                    name: english_store_name(&shop.name),
                                    address: english_store_address(&shop.address),
                                    latitude: shop.latitude.and_then(|value| value.parse().ok()),
                                    longitude: shop.longitude.and_then(|value| value.parse().ok()),
                                    is_open: shop.is_open,
                                    is_enabled: shop.is_enable.map(|value| value == 1),
                                    support_takeaway: shop.support_takeaway.map(|value| value == 1),
                                    hours: shop.business_time_list.unwrap_or_default(),
                                    observed_at: Utc::now(),
                                },
                            ),
                            Err(error) => PollResult::error(status_code, started, error),
                        }
                    }
                    Ok(envelope) => PollResult::error(
                        status_code,
                        started,
                        anyhow!(
                            "shop list returned code {}: {}",
                            envelope.code,
                            envelope.message
                        ),
                    ),
                    Err(error) => PollResult::error(status_code, started, error),
                }
            }
            Err(error) => PollResult::error(None, started, error),
        }
    }

    pub async fn fetch_wait_time(&self, shop_id: i64) -> PollResult<WaitTime> {
        let started = Instant::now();
        let url = format!("{BASE_URL}/api/service-ofc/openapi/agent/expect-time/shop/list");
        let body = WaitTimeRequest {
            expect_time_by_shop_ids: vec![WaitTimeShopRequest {
                shop_id,
                floor_index: None,
            }],
            is_takeaway: false,
            show_time: true,
            location: "-122.399,37.781".to_string(),
        };

        let response = self.http.post(url).json(&body).send().await;
        match response {
            Ok(response) => {
                let status_code = response.status().as_u16();
                let envelope = response.json::<Envelope<Vec<WaitTimeItem>>>().await;
                match envelope {
                    Ok(envelope) if envelope.code == 0 => {
                        let item = envelope
                            .data
                            .unwrap_or_default()
                            .into_iter()
                            .find(|item| item.shop_id == shop_id)
                            .ok_or_else(|| {
                                anyhow!("wait-time response did not include configured shop")
                            });
                        match item {
                            Ok(item) => PollResult::ok(
                                status_code,
                                started,
                                WaitTime {
                                    pickup_wait_minutes: item.expect_time,
                                    delivery_estimate_minutes: item.takeaway_time,
                                    making_cups: item.making_cups,
                                    making_orders: item.making_order,
                                    is_estimate: item.is_estimate_time,
                                    observed_at: Utc::now(),
                                },
                            ),
                            Err(error) => PollResult::error(status_code, started, error),
                        }
                    }
                    Ok(envelope) => PollResult::error(
                        status_code,
                        started,
                        anyhow!(
                            "wait time returned code {}: {}",
                            envelope.code,
                            envelope.message
                        ),
                    ),
                    Err(error) => PollResult::error(status_code, started, error),
                }
            }
            Err(error) => PollResult::error(None, started, error),
        }
    }

    pub async fn fetch_notice(&self, shop_id: i64) -> PollResult<Option<String>> {
        let started = Instant::now();
        let url = format!("{BASE_URL}/api/service-smc/openapi/app/shop/notice/{shop_id}");
        let response = self.http.get(url).send().await;
        match response {
            Ok(response) => {
                let status_code = response.status().as_u16();
                let envelope = response.json::<Envelope<NoticeData>>().await;
                match envelope {
                    Ok(envelope) if envelope.code == 0 => {
                        let notice = envelope.data.and_then(|data| {
                            data.notice
                                .into_iter()
                                .filter_map(|notice| notice.content)
                                .find(|content| !content.trim().is_empty())
                        });
                        PollResult::ok(status_code, started, notice)
                    }
                    Ok(envelope) => PollResult::error(
                        status_code,
                        started,
                        anyhow!(
                            "notice returned code {}: {}",
                            envelope.code,
                            envelope.message
                        ),
                    ),
                    Err(error) => PollResult::error(status_code, started, error),
                }
            }
            Err(error) => PollResult::error(None, started, error),
        }
    }

    pub async fn fetch_closing_notice(&self, shop_id: i64) -> PollResult<Option<String>> {
        let started = Instant::now();
        let url = format!("{BASE_URL}/api/service-smc/openapi/app/event/pop-up/shop-before-closed");
        let response = self
            .http
            .get(url)
            .query(&[("shopId", shop_id)])
            .send()
            .await;
        match response {
            Ok(response) => {
                let status_code = response.status().as_u16();
                let envelope = response.json::<Envelope<serde_json::Value>>().await;
                match envelope {
                    Ok(envelope) if envelope.code == 0 => {
                        let notice = envelope.data.and_then(|value| {
                            value
                                .get("heyteaPopup")
                                .or_else(|| value.get("content"))
                                .and_then(|value| value.as_str())
                                .map(ToOwned::to_owned)
                        });
                        PollResult::ok(status_code, started, notice)
                    }
                    Ok(envelope) => PollResult::error(
                        status_code,
                        started,
                        anyhow!(
                            "closing notice returned code {}: {}",
                            envelope.code,
                            envelope.message
                        ),
                    ),
                    Err(error) => PollResult::error(status_code, started, error),
                }
            }
            Err(error) => PollResult::error(None, started, error),
        }
    }
}

#[derive(Debug)]
pub struct PollResult<T> {
    pub value: Option<T>,
    pub success: bool,
    pub status_code: Option<i32>,
    pub latency_ms: i32,
    pub error_message: Option<String>,
}

trait IntoStatusCodeOption {
    fn into_option(self) -> Option<u16>;
}

impl IntoStatusCodeOption for u16 {
    fn into_option(self) -> Option<u16> {
        Some(self)
    }
}

impl IntoStatusCodeOption for Option<u16> {
    fn into_option(self) -> Option<u16> {
        self
    }
}

impl<T> PollResult<T> {
    fn ok(status_code: impl IntoStatusCodeOption, started: Instant, value: T) -> Self {
        Self {
            value: Some(value),
            success: true,
            status_code: status_code.into_option().map(i32::from),
            latency_ms: started.elapsed().as_millis().try_into().unwrap_or(i32::MAX),
            error_message: None,
        }
    }

    fn error(
        status_code: impl IntoStatusCodeOption,
        started: Instant,
        error: impl Into<anyhow::Error>,
    ) -> Self {
        let error = error.into();
        Self {
            value: None,
            success: false,
            status_code: status_code.into_option().map(i32::from),
            latency_ms: started.elapsed().as_millis().try_into().unwrap_or(i32::MAX),
            error_message: Some(format!("{error:#}")),
        }
    }
}

#[derive(Debug)]
pub struct ShopMetadata {
    pub name: String,
    pub address: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub is_open: Option<bool>,
    pub is_enabled: Option<bool>,
    pub support_takeaway: Option<bool>,
    pub hours: serde_json::Value,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct WaitTime {
    pub pickup_wait_minutes: Option<i32>,
    pub delivery_estimate_minutes: Option<i32>,
    pub making_cups: Option<i32>,
    pub making_orders: Option<i32>,
    pub is_estimate: Option<bool>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    code: i32,
    message: String,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct ShopListItem {
    id: i64,
    name: String,
    address: String,
    latitude: Option<String>,
    longitude: Option<String>,
    is_open: Option<bool>,
    is_enable: Option<i32>,
    support_takeaway: Option<i32>,
    business_time_list: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WaitTimeRequest {
    expect_time_by_shop_ids: Vec<WaitTimeShopRequest>,
    is_takeaway: bool,
    show_time: bool,
    location: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WaitTimeShopRequest {
    shop_id: i64,
    floor_index: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WaitTimeItem {
    shop_id: i64,
    making_cups: Option<i32>,
    making_order: Option<i32>,
    expect_time: Option<i32>,
    takeaway_time: Option<i32>,
    is_estimate_time: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct NoticeData {
    notice: Vec<NoticeItem>,
}

#[derive(Debug, Deserialize)]
struct NoticeItem {
    content: Option<String>,
}

fn english_store_name(_upstream: &str) -> String {
    "Downtown Metreon".to_string()
}

fn english_store_address(_upstream: &str) -> String {
    "165 4th St, San Francisco, CA 94103".to_string()
}
