-- Canonical schema for the singleton heytea.dev status platform.
-- TimescaleDB is required for hypertables and continuous aggregates.

create extension if not exists timescaledb;

create table if not exists store_metadata (
  singleton boolean primary key default true check (singleton),
  name text not null default 'Downtown Metreon',
  address text not null default '165 4th St, San Francisco, CA 94103',
  latitude double precision,
  longitude double precision,
  timezone text not null default 'America/Los_Angeles',
  is_enabled boolean,
  support_takeaway boolean,
  hours jsonb not null default '[]'::jsonb,
  observed_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create table if not exists current_status (
  singleton boolean primary key default true check (singleton),
  is_open boolean,
  pickup_wait_minutes integer,
  delivery_estimate_minutes integer,
  making_cups integer,
  making_orders integer,
  is_estimate boolean,
  notice text,
  closing_notice text,
  observed_at timestamptz not null,
  updated_at timestamptz not null default now()
);

create table if not exists wait_time_observations (
  observed_at timestamptz not null,
  pickup_wait_minutes integer,
  delivery_estimate_minutes integer,
  making_cups integer,
  making_orders integer,
  is_estimate boolean,
  is_open boolean,
  poll_run_id bigint
);

select create_hypertable('wait_time_observations', by_range('observed_at'), if_not_exists => true);

create index if not exists wait_time_observations_observed_at_idx
  on wait_time_observations (observed_at desc);

create table if not exists notice_observations (
  observed_at timestamptz not null,
  notice text,
  poll_run_id bigint
);

select create_hypertable('notice_observations', by_range('observed_at'), if_not_exists => true);

create table if not exists closing_notice_observations (
  observed_at timestamptz not null,
  closing_notice text,
  poll_run_id bigint
);

select create_hypertable('closing_notice_observations', by_range('observed_at'), if_not_exists => true);

create table if not exists poll_runs (
  id bigserial,
  endpoint text not null,
  started_at timestamptz not null default now(),
  finished_at timestamptz,
  success boolean,
  latency_ms integer,
  status_code integer,
  error_message text,
  primary key (id, started_at)
);

select create_hypertable('poll_runs', by_range('started_at'), if_not_exists => true);

create index if not exists poll_runs_endpoint_started_at_idx
  on poll_runs (endpoint, started_at desc);

create table if not exists upstream_endpoint_health (
  observed_at timestamptz not null default now(),
  endpoint text not null,
  success boolean not null,
  latency_ms integer,
  status_code integer,
  error_message text
);

select create_hypertable('upstream_endpoint_health', by_range('observed_at'), if_not_exists => true);

create index if not exists upstream_endpoint_health_endpoint_observed_at_idx
  on upstream_endpoint_health (endpoint, observed_at desc);

create table if not exists status_events (
  observed_at timestamptz not null default now(),
  event_type text not null,
  previous_value jsonb,
  new_value jsonb
);

select create_hypertable('status_events', by_range('observed_at'), if_not_exists => true);

create materialized view if not exists wait_time_5m
with (timescaledb.continuous) as
select
  time_bucket('5 minutes', observed_at) as bucket,
  avg(pickup_wait_minutes)::float8 as avg_pickup_wait_minutes,
  min(pickup_wait_minutes) as min_pickup_wait_minutes,
  max(pickup_wait_minutes) as max_pickup_wait_minutes,
  avg(delivery_estimate_minutes)::float8 as avg_delivery_estimate_minutes,
  avg(making_cups)::float8 as avg_making_cups,
  avg(making_orders)::float8 as avg_making_orders,
  count(*)::int8 as sample_count
from wait_time_observations
group by 1;

create materialized view if not exists wait_time_1h
with (timescaledb.continuous) as
select
  time_bucket('1 hour', observed_at) as bucket,
  avg(pickup_wait_minutes)::float8 as avg_pickup_wait_minutes,
  min(pickup_wait_minutes) as min_pickup_wait_minutes,
  max(pickup_wait_minutes) as max_pickup_wait_minutes,
  avg(delivery_estimate_minutes)::float8 as avg_delivery_estimate_minutes,
  avg(making_cups)::float8 as avg_making_cups,
  avg(making_orders)::float8 as avg_making_orders,
  count(*)::int8 as sample_count
from wait_time_observations
group by 1;

create materialized view if not exists wait_time_1d
with (timescaledb.continuous) as
select
  time_bucket('1 day', observed_at) as bucket,
  avg(pickup_wait_minutes)::float8 as avg_pickup_wait_minutes,
  min(pickup_wait_minutes) as min_pickup_wait_minutes,
  max(pickup_wait_minutes) as max_pickup_wait_minutes,
  avg(delivery_estimate_minutes)::float8 as avg_delivery_estimate_minutes,
  avg(making_cups)::float8 as avg_making_cups,
  avg(making_orders)::float8 as avg_making_orders,
  count(*)::int8 as sample_count
from wait_time_observations
group by 1;

insert into store_metadata (singleton)
values (true)
on conflict (singleton) do nothing;

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
