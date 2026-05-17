-- Canonical location-first schema for heytea.dev.
-- This migration assumes a fresh database. Existing production data can be
-- dropped before applying because the app can rediscover the full catalog.

create extension if not exists timescaledb;

create table if not exists locations (
  shop_id bigint primary key,
  slug text not null unique,
  provider text not null,
  region_code text,
  country_code text not null,
  city_code text,
  name text not null,
  address text not null,
  latitude double precision,
  longitude double precision,
  timezone text not null,
  is_enabled boolean,
  is_open boolean,
  support_takeaway boolean,
  hours jsonb not null default '[]'::jsonb,
  observed_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create unique index if not exists locations_slug_idx
  on locations (slug);

create index if not exists locations_country_city_idx
  on locations (country_code, city_code);

create table if not exists location_current_status (
  shop_id bigint primary key references locations (shop_id) on delete cascade,
  is_open boolean,
  pickup_wait_minutes integer,
  delivery_estimate_minutes integer,
  making_cups integer,
  making_orders integer,
  is_estimate boolean,
  wait_text text,
  wait_observed_at timestamptz,
  notices text[] not null default '{}'::text[],
  closing_notices text[] not null default '{}'::text[],
  notices_observed_at timestamptz,
  closing_notices_observed_at timestamptz,
  updated_at timestamptz not null default now()
);

create table if not exists location_wait_time_observations (
  observed_at timestamptz not null,
  shop_id bigint not null references locations (shop_id) on delete cascade,
  pickup_wait_minutes integer,
  delivery_estimate_minutes integer,
  making_cups integer,
  making_orders integer,
  is_estimate boolean,
  wait_text text
);

select create_hypertable('location_wait_time_observations', by_range('observed_at'), if_not_exists => true);
select set_chunk_time_interval('location_wait_time_observations', interval '1 hour');

create unique index if not exists location_wait_time_observations_shop_observed_at_idx
  on location_wait_time_observations (shop_id, observed_at);

alter table location_wait_time_observations
  set (timescaledb.compress, timescaledb.compress_segmentby = 'shop_id');

create table if not exists upstream_endpoint_health (
  observed_at timestamptz not null default now(),
  endpoint text not null,
  success boolean not null,
  latency_ms integer,
  status_code integer,
  error_message text
);

select create_hypertable('upstream_endpoint_health', by_range('observed_at'), if_not_exists => true);
select set_chunk_time_interval('upstream_endpoint_health', interval '1 day');

create index if not exists upstream_endpoint_health_endpoint_observed_at_idx
  on upstream_endpoint_health (endpoint, observed_at desc);

create materialized view if not exists location_wait_time_1m
with (timescaledb.continuous) as
select
  shop_id,
  time_bucket('1 minute', observed_at) as bucket,
  avg(pickup_wait_minutes)::float8 as avg_pickup_wait_minutes,
  min(pickup_wait_minutes) as min_pickup_wait_minutes,
  max(pickup_wait_minutes) as max_pickup_wait_minutes,
  avg(delivery_estimate_minutes)::float8 as avg_delivery_estimate_minutes,
  avg(making_cups)::float8 as avg_making_cups,
  avg(making_orders)::float8 as avg_making_orders,
  count(*)::int8 as sample_count
from location_wait_time_observations
group by 1, 2;

alter materialized view location_wait_time_1m
  set (timescaledb.materialized_only = false);

select add_continuous_aggregate_policy(
  'location_wait_time_1m',
  start_offset => interval '7 days',
  end_offset => interval '1 minute',
  schedule_interval => interval '1 minute',
  if_not_exists => true
);

select add_retention_policy('location_wait_time_observations', interval '24 hours', if_not_exists => true);
select add_retention_policy('location_wait_time_1m', interval '7 days', if_not_exists => true);
select add_retention_policy('upstream_endpoint_health', interval '7 days', if_not_exists => true);
select add_compression_policy('location_wait_time_observations', interval '2 hours', if_not_exists => true);

do $$
begin
  if exists (select 1 from pg_roles where rolname = 'heytea') then
    execute 'grant usage on schema public to heytea';
    execute 'grant select, insert, update, delete on all tables in schema public to heytea';
    execute 'grant usage, select on all sequences in schema public to heytea';
    execute 'alter default privileges in schema public grant select, insert, update, delete on tables to heytea';
    execute 'alter default privileges in schema public grant usage, select on sequences to heytea';
  end if;
end
$$;
