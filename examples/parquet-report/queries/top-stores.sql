select store, sum(revenue) as revenue
from orders
group by store
order by revenue desc
limit 10;
