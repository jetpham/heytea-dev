use crate::error::ApiError;
use chrono::{DateTime, TimeDelta, Utc};
use heytea_core::{
    ClosingNoticeResponse, HistoryPoint, HistoryRange, HistoryResponse, NoticeResponse,
    StatusResponse, WaitTimeResponse, DEFAULT_STALE_AFTER_SECONDS,
};
use sqlx::FromRow;

#[derive(Debug, FromRow)]
struct CurrentStatusRow {
    name: String,
    address: String,
    is_open: Option<bool>,
    pickup_wait_minutes: Option<i32>,
    delivery_estimate_minutes: Option<i32>,
    making_cups: Option<i32>,
    making_orders: Option<i32>,
    is_estimate: Option<bool>,
    notice: Option<String>,
    closing_notice: Option<String>,
    observed_at: DateTime<Utc>,
}

pub fn stale_after(observed_at: DateTime<Utc>) -> DateTime<Utc> {
    observed_at + TimeDelta::seconds(DEFAULT_STALE_AFTER_SECONDS)
}

pub fn ttl_seconds(stale_after: DateTime<Utc>) -> i64 {
    (stale_after - Utc::now()).num_seconds().max(0)
}

fn is_stale(observed_at: DateTime<Utc>) -> bool {
    Utc::now() > stale_after(observed_at)
}

async fn current_status_row(pool: &sqlx::PgPool) -> Result<CurrentStatusRow, ApiError> {
    let row = sqlx::query_as::<_, CurrentStatusRow>(
        r#"
        select
          coalesce(m.name, 'Downtown Metreon') as name,
          coalesce(m.address, '165 4th St, San Francisco, CA 94103') as address,
          s.is_open,
          s.pickup_wait_minutes,
          s.delivery_estimate_minutes,
          s.making_cups,
          s.making_orders,
          s.is_estimate,
          s.notice,
          s.closing_notice,
          s.observed_at
        from current_status s
        left join store_metadata m on m.singleton = true
        where s.singleton = true
        limit 1
        "#,
    )
    .fetch_optional(pool)
    .await?;

    row.ok_or_else(|| ApiError::not_ready("status has not been observed yet"))
}

pub async fn status(pool: &sqlx::PgPool) -> Result<StatusResponse, ApiError> {
    let row = current_status_row(pool).await?;
    Ok(StatusResponse {
        name: row.name,
        address: row.address,
        is_open: row.is_open,
        pickup_wait_minutes: row.pickup_wait_minutes,
        delivery_estimate_minutes: row.delivery_estimate_minutes,
        making_cups: row.making_cups,
        making_orders: row.making_orders,
        notice: row.notice,
        closing_notice: row.closing_notice,
        observed_at: row.observed_at,
        stale: is_stale(row.observed_at),
        stale_after: stale_after(row.observed_at),
    })
}

pub async fn wait_time(pool: &sqlx::PgPool) -> Result<WaitTimeResponse, ApiError> {
    let row = current_status_row(pool).await?;
    Ok(WaitTimeResponse {
        pickup_wait_minutes: row.pickup_wait_minutes,
        delivery_estimate_minutes: row.delivery_estimate_minutes,
        making_cups: row.making_cups,
        making_orders: row.making_orders,
        is_estimate: row.is_estimate,
        observed_at: row.observed_at,
        stale: is_stale(row.observed_at),
        stale_after: stale_after(row.observed_at),
    })
}

pub async fn notice(pool: &sqlx::PgPool) -> Result<NoticeResponse, ApiError> {
    let row = current_status_row(pool).await?;
    Ok(NoticeResponse {
        notice: row.notice,
        observed_at: Some(row.observed_at),
        stale: is_stale(row.observed_at),
    })
}

pub async fn closing_notice(pool: &sqlx::PgPool) -> Result<ClosingNoticeResponse, ApiError> {
    let row = current_status_row(pool).await?;
    Ok(ClosingNoticeResponse {
        closing_notice: row.closing_notice,
        observed_at: Some(row.observed_at),
        stale: is_stale(row.observed_at),
    })
}

