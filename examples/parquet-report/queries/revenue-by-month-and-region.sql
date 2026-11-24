select strftime(day, '%Y-%m') as month, region, sum(revenue) as revenue
from orders
group by month, region
order by month, region;
