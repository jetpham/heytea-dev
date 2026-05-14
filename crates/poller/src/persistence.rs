use crate::upstream::{PollResult, ShopMetadata, WaitTime};
use chrono::{DateTime, Utc};

pub struct PersistedPoll {
    pub started: DateTime<Utc>,
    pub metadata: PollResult<ShopMetadata>,
    pub wait_time: PollResult<WaitTime>,
    pub notice: PollResult<Option<String>>,
    pub closing_notice: PollResult<Option<String>>,
}

pub async fn persist_poll(pool: &sqlx::PgPool, poll: PersistedPoll) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;

    record_health(&mut tx, "shop_metadata", &poll.metadata).await?;
    record_health(&mut tx, "wait_time", &poll.wait_time).await?;
    record_health(&mut tx, "notice", &poll.notice).await?;
    record_health(&mut tx, "closing_notice", &poll.closing_notice).await?;

    if let Some(metadata) = &poll.metadata.value {
        sqlx::query(
            r#"
            insert into store_metadata (
              singleton, name, address, latitude, longitude, is_enabled,
              support_takeaway, hours, observed_at, updated_at
            )
            values (true, $1, $2, $3, $4, $5, $6, $7, $8, now())
            on conflict (singleton) do update set
              name = excluded.name,
              address = excluded.address,
              latitude = excluded.latitude,
              longitude = excluded.longitude,
              is_enabled = excluded.is_enabled,
              support_takeaway = excluded.support_takeaway,
              hours = excluded.hours,
              observed_at = excluded.observed_at,
              updated_at = now()
            "#,
        )
        .bind(&metadata.name)
        .bind(&metadata.address)
        .bind(metadata.latitude)
        .bind(metadata.longitude)
        .bind(metadata.is_enabled)
        .bind(metadata.support_takeaway)
        .bind(&metadata.hours)
        .bind(metadata.observed_at)
        .execute(&mut *tx)
        .await?;
    }

    if let Some(wait) = &poll.wait_time.value {
        let is_open = poll
            .metadata
            .value
            .as_ref()
            .and_then(|metadata| metadata.is_open);
        sqlx::query(
            r#"
            insert into wait_time_observations (
              observed_at, pickup_wait_minutes, delivery_estimate_minutes,
              making_cups, making_orders, is_estimate, is_open
            )
            values ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(wait.observed_at)
        .bind(wait.pickup_wait_minutes)
        .bind(wait.delivery_estimate_minutes)
        .bind(wait.making_cups)
        .bind(wait.making_orders)
        .bind(wait.is_estimate)
        .bind(is_open)
        .execute(&mut *tx)
        .await?;

        let notice = poll.notice.value.clone().flatten();
        let closing_notice = poll.closing_notice.value.clone().flatten();
        sqlx::query(
            r#"
            insert into current_status (
              singleton, is_open, pickup_wait_minutes, delivery_estimate_minutes,
              making_cups, making_orders, is_estimate, notice, closing_notice,
              observed_at, updated_at
            )
            values (true, $1, $2, $3, $4, $5, $6, $7, $8, $9, now())
            on conflict (singleton) do update set
              is_open = excluded.is_open,
              pickup_wait_minutes = excluded.pickup_wait_minutes,
              delivery_estimate_minutes = excluded.delivery_estimate_minutes,
              making_cups = excluded.making_cups,
              making_orders = excluded.making_orders,
              is_estimate = excluded.is_estimate,
              notice = excluded.notice,
              closing_notice = excluded.closing_notice,
              observed_at = excluded.observed_at,
              updated_at = now()
            "#,
        )
        .bind(is_open)
        .bind(wait.pickup_wait_minutes)
        .bind(wait.delivery_estimate_minutes)
        .bind(wait.making_cups)
        .bind(wait.making_orders)
        .bind(wait.is_estimate)
        .bind(notice)
        .bind(closing_notice)
        .bind(wait.observed_at)
        .execute(&mut *tx)
        .await?;
    }

    if poll.notice.success {
        sqlx::query("insert into notice_observations (observed_at, notice) values ($1, $2)")
            .bind(poll.started)
            .bind(poll.notice.value.flatten())
            .execute(&mut *tx)
            .await?;
    }

    if poll.closing_notice.success {
        sqlx::query(
            "insert into closing_notice_observations (observed_at, closing_notice) values ($1, $2)",
        )
        .bind(poll.started)
        .bind(poll.closing_notice.value.flatten())
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

async fn record_health<T>(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    endpoint: &str,
    result: &PollResult<T>,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        insert into upstream_endpoint_health (
          endpoint, success, latency_ms, status_code, error_message
        )
        values ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(endpoint)
    .bind(result.success)
    .bind(result.latency_ms)
    .bind(result.status_code)
    .bind(&result.error_message)
    .execute(&mut **tx)
    .await?;

    Ok(())
}