pub async fn schema_ready(pool: &sqlx::PgPool) -> bool {
    sqlx::query_scalar::<_, bool>(
        r#"
        select
          to_regclass('public.current_status') is not null
          and to_regclass('public.wait_time_observations') is not null
          and to_regclass('public.store_metadata') is not null
          and exists (select 1 from pg_extension where extname = 'timescaledb')
        "#,
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

#[derive(Debug, FromRow)]
struct HistoryPointRow {
    start: DateTime<Utc>,
    avg_pickup_wait_minutes: Option<f64>,
    min_pickup_wait_minutes: Option<i32>,
    max_pickup_wait_minutes: Option<i32>,
    avg_delivery_estimate_minutes: Option<f64>,
    avg_making_cups: Option<f64>,
    avg_making_orders: Option<f64>,
    sample_count: i64,
}

pub async fn history(
    pool: &sqlx::PgPool,
    range: HistoryRange,
) -> Result<HistoryResponse, ApiError> {
    let sql = match range {
        HistoryRange::Today => r#"
            with current as (
              select is_open from current_status where singleton = true limit 1
            ), day_start as (
              select date_trunc('day', now() at time zone 'America/Los_Angeles') at time zone 'America/Los_Angeles' as start_at
            ), last_closed as (
              select max(observed_at) as observed_at
              from wait_time_observations, day_start
              where observed_at >= day_start.start_at
                and is_open is distinct from true
            ), open_start as (
              select min(observed_at) as start_at
              from wait_time_observations, day_start, last_closed
              where observed_at >= day_start.start_at
                and is_open is true
                and (last_closed.observed_at is null or observed_at > last_closed.observed_at)
            )
            select
              time_bucket('1 minute'::interval, observed_at) as start,
              avg(pickup_wait_minutes)::float8 as avg_pickup_wait_minutes,
              min(pickup_wait_minutes) as min_pickup_wait_minutes,
              max(pickup_wait_minutes) as max_pickup_wait_minutes,
              avg(delivery_estimate_minutes)::float8 as avg_delivery_estimate_minutes,
              avg(making_cups)::float8 as avg_making_cups,
              avg(making_orders)::float8 as avg_making_orders,
              count(*)::int8 as sample_count
            from wait_time_observations, open_start, current
            where current.is_open is true
              and open_start.start_at is not null
              and observed_at >= open_start.start_at
              and is_open is true
            group by 1
            order by 1 asc
            "#
        .to_string(),
        _ => format!(
            r#"
            select
              time_bucket('1 minute'::interval, observed_at) as start,
              avg(pickup_wait_minutes)::float8 as avg_pickup_wait_minutes,
              min(pickup_wait_minutes) as min_pickup_wait_minutes,
              max(pickup_wait_minutes) as max_pickup_wait_minutes,
              avg(delivery_estimate_minutes)::float8 as avg_delivery_estimate_minutes,
              avg(making_cups)::float8 as avg_making_cups,
              avg(making_orders)::float8 as avg_making_orders,
              count(*)::int8 as sample_count
            from wait_time_observations
            where observed_at >= now() - '{}'::interval
            group by 1
            order by 1 asc
            "#,
            range.sql_interval()
        ),
    };

    let rows = sqlx::query_as::<_, HistoryPointRow>(&sql)
        .fetch_all(pool)
        .await?;

    Ok(HistoryResponse {
        range: range.to_string(),
        generated_at: Utc::now(),
        points: rows
            .into_iter()
            .map(|row| HistoryPoint {
                start: row.start,
                end: row.start + TimeDelta::minutes(1),
                avg_pickup_wait_minutes: row.avg_pickup_wait_minutes,
                min_pickup_wait_minutes: row.min_pickup_wait_minutes,
                max_pickup_wait_minutes: row.max_pickup_wait_minutes,
                avg_delivery_estimate_minutes: row.avg_delivery_estimate_minutes,
                avg_making_cups: row.avg_making_cups,
                avg_making_orders: row.avg_making_orders,
                sample_count: row.sample_count,
            })
            .collect(),
    })
}
