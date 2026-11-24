select category, sum(revenue) as revenue
from orders
group by category
order by revenue desc;
