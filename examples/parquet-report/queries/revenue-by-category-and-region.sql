select region, category, cast(round(sum(revenue) / 1000) as integer) as revenue_thousands
from orders
group by region, category
order by region, category;
