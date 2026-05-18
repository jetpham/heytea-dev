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
    let upstream_concurrency = env::var("HEYTEA_UPSTREAM_CONCURRENCY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_UPSTREAM_CONCURRENCY);
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
        upstream_concurrency,
        max_connections,
        "starting heytea poller"
    );

    let mut last_catalog_poll = None;
    loop {
        let poll_catalog = last_catalog_poll
            .map(|last| {
                Utc::now().signed_duration_since(last).num_seconds() >= catalog_interval as i64
            })
            .unwrap_or(true);
        match poll_once(&pool, &client, poll_catalog, upstream_concurrency).await {
            Ok(catalog_success) => {
                if poll_catalog && catalog_success {
                    last_catalog_poll = Some(Utc::now());
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
    upstream_concurrency: usize,
) -> anyhow::Result<bool> {
    let mut catalog_success = !poll_catalog;
    let mut shops = if poll_catalog {
        let catalog = client.fetch_shop_catalog().await;
        catalog_success = catalog.success;
        persistence::persist_catalog(pool, catalog).await?
    } else {
        Vec::new()
    };
    if shops.is_empty() {
        shops = persistence::active_shops(pool).await?;
    }

    let wait_results = fetch_wait_results(client, &shops, upstream_concurrency).await;
    let wait_observed_at = persistence::persist_wait_times(pool, wait_results).await?;

    let (notice_results, closing_notice_results) =
        fetch_notice_results(client, &shops, upstream_concurrency).await;
    let notice_observed_at =
        persistence::persist_notices(pool, notice_results, closing_notice_results).await?;

    if let Some(observed_at) = wait_observed_at.into_iter().chain(notice_observed_at).max() {
        publish_status_updated(pool, observed_at).await?;
    }
    Ok(catalog_success)
}

async fn fetch_wait_results(
    client: &HeyTeaClient,
    shops: &[ShopRef],
    upstream_concurrency: usize,
) -> Vec<PollResult<Vec<WaitTime>>> {
    let jobs = group_shop_ids_by_provider(shops)
        .into_iter()
        .flat_map(|(provider, shop_ids)| {
            shop_ids
                .chunks(provider.batch_size())
                .map(move |batch| (provider, batch.to_vec()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    stream::iter(jobs)
        .map(|(provider, batch)| {
            let client = client.clone();
            async move { fetch_wait_job(client, provider, batch).await }
        })
        .buffer_unordered(upstream_concurrency)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect()
}

async fn fetch_wait_job(
    client: HeyTeaClient,
    provider: WaitProvider,
    batch: Vec<i64>,
) -> Vec<PollResult<Vec<WaitTime>>> {
    let result = client.fetch_wait_times(provider, &batch).await;
    if result.success || batch.len() <= 1 {
        return vec![result];
    }

    tracing::warn!(
        provider = ?provider,
        batch_size = batch.len(),
        "wait batch failed; retrying individual shops"
    );
    let mut results = vec![result];
    results.extend(
        stream::iter(batch)
            .map(|shop_id| {
                let client = client.clone();
                async move { client.fetch_wait_times(provider, &[shop_id]).await }
            })
            .buffer_unordered(8)
            .collect::<Vec<_>>()
            .await,
    );
    results
}

async fn fetch_notice_results(
    client: &HeyTeaClient,
    shops: &[ShopRef],
    upstream_concurrency: usize,
) -> (
    Vec<PollResult<StoreNotices>>,
    Vec<PollResult<StoreClosingNotices>>,
) {
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
        .collect::<Vec<_>>();

    let results = stream::iter(jobs)
        .map(|job| {
            let client = client.clone();
            async move {
                match job {
                    NoticeJob::Notice { provider, shop_id } => {
                        NoticeJobResult::Notice(client.fetch_notice(provider, shop_id).await)
                    }
                    NoticeJob::Closing { provider, shop_id } => NoticeJobResult::Closing(
                        client.fetch_closing_notice(provider, shop_id).await,
                    ),
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
            NoticeJobResult::Notice(result) => notices.push(result),
            NoticeJobResult::Closing(result) => closing_notices.push(result),
        }
    }
    (notices, closing_notices)
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
    Notice(PollResult<StoreNotices>),
    Closing(PollResult<StoreClosingNotices>),
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
