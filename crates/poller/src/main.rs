mod persistence;
mod upstream;

use anyhow::Context;
use chrono::{DateTime, Utc};
use futures::{stream, StreamExt};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::{collections::BTreeMap, env, time::Duration};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use upstream::{
    HeyTeaClient, PollResult, ShopRef, StoreClosingNotices, StoreNotices, WaitProvider, WaitTime,
};

const DEFAULT_CATALOG_INTERVAL_SECONDS: u64 = 86_400;
const DEFAULT_CATALOG_RETRY_INTERVAL_SECONDS: u64 = 900;
const DEFAULT_NOTICE_INTERVAL_SECONDS: u64 = 86_400;
const DEFAULT_UPSTREAM_COOLDOWN_SECONDS: u64 = 3_600;
const DEFAULT_UPSTREAM_CONCURRENCY: usize = 16;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://heytea:heytea@localhost:5432/heytea".to_string());
    let interval = env::var("HEYTEA_POLLER_INTERVAL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(60);
    let catalog_interval = env::var("HEYTEA_CATALOG_INTERVAL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_CATALOG_INTERVAL_SECONDS);
    let catalog_retry_interval = env::var("HEYTEA_CATALOG_RETRY_INTERVAL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_CATALOG_RETRY_INTERVAL_SECONDS);
    let upstream_concurrency = env::var("HEYTEA_UPSTREAM_CONCURRENCY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_UPSTREAM_CONCURRENCY);
    let notice_interval = env::var("HEYTEA_NOTICE_INTERVAL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_NOTICE_INTERVAL_SECONDS);
    let upstream_cooldown = Duration::from_secs(
        env::var("HEYTEA_UPSTREAM_COOLDOWN_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_UPSTREAM_COOLDOWN_SECONDS),
    );
    let max_connections = env_u32("HEYTEA_POLLER_MAX_CONNECTIONS", 5);

    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(&database_url)
        .await
        .context("failed to connect to postgres")?;
    let client = HeyTeaClient::new()?;

    tracing::info!(
        interval,
        catalog_interval,
        catalog_retry_interval,
        notice_interval,
        upstream_concurrency,
        upstream_cooldown_seconds = upstream_cooldown.as_secs(),
        max_connections,
        "starting heytea poller"
    );

    let mut last_catalog_attempt = None;
    let mut last_catalog_success = None;
    let mut last_notice_attempt = None;
    loop {
        let now = Utc::now();
        let catalog_due = last_catalog_success
            .map(|last| now.signed_duration_since(last).num_seconds() >= catalog_interval as i64)
            .unwrap_or(true);
        let catalog_retry_due = last_catalog_attempt
            .map(|last| {
                now.signed_duration_since(last).num_seconds() >= catalog_retry_interval as i64
            })
            .unwrap_or(true);
        let poll_catalog = catalog_due && catalog_retry_due;
        if poll_catalog {
            last_catalog_attempt = Some(now);
        }
        let poll_notices = last_notice_attempt
            .map(|last| now.signed_duration_since(last).num_seconds() >= notice_interval as i64)
            .unwrap_or(true);
        if poll_notices {
            last_notice_attempt = Some(now);
        }
        match poll_once(
            &pool,
            &client,
            poll_catalog,
            poll_notices,
            upstream_concurrency,
            upstream_cooldown,
        )
        .await
        {
            Ok(catalog_success) => {
                if poll_catalog && catalog_success {
                    last_catalog_success = Some(Utc::now());
                }
            }
            Err(error) => tracing::error!(?error, "poll cycle failed"),
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

async fn poll_once(
    pool: &sqlx::PgPool,
    client: &HeyTeaClient,
    poll_catalog: bool,
    poll_notices: bool,
    upstream_concurrency: usize,
    upstream_cooldown: Duration,
) -> anyhow::Result<bool> {
    let mut catalog_success = !poll_catalog;
    if poll_catalog {
        if persistence::cooldown_active(pool, "shop_catalog", "all").await? {
            tracing::warn!("shop catalog refresh skipped during upstream cooldown");
        } else {
            let catalog = client.fetch_shop_catalog().await;
            catalog_success = catalog.success;
            record_result_cooldown(pool, "shop_catalog", "all", &catalog, upstream_cooldown)
                .await?;
            let _ = persistence::persist_catalog(pool, catalog).await?;
        }
    }

    let managed_changed = persistence::refresh_managed_locations(pool).await?;
    if managed_changed > 0 {
        tracing::info!(managed_changed, "refreshed managed locations");
    }

    let shops = persistence::managed_shops(pool).await?;
    if shops.is_empty() {
        tracing::warn!("no managed shops available to poll");
        return Ok(catalog_success);
    }

    let wait_results = fetch_wait_results(
        pool,
        client,
        &shops,
        upstream_concurrency,
        upstream_cooldown,
    )
    .await?;
    let wait_observed_at = persistence::persist_wait_times(pool, wait_results).await?;

    let notice_observed_at = if poll_notices {
        let (notice_results, closing_notice_results) = fetch_notice_results(
            pool,
            client,
            &shops,
            upstream_concurrency,
            upstream_cooldown,
        )
        .await?;
        persistence::persist_notices(pool, notice_results, closing_notice_results).await?
    } else {
        None
    };

    if let Some(observed_at) = wait_observed_at.into_iter().chain(notice_observed_at).max() {
        publish_status_updated(pool, observed_at).await?;
    }
    Ok(catalog_success)
}

async fn fetch_wait_results(
    pool: &sqlx::PgPool,
    client: &HeyTeaClient,
    shops: &[ShopRef],
    upstream_concurrency: usize,
    upstream_cooldown: Duration,
) -> anyhow::Result<Vec<PollResult<Vec<WaitTime>>>> {
    let active_cooldowns = persistence::active_cooldowns(pool, "wait_time").await?;
    let jobs = group_shop_ids_by_provider(shops)
        .into_iter()
        .filter(|(provider, _)| !active_cooldowns.contains(&provider.cooldown_key()))
        .flat_map(|(provider, shop_ids)| {
            shop_ids
                .chunks(provider.batch_size())
                .map(move |batch| (provider, batch.to_vec()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    if jobs.is_empty() {
        tracing::warn!("all wait providers are cooling down or no wait jobs were available");
    }

    let provider_results = stream::iter(jobs)
        .map(|(provider, batch)| {
            let client = client.clone();
            async move { fetch_wait_job(client, provider, batch).await }
        })
        .buffer_unordered(upstream_concurrency)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    let mut results = Vec::with_capacity(provider_results.len());
    for provider_result in provider_results {
        record_result_cooldown(
            pool,
            "wait_time",
            &provider_result.provider.cooldown_key(),
            &provider_result.result,
            upstream_cooldown,
        )
        .await?;
        results.push(provider_result.result);
    }
    Ok(results)
}

struct ProviderPollResult<T> {
    provider: WaitProvider,
    result: PollResult<T>,
}

async fn fetch_wait_job(
    client: HeyTeaClient,
    provider: WaitProvider,
    batch: Vec<i64>,
) -> Vec<ProviderPollResult<Vec<WaitTime>>> {
    let result = client.fetch_wait_times(provider, &batch).await;
    if result.success || batch.len() <= 1 {
        return vec![ProviderPollResult { provider, result }];
    }
    if !should_retry_individual_waits(&result) {
        tracing::warn!(
            provider = ?provider,
            batch_size = batch.len(),
            status_code = ?result.status_code,
            "wait batch failed without individual retry"
        );
        return vec![ProviderPollResult { provider, result }];
    }

    tracing::warn!(
        provider = ?provider,
        batch_size = batch.len(),
        "wait batch failed; retrying individual shops"
    );
    let mut results = vec![ProviderPollResult { provider, result }];
    results.extend(
        stream::iter(batch)
            .map(|shop_id| {
                let client = client.clone();
                async move {
                    ProviderPollResult {
                        provider,
                        result: client.fetch_wait_times(provider, &[shop_id]).await,
                    }
                }
            })
            .buffer_unordered(8)
            .collect::<Vec<_>>()
            .await,
    );
    results
}

fn should_retry_individual_waits<T>(result: &PollResult<T>) -> bool {
    !matches!(result.status_code, Some(403 | 405 | 429))
}

async fn fetch_notice_results(
    pool: &sqlx::PgPool,
    client: &HeyTeaClient,
    shops: &[ShopRef],
    upstream_concurrency: usize,
    upstream_cooldown: Duration,
) -> anyhow::Result<(
    Vec<PollResult<StoreNotices>>,
    Vec<PollResult<StoreClosingNotices>>,
)> {
    let notice_cooldowns = persistence::active_cooldowns(pool, "notice").await?;
    let closing_notice_cooldowns = persistence::active_cooldowns(pool, "closing_notice").await?;
    let jobs = shops
        .iter()
        .filter_map(|shop| {
            let provider = shop.wait_provider()?;
            provider
                .supports_notices()
                .then_some((provider, shop.shop_id))
        })
        .flat_map(|(provider, shop_id)| {
            [
                NoticeJob::Notice { provider, shop_id },
                NoticeJob::Closing { provider, shop_id },
            ]
        })
        .filter(|job| match job {
            NoticeJob::Notice { provider, .. } => {
                !notice_cooldowns.contains(&provider.cooldown_key())
            }
            NoticeJob::Closing { provider, .. } => {
                !closing_notice_cooldowns.contains(&provider.cooldown_key())
            }
        })
        .collect::<Vec<_>>();

    let results = stream::iter(jobs)
        .map(|job| {
            let client = client.clone();
            async move {
                match job {
                    NoticeJob::Notice { provider, shop_id } => NoticeJobResult::Notice {
                        provider,
                        result: client.fetch_notice(provider, shop_id).await,
                    },
                    NoticeJob::Closing { provider, shop_id } => NoticeJobResult::Closing {
                        provider,
                        result: client.fetch_closing_notice(provider, shop_id).await,
                    },
                }
            }
        })
        .buffer_unordered(upstream_concurrency)
        .collect::<Vec<_>>()
        .await;

    let mut notices = Vec::new();
    let mut closing_notices = Vec::new();
    for result in results {
        match result {
            NoticeJobResult::Notice { provider, result } => {
                if let Err(error) = record_result_cooldown(
                    pool,
                    "notice",
                    &provider.cooldown_key(),
                    &result,
                    upstream_cooldown,
                )
                .await
                {
                    tracing::warn!(?error, "failed to record notice cooldown");
                }
                notices.push(result);
            }
            NoticeJobResult::Closing { provider, result } => {
                if let Err(error) = record_result_cooldown(
                    pool,
                    "closing_notice",
                    &provider.cooldown_key(),
                    &result,
                    upstream_cooldown,
                )
                .await
                {
                    tracing::warn!(?error, "failed to record closing notice cooldown");
                }
                closing_notices.push(result);
            }
        }
    }
    Ok((notices, closing_notices))
}

enum NoticeJob {
    Notice {
        provider: WaitProvider,
        shop_id: i64,
    },
    Closing {
        provider: WaitProvider,
        shop_id: i64,
    },
}

enum NoticeJobResult {
    Notice {
        provider: WaitProvider,
        result: PollResult<StoreNotices>,
    },
    Closing {
        provider: WaitProvider,
        result: PollResult<StoreClosingNotices>,
    },
}

async fn record_result_cooldown<T>(
    pool: &sqlx::PgPool,
    endpoint: &str,
    provider_key: &str,
    result: &PollResult<T>,
    upstream_cooldown: Duration,
) -> anyhow::Result<()> {
    if !should_trip_provider_cooldown(result) {
        return Ok(());
    }

    tracing::warn!(
        endpoint,
        provider_key,
        status_code = ?result.status_code,
        cooldown_seconds = upstream_cooldown.as_secs(),
        "tripping upstream provider cooldown"
    );
    persistence::trip_provider_cooldown(
        pool,
        endpoint,
        provider_key,
        result.status_code,
        result.error_message.as_deref(),
        upstream_cooldown,
    )
    .await
}

fn should_trip_provider_cooldown<T>(result: &PollResult<T>) -> bool {
    matches!(result.status_code, Some(403 | 405 | 429))
}

fn group_shop_ids_by_provider(shops: &[ShopRef]) -> BTreeMap<WaitProvider, Vec<i64>> {
    let mut groups = BTreeMap::<WaitProvider, Vec<i64>>::new();
    for shop in shops {
        let Some(provider) = shop.wait_provider() else {
            tracing::warn!(
                shop_id = shop.shop_id,
                country_code = %shop.country_code,
                city_code = ?shop.city_code,
                "no wait provider for shop"
            );
            continue;
        };
        groups.entry(provider).or_default().push(shop.shop_id);
    }
    groups
}

async fn publish_status_updated(
    pool: &sqlx::PgPool,
    observed_at: DateTime<Utc>,
) -> anyhow::Result<()> {
    let payload = json!({ "observedAt": observed_at }).to_string();
    sqlx::query("select pg_notify('heytea_status_updated', $1)")
        .bind(payload)
        .execute(pool)
        .await?;
    Ok(())
}

fn env_u32(name: &str, default: u32) -> u32 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result_with_status(status_code: Option<i32>) -> PollResult<()> {
        PollResult {
            value: None,
            success: false,
            status_code,
            latency_ms: 0,
            error_message: None,
        }
    }

    #[test]
    fn does_not_retry_individual_waits_for_access_denials_or_rate_limits() {
        for status in [Some(403), Some(405), Some(429)] {
            assert!(!should_retry_individual_waits(&result_with_status(status)));
        }
    }

    #[test]
    fn retries_individual_waits_for_unknown_failures() {
        assert!(should_retry_individual_waits(&result_with_status(None)));
        assert!(should_retry_individual_waits(&result_with_status(Some(
            500
        ))));
    }

    #[test]
    fn trips_provider_cooldown_for_access_denials_or_rate_limits() {
        for status in [Some(403), Some(405), Some(429)] {
            assert!(should_trip_provider_cooldown(&result_with_status(status)));
        }
        assert!(!should_trip_provider_cooldown(&result_with_status(Some(
            500
        ))));
        assert!(!should_trip_provider_cooldown(&result_with_status(None)));
    }
}
