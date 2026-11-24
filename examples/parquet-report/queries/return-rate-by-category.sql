select category, round(100.0 * count(*) filter (where returned) / count(*), 1) as return_rate
from orders
group by category
order by return_rate desc;
