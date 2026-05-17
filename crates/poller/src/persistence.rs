use crate::upstream::{
    PollResult, ShopMetadata, ShopRef, StoreClosingNotices, StoreNotices, WaitTime,
};
use chrono::{DateTime, Utc};
use sqlx::QueryBuilder;

pub async fn active_shops(pool: &sqlx::PgPool) -> anyhow::Result<Vec<ShopRef>> {
    let rows = sqlx::query_as::<_, (i64, String, Option<String>)>(
        r#"
        select shop_id, country_code, city_code
        from locations
        where coalesce(is_enabled, true) is true
        order by shop_id
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(shop_id, country_code, city_code)| ShopRef {
            shop_id,
            country_code,
            city_code,
        })
        .collect())
}

pub async fn persist_catalog(
    pool: &sqlx::PgPool,
    catalog: PollResult<Vec<ShopMetadata>>,
) -> anyhow::Result<Vec<ShopRef>> {
    let mut tx = pool.begin().await?;
    record_health(&mut tx, "shop_catalog", &catalog).await?;

    let mut shop_refs = Vec::new();
    if let Some(shops) = &catalog.value {
        for shop in shops {
            let slug = stable_slug(&mut tx, shop.shop_id, &shop.slug).await?;
            sqlx::query(
                r#"
                insert into locations (
                  shop_id, slug, provider, region_code, country_code, city_code,
                  name, address, latitude, longitude, timezone, is_enabled, is_open,
                  support_takeaway, hours, observed_at, updated_at
                )
                values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, now())
                on conflict (shop_id) do update set
                  provider = excluded.provider,
                  region_code = excluded.region_code,
                  country_code = excluded.country_code,
                  city_code = excluded.city_code,
                  name = excluded.name,
                  address = excluded.address,
                  latitude = excluded.latitude,
                  longitude = excluded.longitude,
                  timezone = excluded.timezone,
                  is_enabled = excluded.is_enabled,
                  is_open = excluded.is_open,
                  support_takeaway = excluded.support_takeaway,
                  hours = excluded.hours,
                  observed_at = excluded.observed_at,
                  updated_at = now()
                "#,
            )
            .bind(shop.shop_id)
            .bind(&slug)
            .bind(&shop.provider)
            .bind(&shop.region_code)
            .bind(&shop.country_code)
            .bind(&shop.city_code)
            .bind(&shop.name)
            .bind(&shop.address)
            .bind(shop.latitude)
            .bind(shop.longitude)
            .bind(&shop.timezone)
            .bind(shop.is_enabled)
            .bind(shop.is_open)
            .bind(shop.support_takeaway)
            .bind(&shop.hours)
            .bind(shop.observed_at)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                r#"
                insert into location_current_status (shop_id, is_open, updated_at)
                values ($1, $2, now())
                on conflict (shop_id) do update set
                  is_open = excluded.is_open,
                  updated_at = now()
                "#,
            )
            .bind(shop.shop_id)
            .bind(shop.is_open)
            .execute(&mut *tx)
            .await?;

            shop_refs.push(shop.to_ref());
        }
    }

    tx.commit().await?;
    Ok(shop_refs)
}

pub async fn persist_wait_times(
    pool: &sqlx::PgPool,
    results: Vec<PollResult<Vec<WaitTime>>>,
) -> anyhow::Result<Option<DateTime<Utc>>> {
    let mut tx = pool.begin().await?;
    record_health_many(&mut tx, "wait_time", &results).await?;

    let waits = results
        .into_iter()
        .filter_map(|result| result.value)
        .flatten()
        .collect::<Vec<_>>();
    let observed_at = waits.iter().map(|wait| wait.observed_at).max();

    if !waits.is_empty() {
        insert_wait_observations(&mut tx, &waits).await?;
        upsert_current_waits(&mut tx, &waits).await?;
    }

    tx.commit().await?;
    Ok(observed_at)
}

pub async fn persist_notices(
    pool: &sqlx::PgPool,
    notices: Vec<PollResult<StoreNotices>>,
    closing_notices: Vec<PollResult<StoreClosingNotices>>,
) -> anyhow::Result<Option<DateTime<Utc>>> {
    let mut tx = pool.begin().await?;
    record_health_many(&mut tx, "notice", &notices).await?;
    record_health_many(&mut tx, "closing_notice", &closing_notices).await?;

    let notices = notices
        .into_iter()
        .filter_map(|result| result.value)
        .collect::<Vec<_>>();
    let closing_notices = closing_notices
        .into_iter()
        .filter_map(|result| result.value)
        .collect::<Vec<_>>();
    let observed_at = notices
        .iter()
        .map(|notice| notice.observed_at)
        .chain(closing_notices.iter().map(|notice| notice.observed_at))
        .max();

    if !notices.is_empty() {
        upsert_current_notices(&mut tx, &notices).await?;
    }
    if !closing_notices.is_empty() {
        upsert_current_closing_notices(&mut tx, &closing_notices).await?;
    }

    tx.commit().await?;
    Ok(observed_at)
}

