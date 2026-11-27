-- Seed script for the Parquet files of examples/parquet-report: synthetic orders of a
-- fictional retailer, one million rows, and its forty stores.
-- Run with DuckDB from this report folder: `duckdb < data/seed.sql`.
-- Every value is arithmetic on the row number, with no random function, so each run writes
-- the same rows.

-- A spread of the row number over 0 to 1,000,002, different for each multiplier.
create or replace temp macro spread(i, multiplier) as ((i + 1) * multiplier) % 1000003;
create or replace temp macro pick(items, n) as items[cast(n % len(items) as bigint) + 1];

create or replace temp table stores as
select
    s + 1 as store_id,
    'Store ' || lpad(cast(s + 1 as varchar), 2, '0') as store,
    -- Ten cities, two in each region, so a city is always in its own region.
    pick(['North', 'South', 'East', 'West', 'Central', 'North', 'South', 'East', 'West', 'Central'], s) as region,
    pick(['Liberec', 'Brno', 'Ostrava', 'Plzeň', 'Praha', 'Ústí nad Labem', 'České Budějovice', 'Olomouc', 'Karlovy Vary', 'Jihlava'], s) as city,
    date '2012-03-01' + cast(spread(s, 7919) % 4000 as integer) as opened,
    -- Many small stores and a few large ones.
    cast(280 + (spread(s, 104729) % 2200) * (spread(s, 104729) % 2200) // 2200 as integer) as floor_area_m2,
    -- A store's pull on orders: larger stores sell more, with a spread.
    (500 + (spread(s, 104729) % 2200) * (spread(s, 104729) % 2200) // 2200) * (60 + spread(s, 1299709) % 80) as pull,
    -- How much of the catalogue a store carries: the dearer lines are in the larger stores.
    200 + 40 * (spread(s, 15487469) % 4) as lines
from range(40) t(s);

create or replace temp table products as
select
    p + 1 as product_id,
    pick(['Grocery', 'Home', 'Garden', 'Toys', 'Sport', 'Media', 'Fashion', 'Beauty'], p) as category,
    -- Prices from 4.99 to about 300, dearer in the later categories.
    cast(5 + (spread(p, 15485863) % 1000) * (spread(p, 15485863) % 1000) / 4200 + (p % 8) * 9 + p / 4 - 0.01 as decimal(9, 2)) as unit_price,
    -- Days a category adds to a delivery, and its returns in a thousand orders.
    pick([0, 3, 4, 1, 2, 0, 1, 1], p) + (spread(p, 32452867) % 3 = 0)::int as slow_days,
    pick([22, 47, 44, 65, 52, 31, 95, 38], p) as returns
from range(320) t(p);

create or replace temp table store_pick as
select store_id, sum(pull) over (order by store_id) * 1000003 / sum(pull) over () as upto from stores;

create or replace temp table orders as
with numbered as (
    select
        i,
        spread(i, 2654435761) as n_day,
        spread(i, 40503) as n_season,
        spread(i, 6700417) as n_store,
        spread(i, 179424673) as n_product,
        spread(i, 32452843) as n_units,
        spread(i, 49979687) as n_channel,
        spread(i, 67867967) as n_discount,
        spread(i, 86028121) as n_delivery,
        spread(i, 104395301) as n_age,
        spread(i, 122949823) as n_rating,
        spread(i, 141650939) as n_return
    from range(1000000) t(i)
),
dated as (
    -- Two years of days, with more orders as the period goes on.
    select *, date '2024-01-01' + cast(floor(730 * pow(n_day / 1000003.0, 0.82)) as integer) as spread_day
    from numbered
),
filled as (
    -- One order in six moves into the last quarter of its year: the season before Christmas.
    select
        * exclude (spread_day),
        case
            when n_season % 6 = 0 then make_date(year(spread_day), 10, 1) + cast(n_season % 85 as integer)
            else spread_day
        end as season_day
    from dated
),
weekly as (
    -- One Tuesday or Wednesday order in three moves to the Saturday before.
    select
        * exclude (season_day),
        case
            when isodow(season_day) in (2, 3) and n_season % 3 = 1 and season_day >= date '2024-01-08'
                then season_day - cast(isodow(season_day) + 1 as integer)
            else season_day
        end as day
    from filled
),
placed as (
    select
        f.*,
        s.store_id, s.region, s.city, s.store, s.floor_area_m2,
        p.category, p.unit_price, p.slow_days, p.returns,
        -- Months since January 2024: the web's share of orders grows with them.
        case
            when f.n_channel % 2000 < 760 + 15 * ((year(f.day) - 2024) * 12 + month(f.day) - 1) then 'Web'
            when f.n_channel % 2000 < 1760 then 'Store'
            else 'Partner'
        end as channel,
        -- One to six items: more in the larger stores, and each store has its own habit.
        1 + (f.n_units % 100 >= 55)::int + (f.n_units % 100 >= 80)::int + (f.n_units % 100 >= 92)::int
          + (f.n_units % 100 >= 97)::int * 2 + (s.floor_area_m2 >= 1200 and f.n_units % 4 = 0)::int
          + (f.n_units % 23 < s.store_id % 7)::int as units
    from weekly f
    asof join store_pick sp on f.n_store <= sp.upto
    join stores s on s.store_id = sp.store_id
    join products p on p.product_id = f.n_product % s.lines + 1
),
delivered as (
    select
        *,
        1 + n_delivery % 3 + (n_delivery % 7 = 0)::int * 3 + slow_days + (channel = 'Partner')::int as delivery_days
    from placed
)
select
    row_number() over (order by day, i) as order_id,
    day,
    region,
    city,
    store,
    channel,
    category,
    cast(units as tinyint) as units,
    unit_price,
    cast(unit_price * units as decimal(12, 2)) as revenue,
    cast(pick([0, 0, 0, 0, 0, 0, 0.05, 0.10, 0.10, 0.20], n_discount) as decimal(3, 2)) as discount,
    cast(delivery_days as tinyint) as delivery_days,
    cast(18 + n_age % 35 + (n_age // 64) % 28 as tinyint) as customer_age,
    -- A late delivery costs a star in every second order.
    cast(greatest(1, pick([5, 5, 5, 4, 4, 4, 4, 3, 2, 1], n_rating) - (delivery_days >= 6 and n_rating % 2 = 0)::int) as tinyint) as rating,
    (n_return % 1000) < returns + 18 * (channel = 'Web')::int + 3 * (store_id % 9) as returned,
    'batch ' || lpad(cast(i % 50 as varchar), 2, '0') as import_batch
from delivered;

copy (select order_id, day, region, city, store, channel, category, revenue, delivery_days, rating, returned
      from orders order by order_id)
to 'data/orders.parquet' (format parquet, parquet_version v2, compression zstd, compression_level 19, row_group_size 250000);

copy (select store, city, region, opened, floor_area_m2 from stores order by store)
to 'data/stores.parquet' (format parquet, compression zstd);
