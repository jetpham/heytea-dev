use crate::error::ApiError;
use chrono::{DateTime, TimeDelta, Utc};
use heytea_core::{
    ClosingNoticeResponse, HistoryPoint, HistoryRange, HistoryResponse, LocationResponse,
    LocationsResponse, NoticeResponse, StatusResponse, WaitTimeResponse,
    DEFAULT_STALE_AFTER_SECONDS,
};
use sqlx::FromRow;

#[derive(Debug, FromRow)]
struct LocationRow {
    slug: String,
    name: String,
    address: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    timezone: String,
    is_enabled: Option<bool>,
    support_takeaway: Option<bool>,
    is_open: Option<bool>,
    pickup_wait_minutes: Option<i32>,
    observed_at: Option<DateTime<Utc>>,
}

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
    text: Option<String>,
    notices: Vec<String>,
    closing_notices: Vec<String>,
    observed_at: Option<DateTime<Utc>>,
    notices_observed_at: Option<DateTime<Utc>>,
    closing_notices_observed_at: Option<DateTime<Utc>>,
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

pub async fn locations(pool: &sqlx::PgPool) -> Result<LocationsResponse, ApiError> {
    let rows = sqlx::query_as::<_, LocationRow>(
        r#"
        select
          l.slug,
          l.name,
          l.address,
          l.latitude,
          l.longitude,
          l.timezone,
          l.is_enabled,
          l.support_takeaway,
          s.is_open,
          s.pickup_wait_minutes,
          s.wait_observed_at as observed_at
        from locations l
        left join location_current_status s on s.shop_id = l.shop_id
        where coalesce(l.is_enabled, true) is true
        order by
          s.is_open desc nulls last,
          s.pickup_wait_minutes asc nulls last,
          l.name asc
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(LocationsResponse {
        generated_at: Utc::now(),
        locations: rows.into_iter().map(location_response).collect(),
    })
}

pub async fn location(pool: &sqlx::PgPool, slug: &str) -> Result<LocationResponse, ApiError> {
    let row = location_row(pool, slug).await?;
    Ok(location_response(row))
}

async fn location_row(pool: &sqlx::PgPool, slug: &str) -> Result<LocationRow, ApiError> {
    let row = sqlx::query_as::<_, LocationRow>(
        r#"
        select
          l.slug,
          l.name,
          l.address,
          l.latitude,
          l.longitude,
          l.timezone,
          l.is_enabled,
          l.support_takeaway,
          s.is_open,
          s.pickup_wait_minutes,
          s.wait_observed_at as observed_at
        from locations l
        left join location_current_status s on s.shop_id = l.shop_id
        where l.slug = $1
        limit 1
        "#,
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?;

    row.ok_or_else(|| ApiError::not_found("location not found"))
}

fn location_response(row: LocationRow) -> LocationResponse {
    LocationResponse {
        slug: row.slug,
        name: row.name,
        address: row.address,
        latitude: row.latitude,
        longitude: row.longitude,
        timezone: row.timezone,
        is_enabled: row.is_enabled,
        support_takeaway: row.support_takeaway,
        is_open: row.is_open,
        pickup_wait_minutes: row.pickup_wait_minutes,
        observed_at: row.observed_at,
        stale: row.observed_at.map(is_stale).unwrap_or(true),
        stale_after: row.observed_at.map(stale_after),
    }
}

async fn current_status_row(pool: &sqlx::PgPool, slug: &str) -> Result<CurrentStatusRow, ApiError> {
    let row = sqlx::query_as::<_, CurrentStatusRow>(
        r#"
        select
          l.name,
          l.address,
          s.is_open,
          s.pickup_wait_minutes,
          s.delivery_estimate_minutes,
          s.making_cups,
          s.making_orders,
          s.is_estimate,
          s.wait_text as text,
          s.notices,
          s.closing_notices,
          s.wait_observed_at as observed_at,
          s.notices_observed_at,
          s.closing_notices_observed_at
        from locations l
        join location_current_status s on s.shop_id = l.shop_id
        where l.slug = $1
        limit 1
        "#,
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) if row.observed_at.is_some() => Ok(row),
        Some(_) => Err(ApiError::not_ready("wait time has not been observed yet")),
        None => {
            location_row(pool, slug).await?;
            Err(ApiError::not_ready("status has not been observed yet"))
        }
    }
}

pub async fn status_for_slug(pool: &sqlx::PgPool, slug: &str) -> Result<StatusResponse, ApiError> {
    let row = current_status_row(pool, slug).await?;
    Ok(StatusResponse {
        name: row.name,
        address: row.address,
        is_open: row.is_open,
        pickup_wait_minutes: row.pickup_wait_minutes,
        delivery_estimate_minutes: row.delivery_estimate_minutes,
        making_cups: row.making_cups,
        making_orders: row.making_orders,
        is_estimate: row.is_estimate,
        text: row.text,
        notices: row.notices,
        closing_notices: row.closing_notices,
        observed_at: row.observed_at.expect("checked current status observed_at"),
        stale: is_stale(row.observed_at.expect("checked current status observed_at")),
        stale_after: stale_after(row.observed_at.expect("checked current status observed_at")),
    })
}

pub async fn wait_time_for_slug(
    pool: &sqlx::PgPool,
    slug: &str,
) -> Result<WaitTimeResponse, ApiError> {
    let row = current_status_row(pool, slug).await?;
    Ok(WaitTimeResponse {
        pickup_wait_minutes: row.pickup_wait_minutes,
        delivery_estimate_minutes: row.delivery_estimate_minutes,
        making_cups: row.making_cups,
        making_orders: row.making_orders,
        is_estimate: row.is_estimate,
        text: row.text,
        observed_at: row.observed_at.expect("checked current status observed_at"),
        stale: is_stale(row.observed_at.expect("checked current status observed_at")),
        stale_after: stale_after(row.observed_at.expect("checked current status observed_at")),
    })
}

pub async fn notice_for_slug(pool: &sqlx::PgPool, slug: &str) -> Result<NoticeResponse, ApiError> {
    let row = current_status_row(pool, slug).await?;
    Ok(NoticeResponse {
        notices: row.notices,
        observed_at: row.notices_observed_at,
        stale: row.notices_observed_at.map(is_stale).unwrap_or(true),
    })
}

pub async fn closing_notice_for_slug(
    pool: &sqlx::PgPool,
    slug: &str,
) -> Result<ClosingNoticeResponse, ApiError> {
    let row = current_status_row(pool, slug).await?;
    Ok(ClosingNoticeResponse {
        closing_notices: row.closing_notices,
        observed_at: row.closing_notices_observed_at,
        stale: row
            .closing_notices_observed_at
            .map(is_stale)
            .unwrap_or(true),
    })
}

pub async fn schema_ready(pool: &sqlx::PgPool) -> bool {
    sqlx::query_scalar::<_, bool>(
        r#"
        select
          to_regclass('public.locations') is not null
          and to_regclass('public.location_current_status') is not null
          and to_regclass('public.location_wait_time_observations') is not null
          and to_regclass('public.location_wait_time_1m') is not null
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

pub async fn history_for_slug(
    pool: &sqlx::PgPool,
    slug: &str,
    range: HistoryRange,
) -> Result<HistoryResponse, ApiError> {
    location_row(pool, slug).await?;

    let sql = match range {
        HistoryRange::Today => r#"
            with location as (
              select l.shop_id, l.timezone
              from locations l
              where l.slug = $1
            ), day_start as (
              select date_trunc('day', now() at time zone location.timezone) at time zone location.timezone as start_at
              from location
            )
            select
              time_bucket('1 minute'::interval, w.observed_at) as start,
              avg(w.pickup_wait_minutes)::float8 as avg_pickup_wait_minutes,
              min(w.pickup_wait_minutes) as min_pickup_wait_minutes,
              max(w.pickup_wait_minutes) as max_pickup_wait_minutes,
              avg(w.delivery_estimate_minutes)::float8 as avg_delivery_estimate_minutes,
              avg(w.making_cups)::float8 as avg_making_cups,
              avg(w.making_orders)::float8 as avg_making_orders,
              count(*)::int8 as sample_count
            from location_wait_time_observations w, day_start, location
            where w.shop_id = location.shop_id
              and w.observed_at >= day_start.start_at
            group by 1
            order by 1 asc
            "#
        .to_string(),
        HistoryRange::SevenDays => r#"
            with location as (
              select shop_id from locations where slug = $1
            )
            select
              w.bucket as start,
              w.avg_pickup_wait_minutes,
              w.min_pickup_wait_minutes,
              w.max_pickup_wait_minutes,
              w.avg_delivery_estimate_minutes,
              w.avg_making_cups,
              w.avg_making_orders,
              w.sample_count
            from location_wait_time_1m w, location
            where w.shop_id = location.shop_id
              and w.bucket >= now() - '7 days'::interval
            order by 1 asc
            "#
        .to_string(),
        _ => format!(
            r#"
            with location as (
              select shop_id from locations where slug = $1
            )
            select
              time_bucket('1 minute'::interval, w.observed_at) as start,
              avg(w.pickup_wait_minutes)::float8 as avg_pickup_wait_minutes,
              min(w.pickup_wait_minutes) as min_pickup_wait_minutes,
              max(w.pickup_wait_minutes) as max_pickup_wait_minutes,
              avg(w.delivery_estimate_minutes)::float8 as avg_delivery_estimate_minutes,
              avg(w.making_cups)::float8 as avg_making_cups,
              avg(w.making_orders)::float8 as avg_making_orders,
              count(*)::int8 as sample_count
            from location_wait_time_observations w, location
            where w.shop_id = location.shop_id
              and w.observed_at >= now() - '{}'::interval
            group by 1
            order by 1 asc
            "#,
            range.sql_interval()
        ),
    };

    let rows = sqlx::query_as::<_, HistoryPointRow>(&sql)
        .bind(slug)
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