async fn insert_wait_observations(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    waits: &[WaitTime],
) -> anyhow::Result<()> {
    let mut builder = QueryBuilder::<sqlx::Postgres>::new(
        r#"
        insert into location_wait_time_observations (
          observed_at, shop_id, pickup_wait_minutes, delivery_estimate_minutes,
          making_cups, making_orders, is_estimate, wait_text
        )
        "#,
    );
    builder.push_values(waits, |mut row, wait| {
        row.push_bind(wait.observed_at)
            .push_bind(wait.shop_id)
            .push_bind(wait.pickup_wait_minutes)
            .push_bind(wait.delivery_estimate_minutes)
            .push_bind(wait.making_cups)
            .push_bind(wait.making_orders)
            .push_bind(wait.is_estimate)
            .push_bind(&wait.text);
    });
    builder.push(" on conflict (shop_id, observed_at) do nothing");
    builder.build().execute(&mut **tx).await?;
    Ok(())
}

async fn upsert_current_waits(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    waits: &[WaitTime],
) -> anyhow::Result<()> {
    let mut builder = QueryBuilder::<sqlx::Postgres>::new(
        r#"
        insert into location_current_status (
          shop_id, pickup_wait_minutes, delivery_estimate_minutes, making_cups,
          making_orders, is_estimate, wait_text, wait_observed_at, updated_at
        )
        "#,
    );
    builder.push_values(waits, |mut row, wait| {
        row.push_bind(wait.shop_id)
            .push_bind(wait.pickup_wait_minutes)
            .push_bind(wait.delivery_estimate_minutes)
            .push_bind(wait.making_cups)
            .push_bind(wait.making_orders)
            .push_bind(wait.is_estimate)
            .push_bind(&wait.text)
            .push_bind(wait.observed_at)
            .push("now()");
    });
    builder.push(
        r#"
        on conflict (shop_id) do update set
          pickup_wait_minutes = excluded.pickup_wait_minutes,
          delivery_estimate_minutes = excluded.delivery_estimate_minutes,
          making_cups = excluded.making_cups,
          making_orders = excluded.making_orders,
          is_estimate = excluded.is_estimate,
          wait_text = excluded.wait_text,
          wait_observed_at = excluded.wait_observed_at,
          updated_at = now()
        "#,
    );
    builder.build().execute(&mut **tx).await?;
    Ok(())
}

async fn upsert_current_notices(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    notices: &[StoreNotices],
) -> anyhow::Result<()> {
    let mut builder = QueryBuilder::<sqlx::Postgres>::new(
        r#"
        insert into location_current_status (
          shop_id, notices, notices_observed_at, updated_at
        )
        "#,
    );
    builder.push_values(notices, |mut row, notice| {
        row.push_bind(notice.shop_id)
            .push_bind(&notice.notices)
            .push_bind(notice.observed_at)
            .push("now()");
    });
    builder.push(
        r#"
        on conflict (shop_id) do update set
          notices = excluded.notices,
          notices_observed_at = excluded.notices_observed_at,
          updated_at = now()
        "#,
    );
    builder.build().execute(&mut **tx).await?;
    Ok(())
}

async fn upsert_current_closing_notices(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    notices: &[StoreClosingNotices],
) -> anyhow::Result<()> {
    let mut builder = QueryBuilder::<sqlx::Postgres>::new(
        r#"
        insert into location_current_status (
          shop_id, closing_notices, closing_notices_observed_at, updated_at
        )
        "#,
    );
    builder.push_values(notices, |mut row, notice| {
        row.push_bind(notice.shop_id)
            .push_bind(&notice.closing_notices)
            .push_bind(notice.observed_at)
            .push("now()");
    });
    builder.push(
        r#"
        on conflict (shop_id) do update set
          closing_notices = excluded.closing_notices,
          closing_notices_observed_at = excluded.closing_notices_observed_at,
          updated_at = now()
        "#,
    );
    builder.build().execute(&mut **tx).await?;
    Ok(())
}

async fn stable_slug(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    shop_id: i64,
    suggested: &str,
) -> anyhow::Result<String> {
    if let Some(existing) =
        sqlx::query_scalar::<_, String>("select slug from locations where shop_id = $1")
            .bind(shop_id)
            .fetch_optional(&mut **tx)
            .await?
    {
        return Ok(existing);
    }

    let owner = sqlx::query_scalar::<_, i64>("select shop_id from locations where slug = $1")
        .bind(suggested)
        .fetch_optional(&mut **tx)
        .await?;
    if owner.is_some_and(|owner| owner != shop_id) {
        Ok(format!("{suggested}-{shop_id}"))
    } else {
        Ok(suggested.to_string())
    }
}

async fn record_health<T>(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    endpoint: &str,
    result: &PollResult<T>,
) -> anyhow::Result<()> {
    record_health_many(tx, endpoint, std::slice::from_ref(result)).await
}

async fn record_health_many<T>(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    endpoint: &str,
    results: &[PollResult<T>],
) -> anyhow::Result<()> {
    if results.is_empty() {
        return Ok(());
    }

    let mut builder = QueryBuilder::<sqlx::Postgres>::new(
        r#"
        insert into upstream_endpoint_health (
          endpoint, success, latency_ms, status_code, error_message
        )
        "#,
    );
    builder.push_values(results, |mut row, result| {
        row.push_bind(endpoint)
            .push_bind(result.success)
            .push_bind(result.latency_ms)
            .push_bind(result.status_code)
            .push_bind(&result.error_message);
    });
    builder.build().execute(&mut **tx).await?;
    Ok(())
}
