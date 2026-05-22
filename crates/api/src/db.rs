use crate::error::ApiError;
use chrono::{DateTime, Datelike, TimeDelta, TimeZone, Utc};
use chrono_tz::{America::Los_Angeles, Tz};
use heytea_core::{
    ClosingNoticeResponse, HistoryComparisonPoint, HistoryPoint, HistoryRange, HistoryResponse,
    LocationResponse, LocationsResponse, NoticeResponse, StatusResponse, WaitTimeResponse,
    DEFAULT_STALE_AFTER_SECONDS,
};
use sqlx::FromRow;
use std::collections::BTreeMap;

#[derive(Debug, FromRow)]
struct LocationRow {
    slug: String,
    name: String,
    address: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    timezone: String,
    is_managed: bool,
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
          true as is_managed,
          l.is_enabled,
          l.support_takeaway,
          coalesce(s.is_open, l.is_open) as is_open,
          s.pickup_wait_minutes,
          s.wait_observed_at as observed_at
        from locations l
        left join location_current_status s on s.shop_id = l.shop_id
        where coalesce(l.is_enabled, true) is true
        order by
          coalesce(s.is_open, l.is_open) desc nulls last,
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
          true as is_managed,
          l.is_enabled,
          l.support_takeaway,
          coalesce(s.is_open, l.is_open) as is_open,
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
        is_managed: row.is_managed,
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
          coalesce(s.is_open, l.is_open) as is_open,
          s.pickup_wait_minutes,
          s.delivery_estimate_minutes,
          s.making_cups,
          s.making_orders,
          s.is_estimate,
          s.wait_text as text,
          coalesce(s.notices, '{}'::text[]) as notices,
          coalesce(s.closing_notices, '{}'::text[]) as closing_notices,
          s.wait_observed_at as observed_at,
          s.notices_observed_at,
          s.closing_notices_observed_at
        from locations l
        left join location_current_status s on s.shop_id = l.shop_id
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
          and to_regclass('public.managed_regions') is not null
          and to_regclass('public.managed_locations') is not null
          and to_regclass('public.upstream_provider_cooldowns') is not null
          and exists (select 1 from pg_extension where extname = 'timescaledb')
        "#,
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

const MAX_INTERPOLATION_GAP_SECONDS: i64 = 20 * 60;

#[derive(Debug, Clone, FromRow)]
struct WaitSample {
    observed_at: DateTime<Utc>,
    pickup_wait_minutes: Option<f64>,
    delivery_estimate_minutes: Option<f64>,
    making_cups: Option<f64>,
    making_orders: Option<f64>,
}

pub async fn history_for_slug(
    pool: &sqlx::PgPool,
    slug: &str,
    range: HistoryRange,
) -> Result<HistoryResponse, ApiError> {
    let location = location_row(pool, slug).await?;
    let timezone = location.timezone.parse::<Tz>().unwrap_or(Los_Angeles);
    let now = Utc::now();
    let start = history_start(now, range, timezone);
    let sample_start = start - TimeDelta::seconds(MAX_INTERPOLATION_GAP_SECONDS);
    let samples = if matches!(range, HistoryRange::SevenDays) {
        aggregate_wait_samples(pool, slug, sample_start, now).await?
    } else {
        raw_wait_samples(pool, slug, sample_start, now).await?
    };
    let points = interpolated_history_points(&samples, floor_minute(start), floor_minute(now));
    let comparison_points = if matches!(range, HistoryRange::Today) {
        interpolated_comparison_points(pool, slug, timezone, floor_minute(start)).await?
    } else {
        Vec::new()
    };

    Ok(HistoryResponse {
        range: range.to_string(),
        generated_at: Utc::now(),
        points,
        comparison_points,
    })
}

async fn raw_wait_samples(
    pool: &sqlx::PgPool,
    slug: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<WaitSample>, ApiError> {
    Ok(sqlx::query_as::<_, WaitSample>(
        r#"
        with location as (
          select shop_id from locations where slug = $1
        )
        select
          w.observed_at,
          w.pickup_wait_minutes::float8 as pickup_wait_minutes,
          w.delivery_estimate_minutes::float8 as delivery_estimate_minutes,
          w.making_cups::float8 as making_cups,
          w.making_orders::float8 as making_orders
        from location_wait_time_observations w, location
        where w.shop_id = location.shop_id
          and w.observed_at >= $2
          and w.observed_at <= $3
        order by w.observed_at asc
        "#,
    )
    .bind(slug)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?)
}

async fn aggregate_wait_samples(
    pool: &sqlx::PgPool,
    slug: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<WaitSample>, ApiError> {
    Ok(sqlx::query_as::<_, WaitSample>(
        r#"
        with location as (
          select shop_id from locations where slug = $1
        )
        select
          w.bucket as observed_at,
          w.avg_pickup_wait_minutes as pickup_wait_minutes,
          w.avg_delivery_estimate_minutes as delivery_estimate_minutes,
          w.avg_making_cups as making_cups,
          w.avg_making_orders as making_orders
        from location_wait_time_1m w, location
        where w.shop_id = location.shop_id
          and w.bucket >= $2
          and w.bucket <= $3
        order by w.bucket asc
        "#,
    )
    .bind(slug)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?)
}

async fn interpolated_comparison_points(
    pool: &sqlx::PgPool,
    slug: &str,
    timezone: Tz,
    today_start: DateTime<Utc>,
) -> Result<Vec<HistoryComparisonPoint>, ApiError> {
    let comparison_start = today_start - TimeDelta::days(7);
    let samples = aggregate_wait_samples(
        pool,
        slug,
        comparison_start - TimeDelta::seconds(MAX_INTERPOLATION_GAP_SECONDS),
        today_start,
    )
    .await?;
    let today = today_start.with_timezone(&timezone).date_naive();
    let mut by_day = BTreeMap::new();
    for sample in samples {
        let day = sample.observed_at.with_timezone(&timezone).date_naive();
        if day < today {
            by_day.entry(day).or_insert_with(Vec::new).push(sample);
        }
    }

    let mut buckets = vec![Vec::<f64>::new(); 1440];
    for (day, mut samples) in by_day {
        samples.sort_by_key(|sample| sample.observed_at);
        let day_start = local_midnight_utc(timezone, day);
        let values = interpolated_pickup_values(&samples, day_start, 1440);
        for (minute, value) in values.into_iter().enumerate() {
            if let Some(value) = value {
                buckets[minute].push(value);
            }
        }
    }

    Ok(buckets
        .into_iter()
        .enumerate()
        .filter_map(|(minute, values)| {
            (!values.is_empty()).then(|| HistoryComparisonPoint {
                minute_of_day: minute as i32,
                avg_pickup_wait_minutes: Some(values.iter().sum::<f64>() / values.len() as f64),
                sample_count: values.len() as i64,
            })
        })
        .collect())
}

fn interpolated_history_points(
    samples: &[WaitSample],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Vec<HistoryPoint> {
    let mut points = Vec::new();
    let mut cursor = start;
    let mut index = 0usize;
    while cursor <= end {
        while index < samples.len() && samples[index].observed_at < cursor {
            index += 1;
        }
        let before = index.checked_sub(1).and_then(|index| samples.get(index));
        let after = samples.get(index);
        let pickup = interpolate_value(before, after, cursor, |sample| sample.pickup_wait_minutes);
        let rounded_pickup = pickup.map(|value| value.round() as i32);
        points.push(HistoryPoint {
            start: cursor,
            end: cursor + TimeDelta::minutes(1),
            avg_pickup_wait_minutes: pickup,
            min_pickup_wait_minutes: rounded_pickup,
            max_pickup_wait_minutes: rounded_pickup,
            avg_delivery_estimate_minutes: interpolate_value(before, after, cursor, |sample| {
                sample.delivery_estimate_minutes
            }),
            avg_making_cups: interpolate_value(before, after, cursor, |sample| sample.making_cups),
            avg_making_orders: interpolate_value(before, after, cursor, |sample| {
                sample.making_orders
            }),
            sample_count: pickup.is_some() as i64,
        });
        cursor += TimeDelta::minutes(1);
    }
    points
}

fn interpolated_pickup_values(
    samples: &[WaitSample],
    start: DateTime<Utc>,
    minutes: usize,
) -> Vec<Option<f64>> {
    let mut values = Vec::with_capacity(minutes);
    let mut index = 0usize;
    for minute in 0..minutes {
        let target = start + TimeDelta::minutes(minute as i64);
        while index < samples.len() && samples[index].observed_at < target {
            index += 1;
        }
        let before = index.checked_sub(1).and_then(|index| samples.get(index));
        let after = samples.get(index);
        values.push(interpolate_value(before, after, target, |sample| {
            sample.pickup_wait_minutes
        }));
    }
    values
}

fn interpolate_value(
    before: Option<&WaitSample>,
    after: Option<&WaitSample>,
    target: DateTime<Utc>,
    value: impl Fn(&WaitSample) -> Option<f64>,
) -> Option<f64> {
    let before_value = before.and_then(&value);
    let after_value = after.and_then(&value);
    match (before_value, after_value, before, after) {
        (Some(before_value), Some(after_value), Some(before), Some(after)) => {
            let gap = after
                .observed_at
                .signed_duration_since(before.observed_at)
                .num_seconds();
            if gap < 0 || gap > MAX_INTERPOLATION_GAP_SECONDS {
                return None;
            }
            if gap == 0 {
                return Some(before_value);
            }
            let offset = target
                .signed_duration_since(before.observed_at)
                .num_seconds()
                .clamp(0, gap) as f64;
            Some(before_value + (after_value - before_value) * offset / gap as f64)
        }
        (Some(before_value), None, Some(before), _) => (target
            .signed_duration_since(before.observed_at)
            .num_seconds()
            <= MAX_INTERPOLATION_GAP_SECONDS)
            .then_some(before_value),
        (None, Some(after_value), _, Some(after)) => (after
            .observed_at
            .signed_duration_since(target)
            .num_seconds()
            <= MAX_INTERPOLATION_GAP_SECONDS)
            .then_some(after_value),
        _ => None,
    }
}

fn history_start(now: DateTime<Utc>, range: HistoryRange, timezone: Tz) -> DateTime<Utc> {
    match range {
        HistoryRange::Today => {
            let today = now.with_timezone(&timezone).date_naive();
            local_midnight_utc(timezone, today)
        }
        HistoryRange::OneHour => now - TimeDelta::hours(1),
        HistoryRange::SixHours => now - TimeDelta::hours(6),
        HistoryRange::OneDay => now - TimeDelta::hours(24),
        HistoryRange::SevenDays => now - TimeDelta::days(7),
    }
}

fn local_midnight_utc(timezone: Tz, date: chrono::NaiveDate) -> DateTime<Utc> {
    timezone
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
        .earliest()
        .map(|value| value.with_timezone(&Utc))
        .unwrap_or_else(Utc::now)
}

fn floor_minute(value: DateTime<Utc>) -> DateTime<Utc> {
    value
        - TimeDelta::seconds(value.timestamp().rem_euclid(60))
        - TimeDelta::nanoseconds(i64::from(value.timestamp_subsec_nanos() % 1_000_000_000))
}
