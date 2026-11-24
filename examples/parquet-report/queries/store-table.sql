select store,
       city,
       region,
       count(*) as orders,
       cast(round(sum(revenue)) as bigint) as revenue,
       cast(round(avg(revenue)) as integer) as order_value,
       cast(round(1000.0 * count(*) filter (where returned) / count(*)) as integer) as returned_per_thousand
from orders
group by store, city, region
order by revenue desc
limit 12;
