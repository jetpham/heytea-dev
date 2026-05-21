use anyhow::{anyhow, Context};
use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, USER_AGENT};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, time::Instant};
use tokio::time::{sleep, Duration};

const APP_VERSION: &str = "2.3.1";
const GO_VERSION: &str = "3.7.6";
const CHINA_COUNTRY_CODE: &str = "156";
const GO_CHINA_BASE_URL: &str = "https://go.heytea.com";
const GO_CHINA_LOCATION: &str = "113.408009,22.803969";
const CATALOG_REQUEST_DELAY_MS: u64 = 50;

const INTERNATIONAL_REGIONS: &[InternationalRegion] = &[
    // app-jp/app-fr currently return empty regional catalogs with their
    // regional headers, so keep them out until upstream advertises stores.
    InternationalRegion::Us,
    InternationalRegion::Ca,
    InternationalRegion::Gb,
    InternationalRegion::Sg,
    InternationalRegion::Au,
    InternationalRegion::My,
    InternationalRegion::Kr,
    InternationalRegion::Cn,
];

#[derive(Clone)]
pub struct HeyTeaClient {
    http: reqwest::Client,
}

impl HeyTeaClient {
    pub fn new() -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()?;

        Ok(Self { http })
    }

    pub async fn fetch_shop_catalog(&self) -> PollResult<Vec<ShopMetadata>> {
        let started = Instant::now();
        let mut shops = HashMap::new();
        let mut errors = Vec::new();

        for region in INTERNATIONAL_REGIONS {
            match self.fetch_international_catalog(*region).await {
                Ok(items) => merge_shops(&mut shops, items),
                Err(error) => errors.push(format!("{}: {error:#}", region.code())),
            }
        }

        match self.fetch_go_china_catalog().await {
            Ok(items) => merge_shops(&mut shops, items),
            Err(error) => errors.push(format!("go-cn: {error:#}")),
        }

        if shops.is_empty() {
            let status_code = access_denial_status(&errors);
            return PollResult::error(
                status_code,
                started,
                anyhow!("all catalog providers failed: {}", errors.join("; ")),
            );
        }

        if !errors.is_empty() {
            tracing::warn!(?errors, "catalog provider failures during partial refresh");
        }

        PollResult::ok(None, started, shops.into_values().collect())
    }

    pub async fn fetch_wait_times(
        &self,
        provider: WaitProvider,
        shop_ids: &[i64],
    ) -> PollResult<Vec<WaitTime>> {
        let started = Instant::now();
        if shop_ids.is_empty() {
            return PollResult::ok(None, started, Vec::new());
        }

        let url = provider.wait_time_url();
        let headers = provider.headers();
        let body = WaitTimeRequest {
            expect_time_by_shop_ids: shop_ids
                .iter()
                .copied()
                .map(|shop_id| WaitTimeShopRequest {
                    shop_id,
                    floor_index: None,
                })
                .collect(),
            is_takeaway: false,
            show_time: true,
            location: provider.default_location().to_string(),
        };

        let response = self
            .http
            .post(url)
            .headers(headers)
            .json(&body)
            .send()
            .await;
        match response {
            Ok(response) => {
                let status_code = response.status().as_u16();
                let envelope = response.json::<Envelope<Vec<WaitTimeItem>>>().await;
                match envelope {
                    Ok(envelope) if envelope.code == 0 => {
                        let waits = envelope
                            .data
                            .unwrap_or_default()
                            .into_iter()
                            .map(|item| WaitTime {
                                shop_id: item.shop_id,
                                pickup_wait_minutes: item.expect_time,
                                delivery_estimate_minutes: item.takeaway_time,
                                making_cups: item.making_cups,
                                making_orders: item.making_order,
                                is_estimate: item.is_estimate_time,
                                text: non_empty_string(item.text),
                                observed_at: Utc::now(),
                            })
                            .collect();
                        PollResult::ok(status_code, started, waits)
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

    pub async fn fetch_notice(
        &self,
        provider: WaitProvider,
        shop_id: i64,
    ) -> PollResult<StoreNotices> {
        let started = Instant::now();
        let Some(region) = provider.international_region() else {
            return PollResult::ok(
                None,
                started,
                StoreNotices {
                    shop_id,
                    notices: Vec::new(),
                    observed_at: Utc::now(),
                },
            );
        };

        let url = format!(
            "{}/api/service-smc/openapi/app/shop/notice/{shop_id}",
            region.base_url()
        );
        let response = self.http.get(url).headers(region.headers()).send().await;
        match response {
            Ok(response) => {
                let status_code = response.status().as_u16();
                let envelope = response.json::<Envelope<NoticeData>>().await;
                match envelope {
                    Ok(envelope) if envelope.code == 0 => {
                        let notices = envelope
                            .data
                            .map(|data| {
                                data.notice
                                    .into_iter()
                                    .filter_map(|notice| non_empty_string(notice.content))
                                    .collect()
                            })
                            .unwrap_or_default();
                        PollResult::ok(
                            status_code,
                            started,
                            StoreNotices {
                                shop_id,
                                notices,
                                observed_at: Utc::now(),
                            },
                        )
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

    pub async fn fetch_closing_notice(
        &self,
        provider: WaitProvider,
        shop_id: i64,
    ) -> PollResult<StoreClosingNotices> {
        let started = Instant::now();
        let Some(region) = provider.international_region() else {
            return PollResult::ok(
                None,
                started,
                StoreClosingNotices {
                    shop_id,
                    closing_notices: Vec::new(),
                    observed_at: Utc::now(),
                },
            );
        };

        let url = format!(
            "{}/api/service-smc/openapi/app/event/pop-up/shop-before-closed",
            region.base_url()
        );
        let response = self
            .http
            .get(url)
            .headers(region.headers())
            .query(&[("shopId", shop_id)])
            .send()
            .await;
        match response {
            Ok(response) => {
                let status_code = response.status().as_u16();
                let envelope = response.json::<Envelope<Value>>().await;
                match envelope {
                    Ok(envelope) if envelope.code == 0 => {
                        let closing_notices = envelope
                            .data
                            .map(|value| closing_notice_strings(&value))
                            .unwrap_or_default();
                        PollResult::ok(
                            status_code,
                            started,
                            StoreClosingNotices {
                                shop_id,
                                closing_notices,
                                observed_at: Utc::now(),
                            },
                        )
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

    async fn fetch_international_catalog(
        &self,
        region: InternationalRegion,
    ) -> anyhow::Result<Vec<ShopMetadata>> {
        let base_url = region.base_url();
        let countries = self
            .get_envelope::<Vec<RegionCountry>>(
                &format!("{base_url}/api/service-location/grayapi/app/region/oversea"),
                region.headers(),
            )
            .await?
            .data
            .unwrap_or_default();

        let mut shops = HashMap::new();
        for country in countries {
            let country_code = country.country_code.as_str();
            let queries = std::iter::once((None, region.default_location())).chain(
                country.cities.iter().map(|city| {
                    (
                        Some(city.city_code.as_str()),
                        international_city_location(region, &city.city_code),
                    )
                }),
            );

            for (city_code, location) in queries {
                let mut request = self
                    .http
                    .get(format!(
                        "{base_url}/api/service-smc/openapi/app/user/closest/shop-list"
                    ))
                    .headers(region.headers())
                    .query(&[("country_code", country_code), ("user_location", location)]);
                if let Some(city_code) = city_code {
                    request = request.query(&[("city_code", city_code)]);
                }

                let response = request.send().await.context("shop list request failed")?;
                let status_code = response.status().as_u16();
                let envelope = response
                    .json::<Envelope<Vec<ShopListItem>>>()
                    .await
                    .with_context(|| format!("failed to parse shop list ({status_code})"))?;
                if envelope.code != 0 {
                    return Err(anyhow!(
                        "shop list returned code {}: {}",
                        envelope.code,
                        envelope.message
                    ));
                }

                for shop in envelope.data.unwrap_or_default() {
                    let metadata = shop_metadata(shop, country_code, city_code);
                    shops.insert(metadata.shop_id, metadata);
                }
            }
        }

        Ok(shops.into_values().collect())
    }

    async fn fetch_go_china_catalog(&self) -> anyhow::Result<Vec<ShopMetadata>> {
        let area = self
            .get_envelope::<Vec<GoAreaCountry>>(
                &format!(
                    "{GO_CHINA_BASE_URL}/api/service-sale/vip/openapi/area/include-country?include_country=1"
                ),
                go_headers(),
            )
            .await?
            .data
            .unwrap_or_default();

        let mut shops = HashMap::new();
        for country in area {
            if country.code != CHINA_COUNTRY_CODE {
                continue;
            }

            for city in country.city {
                let body = GoShopListRequest {
                    country_code: &country.code,
                    city_code: &city.city_code,
                    district_code: "",
                    user_location: "",
                };
                let response = self
                    .http
                    .post(format!(
                        "{GO_CHINA_BASE_URL}/api/service-smc/grayapi/shop-list"
                    ))
                    .headers(go_headers())
                    .json(&body)
                    .send()
                    .await
                    .context("go china shop list request failed")?;
                let status_code = response.status().as_u16();
                let envelope = response
                    .json::<Envelope<Vec<ShopListItem>>>()
                    .await
                    .with_context(|| {
                        format!("failed to parse go china shop list ({status_code})")
                    })?;
                if envelope.code != 0 {
                    return Err(anyhow!(
                        "go china shop list returned code {}: {}",
                        envelope.code,
                        envelope.message
                    ));
                }

                for shop in envelope.data.unwrap_or_default() {
                    let metadata = shop_metadata(shop, CHINA_COUNTRY_CODE, Some(&city.city_code));
                    shops.insert(metadata.shop_id, metadata);
                }

                sleep(Duration::from_millis(CATALOG_REQUEST_DELAY_MS)).await;
            }
        }

        Ok(shops.into_values().collect())
    }

    async fn get_envelope<T>(&self, url: &str, headers: HeaderMap) -> anyhow::Result<Envelope<T>>
    where
        T: DeserializeOwned,
    {
        let response = self
            .http
            .get(url)
            .headers(headers)
            .send()
            .await
            .with_context(|| format!("request failed: {url}"))?;
        let status_code = response.status().as_u16();
        let envelope = response
            .json::<Envelope<T>>()
            .await
            .with_context(|| format!("failed to parse envelope ({status_code}): {url}"))?;
        if envelope.code != 0 {
            return Err(anyhow!(
                "request returned code {}: {} ({url})",
                envelope.code,
                envelope.message
            ));
        }
        Ok(envelope)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum WaitProvider {
    International(InternationalRegion),
    GoChina,
}

impl WaitProvider {
    pub fn batch_size(self) -> usize {
        match self {
            Self::International(_) => 10,
            Self::GoChina => 100,
        }
    }

    pub fn supports_notices(self) -> bool {
        matches!(self, Self::International(_))
    }

    pub fn provider_name(self) -> &'static str {
        match self {
            Self::International(_) => "international",
            Self::GoChina => "go-china",
        }
    }

    pub fn region_code(self) -> &'static str {
        match self {
            Self::International(region) => region.code(),
            Self::GoChina => "CN",
        }
    }

    pub fn cooldown_key(self) -> String {
        format!("{}:{}", self.provider_name(), self.region_code())
    }

    fn international_region(self) -> Option<InternationalRegion> {
        match self {
            Self::International(region) => Some(region),
            Self::GoChina => None,
        }
    }

    fn default_location(self) -> &'static str {
        match self {
            Self::International(region) => region.default_location(),
            Self::GoChina => GO_CHINA_LOCATION,
        }
    }

    fn headers(self) -> HeaderMap {
        match self {
            Self::International(region) => region.headers(),
            Self::GoChina => go_headers(),
        }
    }

    fn wait_time_url(self) -> String {
        match self {
            Self::International(region) => format!(
                "{}/api/service-ofc/openapi/agent/expect-time/shop/list",
                region.base_url()
            ),
            Self::GoChina => {
                format!("{GO_CHINA_BASE_URL}/api/service-ofc/openapi/agent/expect-time/shop/list")
            }
        }
    }
}

fn access_denial_status(errors: &[String]) -> Option<u16> {
    [403, 405, 429].into_iter().find(|status| {
        let parenthesized = format!("({status})");
        let spaced = format!(" {status} ");
        errors.iter().any(|error| {
            error.contains(&parenthesized)
                || error.contains(&spaced)
                || error.ends_with(&status.to_string())
        })
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum InternationalRegion {
    Us,
    Ca,
    Gb,
    Sg,
    Au,
    My,
    Kr,
    Cn,
}

impl InternationalRegion {
    fn code(self) -> &'static str {
        match self {
            Self::Us => "US",
            Self::Ca => "CA",
            Self::Gb => "GB",
            Self::Sg => "SG",
            Self::Au => "AU",
            Self::My => "MY",
            Self::Kr => "KR",
            Self::Cn => "CN",
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::Us => "us",
            Self::Ca => "ca",
            Self::Gb => "gb",
            Self::Sg => "sg",
            Self::Au => "au",
            Self::My => "my",
            Self::Kr => "kr",
            Self::Cn => "cn",
        }
    }

    fn base_url(self) -> String {
        format!("https://app-{}.heytea-co.com", self.suffix())
    }

    fn x_region_id(self) -> &'static str {
        match self {
            Self::Us => "16",
            Self::Ca => "17",
            Self::Gb => "18",
            Self::Sg => "19",
            Self::Au => "20",
            Self::My => "21",
            Self::Kr => "22",
            Self::Cn => "23",
        }
    }

    fn default_location(self) -> &'static str {
        match self {
            Self::Us => "-122.399,37.781",
            Self::Ca => "-79.383753,43.663442",
            Self::Gb => "-0.130513,51.512738",
            Self::Sg => "103.851959,1.290270",
            Self::Au => "144.963058,-37.813629",
            Self::My => "101.686855,3.139003",
            Self::Kr => "126.978000,37.566500",
            Self::Cn => "114.1694,22.3193",
        }
    }

    fn headers(self) -> HeaderMap {
        let mut headers = app_headers();
        headers.insert("x-region-code", HeaderValue::from_static(self.code()));
        headers.insert("x-region-id", HeaderValue::from_static(self.x_region_id()));
        headers
    }
}

#[derive(Clone, Debug)]
pub struct ShopRef {
    pub shop_id: i64,
    pub country_code: String,
    pub city_code: Option<String>,
}

impl ShopRef {
    pub fn wait_provider(&self) -> Option<WaitProvider> {
        wait_provider_for(&self.country_code, self.city_code.as_deref())
    }
}

#[derive(Debug)]
pub struct ShopMetadata {
    pub shop_id: i64,
    pub slug: String,
    pub provider: String,
    pub region_code: Option<String>,
    pub name: String,
    pub address: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub timezone: String,
    pub country_code: String,
    pub city_code: Option<String>,
    pub is_open: Option<bool>,
    pub is_enabled: Option<bool>,
    pub support_takeaway: Option<bool>,
    pub hours: Value,
    pub observed_at: DateTime<Utc>,
}

impl ShopMetadata {
    pub fn to_ref(&self) -> ShopRef {
        ShopRef {
            shop_id: self.shop_id,
            country_code: self.country_code.clone(),
            city_code: self.city_code.clone(),
        }
    }
}

#[derive(Debug)]
pub struct WaitTime {
    pub shop_id: i64,
    pub pickup_wait_minutes: Option<i32>,
    pub delivery_estimate_minutes: Option<i32>,
    pub making_cups: Option<i32>,
    pub making_orders: Option<i32>,
    pub is_estimate: Option<bool>,
    pub text: Option<String>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct StoreNotices {
    pub shop_id: i64,
    pub notices: Vec<String>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct StoreClosingNotices {
    pub shop_id: i64,
    pub closing_notices: Vec<String>,
    pub observed_at: DateTime<Utc>,
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

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    code: i32,
    message: String,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct RegionCountry {
    country_code: String,
    #[serde(default)]
    cities: Vec<RegionCity>,
}

#[derive(Debug, Deserialize)]
struct RegionCity {
    city_code: String,
}

#[derive(Debug, Deserialize)]
struct GoAreaCountry {
    code: String,
    #[serde(default)]
    city: Vec<GoAreaCity>,
}

#[derive(Debug, Deserialize)]
struct GoAreaCity {
    city_code: String,
}

#[derive(Debug, Deserialize)]
struct ShopListItem {
    id: i64,
    name: Option<String>,
    address: Option<String>,
    latitude: Option<Value>,
    longitude: Option<Value>,
    country_code: Option<String>,
    city_code: Option<String>,
    is_open: Option<bool>,
    is_enable: Option<Value>,
    support_takeaway: Option<Value>,
    business_time_list: Option<Value>,
}

#[derive(Debug, Serialize)]
struct GoShopListRequest<'a> {
    country_code: &'a str,
    city_code: &'a str,
    district_code: &'a str,
    user_location: &'a str,
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
    #[serde(skip_serializing_if = "Option::is_none")]
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
    text: Option<String>,
    is_estimate_time: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct NoticeData {
    #[serde(default)]
    notice: Vec<NoticeItem>,
}

#[derive(Debug, Deserialize)]
struct NoticeItem {
    content: Option<String>,
}

fn merge_shops(target: &mut HashMap<i64, ShopMetadata>, shops: Vec<ShopMetadata>) {
    for shop in shops {
        target.insert(shop.shop_id, shop);
    }
}

fn common_headers(version: &'static str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/prs.heytea.v1+json"),
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(USER_AGENT, HeaderValue::from_static("okhttp/4.12.0"));
    headers.insert("Client", HeaderValue::from_static("2"));
    headers.insert("X-client", HeaderValue::from_static("app"));
    headers.insert("X-version", HeaderValue::from_static(version));
    headers.insert("version", HeaderValue::from_static(version));
    headers
}

fn app_headers() -> HeaderMap {
    let mut headers = common_headers(APP_VERSION);
    headers.insert("client-system", HeaderValue::from_static("android"));
    headers.insert("Accept-Language", HeaderValue::from_static("en-US"));
    headers
}

fn go_headers() -> HeaderMap {
    let mut headers = common_headers(GO_VERSION);
    headers.insert("GTM-Zone", HeaderValue::from_static("Asia/Shanghai"));
    headers.insert("Accept-Language", HeaderValue::from_static("zh-CN"));
    headers
}

fn international_city_location(region: InternationalRegion, city_code: &str) -> &'static str {
    match city_code {
        "156820000" => "113.5439,22.1987",
        _ => region.default_location(),
    }
}

fn wait_provider_for(country_code: &str, city_code: Option<&str>) -> Option<WaitProvider> {
    match country_code {
        "840" => Some(WaitProvider::International(InternationalRegion::Us)),
        "124" => Some(WaitProvider::International(InternationalRegion::Ca)),
        "826" => Some(WaitProvider::International(InternationalRegion::Gb)),
        "702" => Some(WaitProvider::International(InternationalRegion::Sg)),
        "036" | "36" => Some(WaitProvider::International(InternationalRegion::Au)),
        "458" => Some(WaitProvider::International(InternationalRegion::My)),
        "410" => Some(WaitProvider::International(InternationalRegion::Kr)),
        "156" if is_hk_or_macao(city_code) => {
            Some(WaitProvider::International(InternationalRegion::Cn))
        }
        "156" => Some(WaitProvider::GoChina),
        _ => None,
    }
}

fn is_hk_or_macao(city_code: Option<&str>) -> bool {
    city_code
        .map(|value| value.starts_with("15681") || value.starts_with("15682"))
        .unwrap_or(false)
}

fn shop_metadata(
    shop: ShopListItem,
    fallback_country_code: &str,
    fallback_city_code: Option<&str>,
) -> ShopMetadata {
    let country_code = shop
        .country_code
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback_country_code.to_string());
    let city_code = shop
        .city_code
        .filter(|value| !value.trim().is_empty())
        .or_else(|| fallback_city_code.map(ToOwned::to_owned));
    let latitude = value_as_f64(&shop.latitude);
    let longitude = value_as_f64(&shop.longitude);
    let name = store_name(shop.id, shop.name.as_deref().unwrap_or_default());
    let address = store_address(shop.id, shop.address.as_deref().unwrap_or_default());
    let timezone = timezone_for(&country_code, city_code.as_deref(), longitude).to_string();
    let provider = wait_provider_for(&country_code, city_code.as_deref());

    ShopMetadata {
        shop_id: shop.id,
        slug: slugify(&name, shop.id),
        provider: provider
            .map(WaitProvider::provider_name)
            .unwrap_or("unknown")
            .to_string(),
        region_code: provider.map(|provider| provider.region_code().to_string()),
        name,
        address,
        latitude,
        longitude,
        timezone,
        country_code,
        city_code,
        is_open: shop.is_open,
        is_enabled: value_as_bool(&shop.is_enable),
        support_takeaway: value_as_bool(&shop.support_takeaway),
        hours: shop
            .business_time_list
            .unwrap_or_else(|| Value::Array(Vec::new())),
        observed_at: Utc::now(),
    }
}

fn store_name(shop_id: i64, upstream: &str) -> String {
    if shop_id == 1_000_092 {
        return "Downtown Metreon".to_string();
    }

    let name = upstream.trim();
    if name.is_empty() {
        format!("shop {shop_id}")
    } else {
        name.to_string()
    }
}

fn store_address(shop_id: i64, upstream: &str) -> String {
    if shop_id == 1_000_092 {
        return "165 4th St, San Francisco, CA 94103".to_string();
    }

    let address = upstream.trim();
    if address.is_empty() {
        "unknown".to_string()
    } else {
        address.to_string()
    }
}

fn value_as_f64(value: &Option<Value>) -> Option<f64> {
    match value {
        Some(Value::Number(number)) => number.as_f64(),
        Some(Value::String(value)) => value.parse().ok(),
        _ => None,
    }
}

fn value_as_bool(value: &Option<Value>) -> Option<bool> {
    match value {
        Some(Value::Bool(value)) => Some(*value),
        Some(Value::Number(value)) => value.as_i64().map(|value| value != 0),
        Some(Value::String(value)) => match value.as_str() {
            "1" | "true" | "TRUE" => Some(true),
            "0" | "false" | "FALSE" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn non_empty_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    })
}

fn closing_notice_strings(value: &Value) -> Vec<String> {
    ["heyteaPopup", "content", "message"]
        .into_iter()
        .filter_map(|key| value.get(key))
        .filter_map(Value::as_str)
        .filter_map(|value| non_empty_string(Some(value.to_string())))
        .collect()
}

fn slugify(name: &str, shop_id: i64) -> String {
    let slug = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    if slug.is_empty() {
        format!("shop-{shop_id}")
    } else {
        slug
    }
}

fn timezone_for(
    country_code: &str,
    city_code: Option<&str>,
    longitude: Option<f64>,
) -> &'static str {
    match country_code {
        "840" => match longitude {
            Some(value) if value <= -115.0 => "America/Los_Angeles",
            Some(value) if value <= -105.0 => "America/Denver",
            Some(value) if value <= -90.0 => "America/Chicago",
            _ => "America/New_York",
        },
        "124" => match longitude {
            Some(value) if value <= -100.0 => "America/Vancouver",
            _ => "America/Toronto",
        },
        "826" => "Europe/London",
        "702" => "Asia/Singapore",
        "036" | "36" => match city_code {
            Some("s036100002") => "Australia/Brisbane",
            Some("s036100001") => "Australia/Sydney",
            _ => "Australia/Melbourne",
        },
        "458" => "Asia/Kuala_Lumpur",
        "410" => "Asia/Seoul",
        "156" if is_hk_or_macao(city_code) => match city_code {
            Some(value) if value.starts_with("15682") => "Asia/Macau",
            _ => "Asia/Hong_Kong",
        },
        "156" => "Asia/Shanghai",
        _ => "UTC",
    }
}
