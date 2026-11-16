-- Seed script for examples/sql-report/data/warehouse.duckdb.
-- Run with: `duckdb data/warehouse.duckdb < data/seed.sql` from this report folder.
-- The warehouse file itself is not committed; it is produced locally on demand.

create table if not exists monthly_orders (
    month text primary key,
    orders integer not null
);

delete from monthly_orders;
insert into monthly_orders (month, orders) values
    ('Jul', 120),
    ('Aug', 150),
    ('Sep', 180);
