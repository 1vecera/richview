select category, round(avg(delivery_days), 1) as delivery_days
from orders
group by category
order by avg(delivery_days) desc;
