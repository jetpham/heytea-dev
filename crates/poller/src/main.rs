mod persistence;
mod upstream;

use anyhow::Context;
use chrono::{DateTime, Utc};
use heytea_core::read_shop_id;
use persistence::PersistedPoll;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::{env, time::Duration};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use upstream::HeyTeaClient;

const DEFAULT_SHOP_CONFIG_PATH: &str = "config/shop-id";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://heytea:heytea@localhost:5432/heytea".to_string());
    let config_path =
        env::var("HEYTEA_SHOP_CONFIG").unwrap_or_else(|_| DEFAULT_SHOP_CONFIG_PATH.to_string());
    let interval = env::var("HEYTEA_POLLER_INTERVAL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(60);

    let shop_id = read_shop_id(&config_path).context("failed to read configured shop id")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .context("failed to connect to postgres")?;
    let client = HeyTeaClient::new()?;

    tracing::info!(shop_id, interval, "starting heytea poller");

    loop {
        if let Err(error) = poll_once(&pool, &client, shop_id).await {
            tracing::error!(?error, "poll cycle failed");
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

async fn poll_once(pool: &sqlx::PgPool, client: &HeyTeaClient, shop_id: i64) -> anyhow::Result<()> {
    let started = chrono::Utc::now();
    let metadata = client.fetch_shop_metadata(shop_id).await;
    let wait_time = client.fetch_wait_time(shop_id).await;
    let notice = client.fetch_notice(shop_id).await;
    let closing_notice = client.fetch_closing_notice(shop_id).await;
    let notify_observed_at = wait_time.value.as_ref().map(|wait| wait.observed_at);

    let poll = PersistedPoll {
        started,
        metadata,
        wait_time,
        notice,
        closing_notice,
    };
    persistence::persist_poll(pool, poll).await?;
    if let Some(observed_at) = notify_observed_at {
        if let Err(error) = publish_status_updated(pool, observed_at).await {
            tracing::warn!(?error, "status notification publish failed");
        }
    }
    Ok(())
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
