-- Managed tracking and upstream safety rails.

create table if not exists managed_regions (
  slug text primary key,
  label text not null,
  latitude double precision not null,
  longitude double precision not null,
  radius_miles double precision not null,
  is_enabled boolean not null default true,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

insert into managed_regions (slug, label, latitude, longitude, radius_miles, is_enabled)
values
  ('sf-bay-area', 'San Francisco Bay Area', 37.7749, -122.4194, 60, true),
  ('london', 'London', 51.5074, -0.1278, 60, true)
on conflict (slug) do nothing;

create table if not exists managed_location_overrides (
  shop_id bigint primary key references locations (shop_id) on delete cascade,
  is_managed boolean not null,
  note text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create table if not exists managed_locations (
  shop_id bigint primary key references locations (shop_id) on delete cascade,
  source text not null,
  reason text,
  is_active boolean not null default true,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create index if not exists managed_locations_active_idx
  on managed_locations (is_active, shop_id);

create table if not exists upstream_provider_cooldowns (
  endpoint text not null,
  provider_key text not null,
  status_code integer,
  error_message text,
  cooldown_until timestamptz not null,
  observed_at timestamptz not null default now(),
  primary key (endpoint, provider_key)
);

create or replace function managed_distance_miles(
  lat1 double precision,
  lon1 double precision,
  lat2 double precision,
  lon2 double precision
) returns double precision
language sql
immutable
parallel safe
returns null on null input
as $$
  select 3958.7613 * 2 * asin(least(1.0, sqrt(
    power(sin(radians(($3 - $1) / 2)), 2) +
    cos(radians($1)) * cos(radians($3)) * power(sin(radians(($4 - $2) / 2)), 2)
  )))
$$;

create or replace function refresh_managed_locations()
returns integer
language plpgsql
as $$
declare
  changed integer := 0;
  deactivated integer := 0;
begin
  with region_matches as (
    select
      l.shop_id,
      'region'::text as source,
      string_agg(r.label, ', ' order by r.label) as reason
    from locations l
    join managed_regions r on r.is_enabled
    left join managed_location_overrides o on o.shop_id = l.shop_id
    where coalesce(l.is_enabled, true) is true
      and l.latitude is not null
      and l.longitude is not null
      and coalesce(o.is_managed, true) is true
      and managed_distance_miles(r.latitude, r.longitude, l.latitude, l.longitude) <= r.radius_miles
    group by l.shop_id
  ), manual_matches as (
    select
      l.shop_id,
      'manual'::text as source,
      coalesce(o.note, 'manual override') as reason
    from managed_location_overrides o
    join locations l on l.shop_id = o.shop_id
    where o.is_managed is true
      and coalesce(l.is_enabled, true) is true
  ), desired as (
    select * from region_matches
    union all
    select * from manual_matches
  ), collapsed as (
    select
      shop_id,
      case when bool_or(source = 'manual') then 'manual' else 'region' end as source,
      string_agg(distinct reason, '; ' order by reason) as reason
    from desired
    group by shop_id
  )
  insert into managed_locations (shop_id, source, reason, is_active, updated_at)
  select shop_id, source, reason, true, now()
  from collapsed
  on conflict (shop_id) do update set
    source = excluded.source,
    reason = excluded.reason,
    is_active = true,
    updated_at = now()
  where managed_locations.source is distinct from excluded.source
    or managed_locations.reason is distinct from excluded.reason
    or managed_locations.is_active is distinct from true;

  get diagnostics changed = row_count;

  with region_matches as (
    select
      l.shop_id,
      'region'::text as source,
      string_agg(r.label, ', ' order by r.label) as reason
    from locations l
    join managed_regions r on r.is_enabled
    left join managed_location_overrides o on o.shop_id = l.shop_id
    where coalesce(l.is_enabled, true) is true
      and l.latitude is not null
      and l.longitude is not null
      and coalesce(o.is_managed, true) is true
      and managed_distance_miles(r.latitude, r.longitude, l.latitude, l.longitude) <= r.radius_miles
    group by l.shop_id
  ), manual_matches as (
    select
      l.shop_id,
      'manual'::text as source,
      coalesce(o.note, 'manual override') as reason
    from managed_location_overrides o
    join locations l on l.shop_id = o.shop_id
    where o.is_managed is true
      and coalesce(l.is_enabled, true) is true
  ), desired as (
    select * from region_matches
    union all
    select * from manual_matches
  ), collapsed as (
    select shop_id
    from desired
    group by shop_id
  )
  update managed_locations ml
  set is_active = false, updated_at = now()
  where ml.is_active is true
    and not exists (select 1 from collapsed c where c.shop_id = ml.shop_id);

  get diagnostics deactivated = row_count;

  return changed + deactivated;
end;
$$;

do $$
begin
  if exists (select 1 from pg_roles where rolname = 'heytea') then
    execute 'grant usage on schema public to heytea';
    execute 'grant select, insert, update, delete on managed_regions to heytea';
    execute 'grant select, insert, update, delete on managed_location_overrides to heytea';
    execute 'grant select, insert, update, delete on managed_locations to heytea';
    execute 'grant select, insert, update, delete on upstream_provider_cooldowns to heytea';
    execute 'grant execute on function managed_distance_miles(double precision, double precision, double precision, double precision) to heytea';
    execute 'grant execute on function refresh_managed_locations() to heytea';
  end if;
end
$$;
